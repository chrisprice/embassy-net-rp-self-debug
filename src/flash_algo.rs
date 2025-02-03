use defmt::{trace, warn, Format};
use embassy_boot_rp::AlignedBuffer;
use embassy_rp::flash::WRITE_SIZE;

use crate::flash_new::FlashNew;

static ALGO_THUNK: [extern "C" fn(usize, usize, usize) -> usize; 4] =
    [init, uninit, program_page, erase_sector];

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
    let Some(flash_new) = FlashNew::try_get() else {
        warn!("Flash not initialized");
        return 2;
    };
    match operation {
        Operation::Program => {
            trace!("Marking updated");
            let mut state_buffer = AlignedBuffer([0; WRITE_SIZE]);
            flash_new.with_firmware_updater(
                &mut state_buffer,
                |mut updater, _| {
                    updater.mark_updated().map_or_else(
                        |e| {
                            warn!("Failed to mark updated: {:?}", e);
                            1
                        },
                        |_| 0,
                    )
                },
                (),
            )
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
    let Some(flash_new) = FlashNew::try_get() else {
        warn!("Flash not initialized");
        return 2;
    };
    let mut state_buffer = AlignedBuffer([0; WRITE_SIZE]);
    flash_new.with_firmware_updater(
        &mut state_buffer,
        |mut updater, _| {
            updater.write_firmware(address, buffer).map_or_else(
                |e| {
                    warn!("Failed to write firmware: {:?}", e);
                    1
                },
                |_| 0,
            )
        },
        (),
    )
}

extern "C" fn erase_sector(address: usize, _: usize, _: usize) -> usize {
    trace!("Erasing sector at {:#x}", address);
    // erasing is performed as part of proram_page
    0
}
