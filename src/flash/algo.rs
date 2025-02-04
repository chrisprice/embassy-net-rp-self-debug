use core::cell::RefCell;

use defmt::{trace, warn, Format};
use embassy_boot_rp::{AlignedBuffer, FirmwareUpdaterConfig};
use embassy_rp::{
    flash::{Async, Flash, WRITE_SIZE},
    peripherals::FLASH,
    Peripherals,
};
use embassy_sync::{
    blocking_mutex::{raw::NoopRawMutex, Mutex},
    once_lock::OnceLock,
};

use crate::FLASH_SIZE;

use super::{guard::BlockingFirmwareUpdater, spinlock::with_spinlock_blocking};

/// Together with the
/// ```yaml
///  instructions: +kwF4PpMA+D6TAHg+kz/5wC1oEcAvQ==
///  load_address: 0x20000004
///  pc_init: 0x1
///  pc_uninit: 0x5
///  pc_program_page: 0x9
///  pc_erase_sector: 0xd
/// ```
/// The instructions decode as -
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
///
/// ```
/// const PROBE_RS_ARM_HEADER: [u32; 1] = [0xBE00BE00];
/// let load_address = RESERVED_BASE_ADDRESS + core::mem::size_of(PROBE_RS_ARM_HEADER);
/// assert_eq!(load_address, 0x20000004);
/// const TABLE_SIZE: usize = core::mem::size_of(FUNCTION_TABLE);
/// let lookup_delta = RESERVED_SIZE - core::mem::size_of(PROBE_RS_ARM_HEADER) - TABLE_SIZE;
/// assert_eq!(lookup_delta, 0x3e8);
/// ```
static FUNCTION_TABLE: [extern "C" fn(usize, usize, usize) -> usize; 4] =
    [init, uninit, program_page, erase_sector];

/// The base address of the RAM region reserved for the flash algorithm.
/// Must align with the configuration of the probe-rs target.
const RESERVED_BASE_ADDRESS: usize = 0x20000000;
/// The base address of the RAM region reserved for the flash algorithm.
/// Must align with the configuration of the probe-rs target.
const RESERVED_SIZE: usize = 1024;

pub fn write_function_table() {
    let entry_size = size_of::<extern "C" fn(usize, usize, usize) -> usize>();
    let table_size = entry_size * FUNCTION_TABLE.len();
    // Place the function table at the end of the reserved RAM region
    let base_address: usize = RESERVED_BASE_ADDRESS + RESERVED_SIZE - table_size;
    unsafe {
        core::ptr::copy_nonoverlapping(
            FUNCTION_TABLE.as_ptr(),
            base_address as *mut _,
            FUNCTION_TABLE.len(),
        );
    }
}

fn with_firmware_updater<'a, R>(
    buffer: &'a mut AlignedBuffer<WRITE_SIZE>,
    func: impl FnOnce(BlockingFirmwareUpdater<'a>) -> R,
) -> R {
    with_spinlock_blocking(
        |_| {
            static FLASH: OnceLock<
                Mutex<NoopRawMutex, RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>,
            > = OnceLock::new();
            let flash = FLASH.get_or_init(|| {
                // SAFETY: Non-stolen flash access is guarded by the spinlock
                // SAFETY: OtaDebugger explicitly requires DMA0 for flash
                let (flash, dma) = unsafe {
                    let p = Peripherals::steal();
                    (p.FLASH, p.DMA_CH0)
                };
                let flash: Flash<'_, _, _, FLASH_SIZE> = Flash::new(flash, dma);
                Mutex::<NoopRawMutex, _>::new(RefCell::new(flash))
            });
            let firmware_updater = BlockingFirmwareUpdater::new(
                FirmwareUpdaterConfig::from_linkerfile_blocking(&flash, &flash),
                &mut buffer.0,
            );
            func(firmware_updater)
        },
        (),
    )
}

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

extern "C" fn init(address: usize, _clock_or_zero: usize, operation: usize) -> usize {
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
            let mut state_buffer = AlignedBuffer([0; WRITE_SIZE]);
            with_firmware_updater(&mut state_buffer, |mut updater| {
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
        address + count as usize
    );
    let mut state_buffer = AlignedBuffer([0; WRITE_SIZE]);
    with_firmware_updater(&mut state_buffer, |mut updater| {
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
