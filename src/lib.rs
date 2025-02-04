#![no_std]

mod boot_success;
pub mod debug;
mod flash;

use core::future::Future;

use boot_success::{mark_booted_task, BootSuccessSignaler};
use debug::socket::DebugSocket;
use embassy_executor::{Executor, Spawner};
use embassy_rp::{
    flash::Flash,
    multicore::{spawn_core1, Stack},
    peripherals::{CORE1, DMA_CH0, FLASH},
};
use flash::guard::{FlashGuard, FlashMutex};
use static_cell::StaticCell;

#[cfg(feature = "flash-size-2048k")]
pub const FLASH_SIZE: usize = 2048 * 1024;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

pub struct OtaDebugger {
    flash: &'static FlashGuard,
}
impl OtaDebugger {
    pub async fn new<const FLASH_SIZE: usize>(
        core1: CORE1,
        flash: FLASH,
        dma: DMA_CH0,
        core1_init: impl FnOnce(Spawner, DebugSocket) + Send + 'static,
    ) -> Self {
        flash::algo::write_function_table();

        let flash = Flash::new(flash, dma);

        let flash_new = FlashGuard::new(flash)
            .map_err(|_| "Flash already initialised")
            .unwrap(); // TODO: handle error

        let spawner = Spawner::for_current_executor().await;

        let boot_success_signaler = BootSuccessSignaler::new();

        spawner.must_spawn(mark_booted_task(flash_new, boot_success_signaler));

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
        Self { flash: flash_new }
    }

    pub async fn with_flash<A, F: Future<Output = R>, R>(
        &self,
        func: impl FnOnce(&FlashMutex, A) -> F,
        args: A,
    ) -> R {
        self.flash.with_flash(func, args).await
    }
}
