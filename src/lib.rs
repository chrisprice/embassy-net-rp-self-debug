#![no_std]

mod boot_success;
pub mod debug;
mod flash;

use core::future::Future;

use boot_success::{mark_booted_task, BootSuccessSignaler};
use debug::socket::{listen_task, DebugSocket};
use embassy_executor::{Executor, SpawnToken, Spawner};
use embassy_rp::{
    flash::{Async, Flash},
    multicore::{spawn_core1, Stack},
    peripherals::{CORE1, FLASH},
};
use embassy_time::Duration;
use flash::guard::{FlashGuard, FlashMutex, FLASH_SIZE};
use static_cell::StaticCell;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

pub struct Bob {
    flash: &'static FlashGuard,
}
impl Bob {
    pub async fn new<ARGS, S>(
        core1: CORE1,
        flash: Flash<'static, FLASH, Async, { FLASH_SIZE }>,
        init_args: ARGS,
        net_init: impl FnOnce(ARGS, DebugSocket) -> SpawnToken<S> + Send + 'static,
        port: u16,
        timeout: Duration,
    ) -> Self
    where
        ARGS: Send + 'static,
    {
        flash::algo::write_function_table();

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
                    spawner.must_spawn(net_init(init_args, DebugSocket::new()));
                    spawner.must_spawn(listen_task(boot_success_signaler, port, timeout))
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
