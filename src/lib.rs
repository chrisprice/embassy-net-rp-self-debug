#![no_std]

mod boot_success;
pub mod debug;
mod flash;

use core::{cell::RefCell, future::Future};

use boot_success::{BootSuccessMarker, BootSuccessSignaler};
use debug::socket::DebugSocket;
use embassy_executor::{Executor, Spawner};
use embassy_rp::{
    flash::{Async, Flash},
    multicore::{spawn_core1, Stack},
    peripherals::{CORE1, DMA_CH0, FLASH},
};
use embassy_sync::{
    blocking_mutex::{
        raw::CriticalSectionRawMutex,
        NoopMutex,
    },
    signal::Signal,
};
use flash::{
    algorithm::FlashAlgorithm,
    spinlock::with_spinlock,
};
use static_cell::StaticCell;

#[cfg(feature = "flash-size-2048k")]
pub const FLASH_SIZE: usize = 2048 * 1024;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

pub struct State<const FLASH_SIZE: usize> {
    flash: NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>,
    boot_success_signal: Signal<CriticalSectionRawMutex, ()>,
}

impl<const FLASH_SIZE: usize> State<FLASH_SIZE> {
    pub fn new(flash: FLASH, dma: DMA_CH0) -> Self {
        let flash = Flash::new(flash, dma);

        Self {
            flash: NoopMutex::new(RefCell::new(flash)),
            boot_success_signal: Signal::new(),
        }
    }
}

pub struct OtaDebugger<const FLASH_SIZE: usize> {
    state: &'static State<FLASH_SIZE>,
}
impl<const FLASH_SIZE: usize> OtaDebugger<FLASH_SIZE> {
    pub async fn new(
        state: &'static mut State<FLASH_SIZE>,
        core1: CORE1,
        core1_init: impl FnOnce(Spawner, DebugSocket) + Send + 'static,
    ) -> (Self, BootSuccessMarker<FLASH_SIZE>) {
        FlashAlgorithm::new(&state.flash);

        let spawner = Spawner::for_current_executor().await;

        let boot_success_signaler = BootSuccessSignaler::new(&state.boot_success_signal);

        spawn_core1(
            core1,
            unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
            move || {
                let executor1 = EXECUTOR1.init(Executor::new());
                executor1.run(|spawner| {
                    core1_init(spawner, DebugSocket::new(boot_success_signaler));
                });
            },
        );
        (
            Self { state },
            BootSuccessMarker::new(&state.flash, &state.boot_success_signal),
        )
    }

    pub async fn with_flash<A, F: Future<Output = R>, R>(
        &self,
        func: impl FnOnce(&NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>, A) -> F,
        args: A,
    ) -> R {
        with_spinlock(|()| func(&self.state.flash, args), ()).await
    }
}
