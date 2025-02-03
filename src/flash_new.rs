use core::{
    cell::RefCell,
    future::Future,
    sync::atomic::{fence, Ordering},
};

use defmt::{debug, trace, unwrap, warn, Format};
use embassy_boot_rp::{AlignedBuffer, BlockingFirmwareUpdater, FirmwareUpdaterConfig};
use embassy_embedded_hal::flash::partition::BlockingPartition;
use embassy_rp::{
    flash::{Async, Flash, WRITE_SIZE},
    peripherals::FLASH,
};
use embassy_sync::{
    blocking_mutex::{
        raw::{CriticalSectionRawMutex, NoopRawMutex},
        Mutex,
    },
    once_lock::OnceLock,
    signal::Signal,
};

use crate::spinlock::Spinlock;

#[cfg(feature = "flash-size-2048k")]
pub const FLASH_SIZE: usize = 2048 * 1024;

pub type FlashMutex = Mutex<NoopRawMutex, RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>;
/// This is a cross-core spinlock designed to prevent a deadlock, whereby both cores succeed
/// in simultaneously pausing each other.
///
/// The embassy-rp::flash::Flash methods check they are running on core0 (they will error if
/// invoked from core1) before explicitly pausing core1 via a private spinlock mechanism.
///
/// During debugging, the flash algorithm (proxied via core1) halts core0 via the debug port.
///
/// We can't be sure that the point at which this occurs isn't exactly as core0 has instructed
/// core1 to halt.
///
/// To prevent the deadlock, we acquire this spinlock prior to flash operations from core0 and
/// prior to establishing a debugger connection.
///
/// We do not use a critical section for this as it would severely limit the usefulness of the
/// debugging (i.e. the debugger would deadlock on every critical section).
pub type FlashSpinlock = Spinlock<30>;

/// Guard access to flash to prevent potential deadlock - see [`crate::flash_new::FlashSpinlock`].
pub async fn with_flash<A, F: Future<Output = R>, R>(
    f: impl FnOnce(&FlashMutex, A) -> F,
    args: A,
) -> R {
    let flash = FLASH.try_get().expect("FLASH not initialized");
    let flash_spinlock = loop {
        if let Some(spinlock) = FlashSpinlock::try_claim() {
            break spinlock;
        }
    };
    // Ensure the spinklock is acquired before calling the flash operation
    fence(Ordering::SeqCst);
    let result = f(flash, args).await;
    // Ensure the spinklock is released after calling the flash operation
    fence(Ordering::SeqCst);
    drop(flash_spinlock);
    result
}

static FLASH: OnceLock<FlashMutex> = OnceLock::new();

pub fn init_flash(flash: Flash<'static, FLASH, Async, FLASH_SIZE>) {
    let flash = embassy_sync::blocking_mutex::Mutex::new(RefCell::new(flash));
    unwrap!(FLASH
        .init(flash)
        .map_err(|_| "FLASH already initialised"));
}

pub fn firmware_updater<'a>(
    buffer: &'a mut AlignedBuffer<WRITE_SIZE>,
) -> BlockingFirmwareUpdater<
    'a,
    BlockingPartition<'static, NoopRawMutex, Flash<'static, FLASH, Async, FLASH_SIZE>>,
    BlockingPartition<'static, NoopRawMutex, Flash<'static, FLASH, Async, FLASH_SIZE>>,
> {
    let flash = FLASH.try_get().expect("FLASH not initialized");
    let config = FirmwareUpdaterConfig::from_linkerfile_blocking(flash, flash);
    BlockingFirmwareUpdater::new(config, &mut buffer.0)
}

#[embassy_executor::task]
pub async fn mark_successful_boot_task(signal: &'static Signal<CriticalSectionRawMutex, ()>) {
    signal.wait().await;
    debug!("Marking successful boot");

    let mut state_buffer = AlignedBuffer([0; WRITE_SIZE]);
    let mut updater = firmware_updater(&mut state_buffer);

    match unwrap!(updater.get_state()) {
        embassy_boot_rp::State::Swap => {
            unwrap!(updater.mark_booted());
        }
        _ => {}
    }
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
            let mut updater = firmware_updater(&mut state_buffer);
            updater.mark_updated().map_or_else(
                |e| {
                    warn!("Failed to mark updated: {:?}", e);
                    1
                },
                |_| 0,
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

    let mut state_buffer = AlignedBuffer([0; WRITE_SIZE]);
    let mut updater = firmware_updater(&mut state_buffer);

    updater.write_firmware(address, buffer).map_or_else(
        |e| {
            warn!("Failed to write firmware: {:?}", e);
            1
        },
        |_| 0,
    )
}

extern "C" fn erase_sector(address: usize, _: usize, _: usize) -> usize {
    trace!("Erasing sector at {:#x}", address);
    // erasing is performed as part of proram_page
    0
}
