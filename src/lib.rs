#![no_std]

use cortex_m::asm::nop;
use defmt::unwrap;
use embassy_executor::{Executor, SpawnToken};
use embassy_net::{driver::Driver, tcp::TcpSocket};
use embassy_rp::{
    multicore::{spawn_core1, Stack},
    peripherals::CORE1,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, once_lock::OnceLock};
use static_cell::StaticCell;

type DebugSocketLock = OnceLock<Mutex<NoopRawMutex, TcpSocket<'static>>>;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static DEBUG_SOCKET: DebugSocketLock = OnceLock::new();

#[embassy_executor::task]
async fn core1_task() -> ! {
    let debug_socket = DEBUG_SOCKET.get().await.lock().await;
    loop {
        nop();
    }
}

pub struct Carol(&'static DebugSocketLock);

impl Carol {
    fn new() -> Self {
        Self(&DEBUG_SOCKET)
    }

    pub fn listen<D: Driver>(&self, stack: &'static embassy_net::Stack<D>) {
        static SOCKET_RX_BUFFER: StaticCell<[u8; 1]> = StaticCell::new();
        static SOCKET_TX_BUFFER: StaticCell<[u8; 1]> = StaticCell::new();
        let rx_buffer = SOCKET_RX_BUFFER.init([0; 1]);
        let tx_buffer = SOCKET_TX_BUFFER.init([0; 1]);
        let socket = TcpSocket::new(stack, rx_buffer, tx_buffer);
        unwrap!(self
            .0
            .init(Mutex::new(socket))
            .map_err(|_| "socket already initialized"));
    }
}

pub struct Bob {
    phantom: core::marker::PhantomData<()>,
}
impl Bob {
    pub fn new<ARGS, S>(
        core1: CORE1,
        init_args: ARGS,
        net_init: impl FnOnce(ARGS, Carol) -> SpawnToken<S> + Send + 'static,
    ) -> Self
    where
        ARGS: Send + 'static,
    {
        spawn_core1(
            core1,
            unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
            move || {
                let executor1 = EXECUTOR1.init(Executor::new());
                executor1.run(|spawner| {
                    unwrap!(spawner.spawn(net_init(init_args, Carol::new())));
                    unwrap!(spawner.spawn(core1_task()))
                });
            },
        );
        Self {
            phantom: core::marker::PhantomData,
        }
    }
}
