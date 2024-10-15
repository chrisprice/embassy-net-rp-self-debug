use core::{
    sync::atomic::Ordering
};
use defmt::{info, error};
use embassy_rp::pac as pac;

use crate::flash::{
    ipc::{IPC, IpcWhat},
    thunk::Operation,
};

pub fn handle_pending_flash() {
    use embassy_rp::rom_data;

    #[allow(static_mut_refs)]
    let ipc = unsafe { &IPC };

    match ipc.read_what() {
        Ok(None) => return,

        Ok(Some(IpcWhat::Init)) => {
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
        Ok(Some(IpcWhat::Deinit)) => {
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

            if ipc.regs[0] == Operation::Program as usize {
                // all done, laters
                info!("deinit(Operation::Program) detected, finalising...");
                flash_done();
            }
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

            let addr = flash_map_address(addr as u32);
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

            let addr = flash_map_address(ipc.regs[0] as u32);
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
    extern "C" {
        static __bootloader_active_start: u32;
        static __bootloader_dfu_start: u32;
    }

    // 1. Addresses are given to us relative to memory, we want them relative to flash.
    //    Flash is mapped at 0x10000000, so we subtract that
    // 2. Addresses are for FLASH (memory.x), we want to write into DFU,
    // so add the offset from FLASH to DFU

    let active_start = unsafe { &__bootloader_active_start as *const _ as u32 };
    let dfu_start = unsafe { &__bootloader_dfu_start as *const _ as u32 };
    let dfu_offset = dfu_start - active_start;

    addr - 0x10000000 + dfu_offset
}

fn flash_safe(cb: impl FnOnce()) {
    assert!(pac::SIO.cpuid().read() == 0, "must be on core0");

    cortex_m::interrupt::free(|_| {
        // TODO: wait for dma to finish

        cb()
    });
}

fn flash_done() -> ! {
    use core::cell::RefCell;
    use embassy_sync::blocking_mutex::Mutex;
    use embassy_boot_rp::{AlignedBuffer, FirmwareUpdaterConfig, BlockingFirmwareUpdater};
    use embassy_rp::flash::Flash;

    let p = unsafe { embassy_rp::Peripherals::steal() };

    const FLASH_SIZE: usize = 2 * 1024 * 1024;

    let flash = Flash::<_, _, FLASH_SIZE>::new_blocking(p.FLASH);
    let flash = Mutex::new(RefCell::new(flash));

    let config = FirmwareUpdaterConfig::from_linkerfile_blocking(&flash);

    info!("created FirmwareUpdaterConfig");

    let mut aligned = AlignedBuffer([0; 1]);
    let mut updater = BlockingFirmwareUpdater::new(config, &mut aligned.0);

    // this erases DFU and gives us the writer
    // we don't need this - probe-rs does the erase & write
    //updater.prepare_update();

    info!("marking bootloader state as updated...");
    updater.mark_updated().unwrap(); // sets state parititon, fill to SWAP_MAGIC, i.e. 0xf0

    info!("marked bootloader state as updated");

    // bootloader (already flashed) will now check for 0xf0 (prepare_boot()) and,
    // upon finding all SWAP_MAGICs, indicate it's in State::Swap, do the swap()
    // and boot us. we reset to initiate this:

    info!("resetting...");

    cortex_m::peripheral::SCB::sys_reset()
}
