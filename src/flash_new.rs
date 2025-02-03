use core::{
    cell::RefCell,
    future::Future,
    sync::atomic::{fence, Ordering},
};

use defmt::{trace, warn, Format};
use embassy_boot_rp::{AlignedBuffer, FirmwareUpdaterConfig};
use embassy_embedded_hal::flash::partition::BlockingPartition;
use embassy_rp::{
    flash::{Async, WRITE_SIZE},
    peripherals::FLASH,
};
use embassy_sync::{
    blocking_mutex::{raw::NoopRawMutex, Mutex},
    once_lock::OnceLock,
};

use crate::spinlock::Spinlock;

#[cfg(feature = "flash-size-2048k")]
pub const FLASH_SIZE: usize = 2048 * 1024;

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

/// Guarded access to flash to prevent potential deadlock - see [`crate::flash_new::FlashSpinlock`].
pub async fn with_spinlock<A, F: Future<Output = R>, R>(f: impl FnOnce(A) -> F, args: A) -> R {
    let spinlock = loop {
        if let Some(spinlock) = FlashSpinlock::try_claim() {
            break spinlock;
        }
    };
    // Ensure the spinklock is acquired before calling the flash operation
    fence(Ordering::SeqCst);
    let result = f(args).await;
    // Ensure the spinklock is released after calling the flash operation
    fence(Ordering::SeqCst);
    drop(spinlock);
    result
}

pub type Flash = embassy_rp::flash::Flash<'static, FLASH, Async, FLASH_SIZE>;
pub type FlashMutex = Mutex<NoopRawMutex, RefCell<Flash>>;
pub type BlockingFirmwareUpdater<'a> = embassy_boot_rp::BlockingFirmwareUpdater<
    'a,
    BlockingPartition<'static, NoopRawMutex, Flash>,
    BlockingPartition<'static, NoopRawMutex, Flash>,
>;

static FLASH_NEW: OnceLock<FlashNew> = OnceLock::new();

pub struct FlashNew {
    flash: FlashMutex,
}

impl FlashNew {
    pub fn new(
        flash: Flash,
    ) -> Result<&'static Self, Flash> {
        let instance = Self {
            flash: Mutex::new(RefCell::new(flash)),
        };
        FLASH_NEW
            .init(instance)
            .map_err(|instance| instance.flash.into_inner().into_inner())
            .map(|_| FLASH_NEW.try_get().unwrap())
    }

    pub async fn with_flash<A, F: Future<Output = R>, R>(
        &self,
        func: impl FnOnce(&FlashMutex, A) -> F,
        args: A,
    ) -> R {
        let flash = &self.flash;
        with_spinlock(|_| func(flash, args), ()).await
    }

    fn firmware_updater<'a>(
        &'static self,
        buffer: &'a mut AlignedBuffer<WRITE_SIZE>,
    ) -> BlockingFirmwareUpdater<'a> {
        BlockingFirmwareUpdater::new(
            FirmwareUpdaterConfig::from_linkerfile_blocking(&self.flash, &self.flash),
            &mut buffer.0,
        )
    }

    pub async fn with_firmware_updater<'a, A, F: Future<Output = R>, R>(
        &'static self,
        buffer: &'a mut AlignedBuffer<WRITE_SIZE>,
        func: impl FnOnce(BlockingFirmwareUpdater<'a>, A) -> F,
        args: A,
    ) -> R {
        let firmware_updater = self.firmware_updater(buffer);
        with_spinlock(
            |_| { func(firmware_updater, args) },
            (),
        )
        .await
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
    let Some(flash_new) = FLASH_NEW.try_get() else {
        warn!("Flash not initialized");
        return 2;
    };
    match operation {
        Operation::Program => {
            trace!("Marking updated");
            let mut state_buffer = AlignedBuffer([0; WRITE_SIZE]);
            let mut updater = flash_new.firmware_updater(&mut state_buffer);
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
    let Some(flash_new) = FLASH_NEW.try_get() else {
        warn!("Flash not initialized");
        return 2;
    };
    let mut state_buffer = AlignedBuffer([0; WRITE_SIZE]);
    let mut updater = flash_new.firmware_updater(&mut state_buffer);

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
