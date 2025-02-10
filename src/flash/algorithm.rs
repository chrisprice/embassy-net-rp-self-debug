use core::{
    cell::RefCell,
    ops::Deref,
    sync::atomic::{AtomicBool, Ordering},
};

use defmt::{trace, unwrap, warn, Format};
use embassy_boot_rp::{AlignedBuffer, FirmwareUpdaterConfig};
use embassy_embedded_hal::flash::partition::BlockingPartition;
use embassy_rp::{
    flash::{Async, Flash, WRITE_SIZE},
    peripherals::FLASH,
};
use embassy_sync::blocking_mutex::{
    raw::{CriticalSectionRawMutex, NoopRawMutex},
    NoopMutex,
};
use embassy_sync::mutex::Mutex;

/// A flag to indicate the flash algorithm has been initialised.
pub(crate) static INIT_CALLED: AtomicBool = AtomicBool::new(false);

/// The pertinent parts of the probe-rs config are included below,
/// the associated code statically asserts the derivation of the
/// various magic numbers contained within.
///
/// ```yaml
///  instructions: +kwF4PpMA+D6TAHg+kz/5wC1oEcAvQ==
///  load_address: 0x20000004
///  stack_size: 512
///  pc_init: 0x1
///  pc_uninit: 0x5
///  pc_program_page: 0x9
///  pc_erase_sector: 0xd
/// ```
///
/// `instructions` is a base64 encoding of the following ARM assembly -
/// ```asm
/// ldr r4, [pc, #0x3e8]
/// b #0x10
/// ldr r4, [pc, #0x3e8]
/// b #0x10
/// ldr r4, [pc, #0x3e8]
/// b #0x10
/// ldr r4, [pc, #0x3e8]
/// b #0x10
/// push {lr}
/// blx r4
/// pop {pc}
/// ```
const _: () = {
    type ProbeRsArmHeader = [u32; 1];
    // For reasons*, probe-rs prefixes the instructions with a header but loads
    // the instructions at the load address. Therefore, we offset the load address
    // by the size of the header to ensure the instructions will fit in the
    // reserved space.
    // * - When probe-rs jumps to the instructions, it initialises the link
    // register with the address of the first header instruction. This allows the
    // code to jump back to the header instructions after they are complete. The
    // header instructions themselves cause the core to halt, which is detected by
    // probe-rs, signalling that the flash algorithm has completed.
    let _probe_rs_arm_header: ProbeRsArmHeader = [0xBE00BE00];
    let probe_rs_arm_header_size = core::mem::size_of::<ProbeRsArmHeader>();
    let load_address = RESERVED_BASE_ADDRESS + probe_rs_arm_header_size;
    assert!(load_address == 0x20000004);
    // Probe-rs will place the stack immediately after the instructions. To keep
    // things simple, we choose the end of the reserved space to store the
    // function table.
    let stack_size = 512;
    let thumb_instruction_size = 2;
    let instruction_count = 11;
    let instructions_size = thumb_instruction_size * instruction_count;
    assert!(
        probe_rs_arm_header_size + instructions_size + stack_size + TABLE_SIZE <= RESERVED_SIZE
    );
    // Additionally, the layout of the entry points is such that the relative
    // offset to their respective function table entry is the same for all entry
    // points (i.e. each entry point is 4 bytes apart). The kicker is that these
    // are thumb-mode instructions, so "the value of the PC is the address of the
    // current instruction plus 4 bytes."
    let pc_relative_offset = 4;
    let lookup_delta = RESERVED_SIZE - probe_rs_arm_header_size - TABLE_SIZE - pc_relative_offset;
    assert!(lookup_delta == 0x3e8);
};

/// The base address of the RAM region reserved for the flash algorithm.
const RESERVED_BASE_ADDRESS: usize = 0x20000000;
/// The size of the RAM region reserved for the flash algorithm.
const RESERVED_SIZE: usize = 1024;
/// The size of the function table used to store the pointers.
const TABLE_SIZE: usize = size_of::<[extern "C" fn(usize, usize, usize) -> usize; 4]>();
/// The location of the function table in the reserved RAM region.
const TABLE_BASE_ADDRESS: usize = RESERVED_BASE_ADDRESS + RESERVED_SIZE - TABLE_SIZE;

#[derive(Format)]
pub enum Operation {
    Erase,
    Program,
    Verify,
}

impl core::convert::TryFrom<usize> for Operation {
    type Error = ();
    fn try_from(v: usize) -> Result<Self, Self::Error> {
        match v {
            1 => Ok(Self::Erase),
            2 => Ok(Self::Program),
            3 => Ok(Self::Verify),
            _ => Err(()),
        }
    }
}

