use core::sync::atomic::Ordering;
use defmt::{error, info};
use embassy_boot_rp::BlockingFirmwareUpdater;
use embassy_embedded_hal::flash::partition::BlockingPartition;
use embassy_rp::{
    flash::{Async, Flash},
    peripherals::FLASH,
    watchdog::Watchdog,
};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_time::Duration;

use crate::{
    flash::{
        ipc::{IpcWhat, IPC},
        thunk::Operation,
    },
    FLASH_SIZE,
};

pub fn handle_pending_flash<'a>(
    firmware_updater: &mut BlockingFirmwareUpdater<
        'a,
        BlockingPartition<'a, NoopRawMutex, Flash<'a, FLASH, Async, FLASH_SIZE>>,
        BlockingPartition<'a, NoopRawMutex, Flash<'a, FLASH, Async, FLASH_SIZE>>,
    >,
) {
    #[allow(static_mut_refs)]
    let ipc = unsafe { &IPC };

    match ipc.read_what() {
        Ok(None) => return,
        Ok(Some(IpcWhat::Init)) => {
            info!(
                "found init({:#x}, {:#x}, {:#x}), initialising...",
                ipc.regs[0], ipc.regs[1], ipc.regs[2],
            );

            if ipc.regs[2] == Operation::Program as usize {
                // avoid BadState
                firmware_updater.mark_booted().unwrap();
            }

            info!("init done");
        }
        Ok(Some(IpcWhat::Deinit)) => {
            info!("found deinit({:#x}), deinitialising...", ipc.regs[0],);

            info!("deinit done");
            if ipc.regs[0] == Operation::Program as usize {
                // all done, laters
                info!("deinit(Operation::Program) detected, finalising...");
                firmware_updater.mark_updated().unwrap();
                info!("marked bootloader state as updated");
                // SAFETY: YOLO
                let p = unsafe { embassy_rp::Peripherals::steal() };
                Watchdog::new(p.WATCHDOG).start(Duration::from_millis(1000));
                info!("scheduled reset for 1 sec...");
            }
        }
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
                firmware_updater
                    .write_firmware(addr as usize, data)
                    .unwrap();
            }

            info!("program_page done");
        }
        Ok(Some(IpcWhat::Erase)) => {
            info!("found erase_sector({:#x}), erasing...", ipc.regs[0],);

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
    }

    // 1. Addresses are given to us relative to memory, we want them relative to flash.
    //    Flash is mapped at 0x10000000, so we subtract that
    // 2. Addresses will include an offset for the bootloader, we subtract that too

    let active_start = unsafe { &__bootloader_active_start as *const _ as u32 };
    addr - 0x10000000 - active_start
}
