#![no_std]

pub mod debug;
mod flash;

use core::{cell::RefCell, ops::{Deref, DerefMut}};

use debug::socket::DebugSocket;
use embassy_boot_rp::{AlignedBuffer, FirmwareUpdaterConfig};
use embassy_embedded_hal::flash::partition::BlockingPartition;
use embassy_executor::{Executor, Spawner};
use embassy_rp::{
    flash::{Async, Flash, WRITE_SIZE},
    multicore::{spawn_core1, Stack},
    peripherals::{CORE1, DMA_CH0, FLASH},
};
use embassy_sync::{
    blocking_mutex::{
        raw::{CriticalSectionRawMutex, NoopRawMutex},
        NoopMutex,
    },
    mutex::Mutex,
};
use flash::{algorithm::FlashAlgorithm, spinlock::with_spinlock};
use static_cell::StaticCell;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

pub struct State<const FLASH_SIZE: usize> {
    flash: Mutex<
        CriticalSectionRawMutex,
        NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>,
    >,
}

impl<const FLASH_SIZE: usize> State<FLASH_SIZE> {
    pub fn new(flash: FLASH, dma: DMA_CH0) -> Self {
        Self {
            flash: Mutex::new(NoopMutex::new(RefCell::new(Flash::new(flash, dma)))),
        }
    }
}

pub struct OtaDebugger<const FLASH_SIZE: usize> {
    _state: &'static State<FLASH_SIZE>,
}
impl<const FLASH_SIZE: usize> OtaDebugger<FLASH_SIZE> {
    pub async fn new(
        state: &'static mut State<FLASH_SIZE>,
        core1: CORE1,
        core1_init: impl FnOnce(Spawner, DebugSocket) + Send + 'static,
    ) -> Self {
        let instance = Self { _state: state };

        // By accepting the singleton CORE1 peripheral we're ensuring that this function isn't called twice.
        // Therefore we're not going to overwrite any existing algorithm.
        FlashAlgorithm::install(&instance._state.flash);

        spawn_core1(
            core1,
            unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
            move || {
                let executor1 = EXECUTOR1.init(Executor::new());
                executor1.run(|spawner| {
                    core1_init(spawner, DebugSocket::new());
                });
            },
        );

        instance
    }

    /// Whilst this function is async, the underlying Flash instance is wrapped in a blocking
    /// mutex to allow compatability with the flash algorithm (which currently runs without an
    /// async executor).
    pub async fn with_flash_blocking<R>(
        &self,
        func: impl FnOnce(&mut Flash<'static, FLASH, Async, FLASH_SIZE>) -> R,
    ) -> R {
        with_spinlock(
            |_| async {
                let flash = self._state.flash.lock().await;
                flash.lock(|flash| func(flash.borrow_mut().deref_mut()))
            },
            (),
        )
        .await
    }

    pub async fn with_firmware_updater_blocking<R>(
        &self,
        func: impl for<'updater, 'mutex> FnOnce(
            &'updater mut embassy_boot_rp::BlockingFirmwareUpdater<
                BlockingPartition<'mutex, NoopRawMutex, Flash<'static, FLASH, Async, FLASH_SIZE>>,
                BlockingPartition<'mutex, NoopRawMutex, Flash<'static, FLASH, Async, FLASH_SIZE>>,
            >,
        ) -> R,
    ) -> R {
        with_spinlock(
            |_| async {
                let mut buffer = AlignedBuffer([0; WRITE_SIZE]);
                let flash = self._state.flash.lock().await;
                let mut firmware_updater = embassy_boot_rp::BlockingFirmwareUpdater::new(
                    FirmwareUpdaterConfig::from_linkerfile_blocking(flash.deref(), flash.deref()),
                    &mut buffer.0,
                );
                func(&mut firmware_updater)
            },
            (),
        )
        .await
    }
}