/// Used to share the flash instance with the flash algorithm.
/// Can't be correctly typed because Flash is generic over the flash size.
static mut FLASH_POINTER: usize = 0;

/// Flash algorithm methods using a const generic for reuse across flash sizes.
/// These methods must be statically invoked (via the function table), therefore
/// we steal the required peripherals when constructing the flash instance.
/// To ensure that it is safe to do so we guard access to flash with the spinlock.
/// To ensure we steal the right peripherals we construct the flash instance here.
pub struct FlashAlgorithm<const FLASH_SIZE: usize> {}

impl<const FLASH_SIZE: usize> FlashAlgorithm<FLASH_SIZE> {
    pub fn install(
        flash: &'static Mutex<
            CriticalSectionRawMutex,
            NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>,
        >,
    ) {
        let function_table: [extern "C" fn(usize, usize, usize) -> usize; 4] = [
            Self::init,
            Self::uninit,
            Self::program_page,
            Self::erase_sector,
        ];
        debug_assert_eq!(core::mem::size_of_val(&function_table), TABLE_SIZE);
        // SAFETY: These memory locations are reserved for the flash algorithm.
        // SAFETY: All values are pointers to static items.
        unsafe {
            core::ptr::copy_nonoverlapping(
                function_table.as_ptr(),
                TABLE_BASE_ADDRESS as *mut _,
                function_table.len(),
            );
            FLASH_POINTER = flash as *const _ as usize;
        }
    }

    fn flash() -> &'static Mutex<
        CriticalSectionRawMutex,
        NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>,
    > {
        // SAFETY: Must be invoked after the flash algorithm has been installed.
        // SAFETY: Install must only be invoked once (such that the flash pointer
        // and function table share the same generics).
        unsafe { &*(FLASH_POINTER as *const _) }
    }

    /// Retrieves flash from the mutex and invokes the provided function.
    /// This function will panic if the mutex can not be acquired. It is
    /// expected that access to this function is guarded by the spinlock.
    fn with_firmware_updater<R>(
        // buffer: &'buffer mut AlignedBuffer<WRITE_SIZE>,
        func: impl for<'updater, 'mutex> FnOnce(
            &'updater mut embassy_boot_rp::BlockingFirmwareUpdater<
                BlockingPartition<'mutex, NoopRawMutex, Flash<'static, FLASH, Async, FLASH_SIZE>>,
                BlockingPartition<'mutex, NoopRawMutex, Flash<'static, FLASH, Async, FLASH_SIZE>>,
            >,
        ) -> R,
    ) -> R {
        let flash = unwrap!(Self::flash()
            .try_lock()
            .map_err(|_| "Failed to acquire flash mutex"));
        let mut buffer = AlignedBuffer([0; WRITE_SIZE]);
        let mut firmware_updater = embassy_boot_rp::BlockingFirmwareUpdater::new(
            FirmwareUpdaterConfig::from_linkerfile_blocking(flash.deref(), flash.deref()),
            &mut buffer.0,
        );
        func(&mut firmware_updater)
    }

    extern "C" fn init(address: usize, _clock_or_zero: usize, operation: usize) -> usize {
        INIT_CALLED.store(true, Ordering::SeqCst);
        match Operation::try_from(operation) {
            Ok(operation) => {
                trace!("Init: {:#x}, {:?}", address, operation);
                0
            }
            Err(_) => 1,
        }
    }

    extern "C" fn uninit(operation: usize, _: usize, _: usize) -> usize {
        let Ok(operation) = Operation::try_from(operation) else {
            return 1;
        };
        trace!("Uninit: {:?}", operation);
        match operation {
            Operation::Program => {
                trace!("Marking updated");
                Self::with_firmware_updater(|updater| {
                    updater.mark_updated().map_or_else(
                        |e| {
                            warn!("Failed to mark updated: {:?}", e);
                            1
                        },
                        |_| 0,
                    )
                })
            }
            _ => 0,
        }
    }

    extern "C" fn program_page(address: usize, count: usize, buffer: usize) -> usize {
        let address = address - embassy_rp::flash::FLASH_BASE as usize;
        let buffer = buffer as *const u8;
        let buffer = unsafe { core::slice::from_raw_parts(buffer, count) };

        trace!(
            "Programming {:#x} to {:#x}",
            address,
            address + count
        );
        Self::with_firmware_updater(|updater| {
            updater.write_firmware(address, buffer).map_or_else(
                |e| {
                    warn!("Failed to write firmware: {:?}", e);
                    1
                },
                |_| 0,
            )
        })
    }

    extern "C" fn erase_sector(address: usize, _: usize, _: usize) -> usize {
        trace!("Erasing sector at {:#x}", address);
        // erasing is performed as part of program_page
        0
    }
}
