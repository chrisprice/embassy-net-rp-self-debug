use core::{
    sync::atomic::{AtomicU8, Ordering},
    mem,
};
use defmt::{info, error};
use embassy_rp::pac as pac;

#[repr(C)]
pub struct Ipc {
    what: AtomicU8, // IpcWhat,
    regs: [u32; 3],
}

impl Ipc {
    const fn new() -> Self {
        Self  {
            what: AtomicU8::new(0),
            regs: [0; 3],
        }
    }

    fn read_what(&self) -> Result<Option<IpcWhat>, u8> {
        let w: u8 = self.what.load(Ordering::Acquire);
        match w {
            0 => Ok(None),
            1 ..= 4 => Ok(Some(unsafe {
                // SAFETY: repr(u8) on IpcWhat
                mem::transmute(w)
            })),
            w => Err(w),
        }
    }
}

#[allow(dead_code)]
#[repr(u8)]
enum IpcWhat {
    Initialise = 1, // anything but zero
    Deinitalise,
    Program,
    Erase,
}

// reserve the memory address for IPC:
#[used]
#[link_section = ".probe_rs_scratch"]
pub static mut IPC: Ipc = Ipc::new();

// TODO: no copy+paste of address
// see https://github.com/embassy-rs/embassy/blob/2537fc6f4fcbdaa0fcea45a37382d61f59cc5767/examples/boot/bootloader/rp/memory.x#L18-L21

pub fn handle_pending_flash() {
    use embassy_rp::rom_data;

    #[allow(static_mut_refs)]
    let ipc = unsafe { &IPC };

    match ipc.read_what() {
        Ok(None) => return,

        Ok(Some(IpcWhat::Initialise)) => {
            info!(
                "found init({:#x}, {:#x}, {:#x}), initialising...",
                ipc.regs[0],
                ipc.regs[1],
                ipc.regs[2],
            );


            unsafe {
                // SAFETY:
                // none known
                rom_data::connect_internal_flash(); // "IF"
                rom_data::flash_exit_xip(); // "EX"
            }

            info!("init done");
        }
        Ok(Some(IpcWhat::Deinitalise)) => {
            info!(
                "found deinit({:#x}), flushing & resoring xip...",
                ipc.regs[0],
            );

            unsafe {
                // SAFETY (TODO):
                // none known
                rom_data::flash_flush_cache(); // "FX"
                rom_data::flash_enter_cmd_xip(); // "CX"
            }

            info!("deinit done");
        }
        Ok(Some(IpcWhat::Program)) => {
            info!(
                "found program_page({:#x}, {:#x}, {:#x}), programming...",
                ipc.regs[0],
                ipc.regs[1],
                ipc.regs[2],
            );

            // count and data are passed reversed, see probe-rs:
            // 0eaed1a2461ca, src/flashing/flasher.rs, L849-L851
            let [addr, count, data] = ipc.regs;

            let addr = flash_map_address(addr);
            let count = count as usize;
            let data = data as *const u8;

            debug_assert!(
                addr as usize % embassy_rp::flash::WRITE_SIZE == 0,
                "buffers must be aligned"
            ); // trivial


            flash_safe(|| {
                unsafe {
                    // SAFETY (TODO):
                    // - interrupts disabled
                    // - 2nd core is running code in ram (flash algo), interrupts also disabled
                    // - DMA is not accessing flash
                    rom_data::flash_range_program(addr, data, count) // "RP"
                }
            });

            info!("program_page done");
        }
        Ok(Some(IpcWhat::Erase)) => {
            info!(
                "found erase_sector({:#x}), erasing...",
                ipc.regs[0],
            );

            let addr = flash_map_address(ipc.regs[0]);
            let (count, block_size, block_cmd) = (0x1000, 0x10000, 0xd8);

            flash_safe(|| {
                unsafe {
                    // SAFETY:
                    // - interrupts disabled
                    // - 2nd core is running code in ram (flash algo), interrupts also disabled
                    // - DMA is not accessing flash
                    rom_data::flash_range_erase(addr, count, block_size, block_cmd) // "RE"
                }
            });

            info!("erase done");
        }
        Err(v) => {
            error!("unknown ipc value {}", v);
        }
    }

    ipc.what.store(0, Ordering::SeqCst);
}

fn flash_map_address(addr: u32) -> u32 {
    // TODO: DFU
    addr - 0x10000000
}

fn flash_safe(cb: impl FnOnce()) {
    assert!(pac::SIO.cpuid().read() == 0, "must be on core0");

    cortex_m::interrupt::free(|_| {
        // TODO: wait for dma to finish

        cb()
    });
}
