use core::{cell::RefCell, future::Future};

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

use crate::FLASH_SIZE;

use super::spinlock::{with_spinlock, with_spinlock_blocking};

pub type Flash = embassy_rp::flash::Flash<'static, FLASH, Async, FLASH_SIZE>;
pub type FlashMutex = Mutex<NoopRawMutex, RefCell<Flash>>;
pub type BlockingFirmwareUpdater<'a> = embassy_boot_rp::BlockingFirmwareUpdater<
    'a,
    BlockingPartition<'static, NoopRawMutex, Flash>,
    BlockingPartition<'static, NoopRawMutex, Flash>,
>;

static FLASH_GUARD: OnceLock<FlashGuard> = OnceLock::new();

pub struct FlashGuard {
    flash: FlashMutex,
}

impl FlashGuard {
    pub fn new(flash: Flash) -> Result<&'static Self, Flash> {
        let instance = Self {
            flash: Mutex::new(RefCell::new(flash)),
        };
        FLASH_GUARD
            .init(instance)
            .map_err(|instance| instance.flash.into_inner().into_inner())
            .map(|_| FLASH_GUARD.try_get().unwrap())
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
