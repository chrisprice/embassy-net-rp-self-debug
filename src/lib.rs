#![no_std]

pub mod boot_success;
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
    blocking_mutex::{raw::CriticalSectionRawMutex, NoopMutex},
    signal::Signal,
};
use flash::{
    algorithm::FlashAlgorithm,
    spinlock::{with_spinlock, with_spinlock_blocking},
};
use static_cell::StaticCell;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

pub struct State {
    boot_success_signal: Signal<CriticalSectionRawMutex, ()>,
}

impl State {
    pub fn new() -> Self {
        Self {
            boot_success_signal: Signal::new(),
        }
    }
}

// TODO: check type visibility

pub struct OtaDebugger<const FLASH_SIZE: usize> {
    _state: &'static State,
    flash: NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>,
}
impl<const FLASH_SIZE: usize> OtaDebugger<FLASH_SIZE> {
    pub async fn new(
        state: &'static mut State,
        flash: FLASH,
        dma: DMA_CH0,
        core1: CORE1,
        core1_init: impl FnOnce(Spawner, DebugSocket) + Send + 'static,
    ) -> (Self, BootSuccessMarker<FLASH_SIZE>) {
        let (flash_algorithm, flash) = FlashAlgorithm::new(flash, dma);

        // By accepting the singleton CORE1 peripheral we're ensuring that this function isn't called twice.
        // Therefore we're not going to overwrite any existing algorithm.
        flash_algorithm.install();

        let instance = Self {
            _state: state,
            flash,
        };

        let boot_success_signaler = BootSuccessSignaler::new(&instance._state.boot_success_signal);
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

        let boot_success_marker = BootSuccessMarker::new(&instance._state.boot_success_signal);
        (instance, boot_success_marker)
    }

    pub async fn with_flash<A, F: Future<Output = R>, R>(
        &self,
        func: impl FnOnce(&NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>, A) -> F,
        args: A,
    ) -> R {
        with_spinlock(|()| func(&self.flash, args), ()).await
    }

    // TODO: Remove args, only async needs that (due to lack of async closure)
    pub fn with_flash_blocking<A, R>(
        &self,
        func: impl FnOnce(&NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>, A) -> R,
        args: A,
    ) -> R {
        with_spinlock_blocking(|()| func(&self.flash, args), ())
    }
}
