use core::{
    cell::RefCell,
    future::Future,
    sync::atomic::{fence, Ordering},
};

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
pub async fn with_spinlock<A, F: Future<Output = R>, R>(func: impl FnOnce(A) -> F, args: A) -> R {
    let spinlock = loop {
        if let Some(spinlock) = FlashSpinlock::try_claim() {
            break spinlock;
        }
    };
    // Ensure the spinklock is acquired before calling the flash operation
    fence(Ordering::SeqCst);
    let result = func(args).await;
    // Ensure the spinklock is released after calling the flash operation
    fence(Ordering::SeqCst);
    drop(spinlock);
    result
}

/// Guarded access to flash to prevent potential deadlock - see [`crate::flash_new::FlashSpinlock`].
fn with_spinlock_blocking<A, R>(func: impl FnOnce(A) -> R, args: A) -> R {
    let spinlock = loop {
        if let Some(spinlock) = FlashSpinlock::try_claim() {
            break spinlock;
        }
    };
    // Ensure the spinklock is acquired before calling the flash operation
    fence(Ordering::SeqCst);
    let result = func(args);
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
    pub fn try_get() -> Option<&'static Self> {
        FLASH_NEW.try_get()
    }

    pub fn new(flash: Flash) -> Result<&'static Self, Flash> {
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

    pub fn with_firmware_updater<'a, A, R>(
        &'static self,
        buffer: &'a mut AlignedBuffer<WRITE_SIZE>,
        func: impl FnOnce(BlockingFirmwareUpdater<'a>, A) -> R,
        args: A,
    ) -> R {
        let firmware_updater = BlockingFirmwareUpdater::new(
            FirmwareUpdaterConfig::from_linkerfile_blocking(&self.flash, &self.flash),
            &mut buffer.0,
        );
        with_spinlock_blocking(|_| func(firmware_updater, args), ())
    }
}
