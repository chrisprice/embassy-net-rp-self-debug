use core::sync::atomic::Ordering;
use defmt::{error, info};
use embassy_rp::{
    flash::{Async, Flash, ERASE_SIZE},
    peripherals::FLASH,
    rom_data,
    watchdog::Watchdog,
};
use embassy_time::Duration;

use crate::{
    flash::ipc::{IpcWhat, IPC},
    FLASH_SIZE,
};

pub fn handle_pending_flash(flash: &mut Flash<'static, FLASH, Async, FLASH_SIZE>) {
    #[allow(static_mut_refs)]
    let ipc = unsafe { &IPC };

    match ipc.read_what() {
        Ok(None) => return,

        Ok(Some(IpcWhat::Program)) => {
            info!(
                "found program_page({:#x}, {:#x}, {:#x}), programming...",
                ipc.regs[0], ipc.regs[1], ipc.regs[2],
            );

            #[cfg(not(feature = "flash-dry-run"))]
            {
                let [addr, count, data] = ipc.regs;

                let addr = flash_map_address(addr as u32);
                let count = count as usize;
                let data = data as *const u8;

                let data = unsafe { core::slice::from_raw_parts(data, count) };
                info!("programming {:#x} to {:#x}", addr, addr + count as u32);
                flash.blocking_write(addr, data).unwrap();
            }

            info!("program_page done");
        }
        Ok(Some(IpcWhat::Erase)) => {
            info!("found erase_sector({:#x}), erasing...", ipc.regs[0],);

            #[cfg(not(feature = "flash-dry-run"))]
            {
                let from = flash_map_address(ipc.regs[0] as u32);
                let to = from + ERASE_SIZE as u32; // TODO: this is way less than we need
                info!("erasing {:#x} to {:#x}", from, to);
                flash.blocking_erase(from, to).unwrap();
            }

            info!("erase done");
        }
        Err(v) => {
            error!("unknown ipc value {}", v);
        }
    }

    ipc.what.store(0, Ordering::SeqCst);
}

#[cfg(not(feature = "flash-dry-run"))]
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


fn flash_done() {
    use core::cell::RefCell;
    use embassy_boot_rp::{AlignedBuffer, BlockingFirmwareUpdater, FirmwareUpdaterConfig};
    use embassy_rp::flash::Flash;
    use embassy_sync::blocking_mutex::Mutex;

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
    // updater.prepare_update().unwrap();

    info!("marking bootloader state as updated...");
    // updater.mark_updated().unwrap(); // sets state parititon, fill to SWAP_MAGIC, i.e. 0xf0

    info!("marked bootloader state as updated");

    // bootloader (already flashed) will now check for 0xf0 (prepare_boot()) and,
    // upon finding all SWAP_MAGICs, indicate it's in State::Swap, do the swap()
    // and boot us. we reset to initiate this:

    info!("scheduling reset for 10 sec... (in a very cheap way)");
    Watchdog::new(p.WATCHDOG).start(Duration::from_millis(8000));

    // rom_data::reset_to_usb_boot(0, 1 | 2);
}
