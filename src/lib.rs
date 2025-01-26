#![no_std]

mod dap;

use dap::Dap;
use dap_rs::dap::DapVersion;
use defmt::{debug, trace, unwrap, warn};
use embassy_executor::{Executor, SpawnToken};
use embassy_net::{driver::Driver, tcp::TcpSocket};
use embassy_rp::{
    multicore::{spawn_core1, Stack},
    peripherals::CORE1,
};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, once_lock::OnceLock};
use embedded_io_async::Write;
use static_cell::StaticCell;


const PACKET_SIZE: usize = dap_rs::usb::DAP2_PACKET_SIZE as usize;
type DebugSocketLock = OnceLock<Mutex<NoopRawMutex, TcpSocket<'static>>>;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static DEBUG_SOCKET: DebugSocketLock = OnceLock::new();

#[embassy_executor::task]
async fn debug_listen_task(port: u16) -> ! {
    let mut dap = Dap::new();
    let mut socket = DEBUG_SOCKET.get().await.lock().await;
    loop {
        debug!("Waiting for connection");

        if socket.accept(port).await.is_err() {
            warn!("Failed to accept connection");
            continue;
        }

        debug!("Connected");

        loop {
            let mut request_buffer = [0; dap_rs::usb::DAP2_PACKET_SIZE as usize];

            trace!("Waiting for request");

            let n = match socket.read(&mut request_buffer).await {
                Ok(0) => {
                    debug!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    warn!("read error: {:?}", e);
                    break;
                }
            };

            trace!("Received {} bytes", n);

            let mut response_buffer = [0; dap_rs::usb::DAP2_PACKET_SIZE as usize];
            let n = dap.process_command(&request_buffer[..n], &mut response_buffer, DapVersion::V2);

            trace!("Responding with {} bytes", n);

            match socket.write_all(&response_buffer[..n]).await {
                Ok(()) => {}
                Err(e) => {
                    warn!("write error: {:?}", e);
                    break;
                }
            };
        }

        dap.suspend();
        socket.abort();

        if let embassy_net::tcp::State::CloseWait = socket.state() {
            let _ = socket.flush().await;
        }

        let embassy_net::tcp::State::Closed = socket.state() else {
            panic!("Failed to close connection");
        };

        debug!("Connection closed");
    }
}

pub struct Carol(&'static DebugSocketLock);

impl Carol {
    fn new() -> Self {
        Self(&DEBUG_SOCKET)
    }

    pub fn listen<D: Driver>(&self, stack: &'static embassy_net::Stack<D>) {
        static SOCKET_RX_BUFFER: StaticCell<[u8; PACKET_SIZE]> = StaticCell::new();
        static SOCKET_TX_BUFFER: StaticCell<[u8; PACKET_SIZE]> = StaticCell::new();
        let rx_buffer = SOCKET_RX_BUFFER.init([0; PACKET_SIZE]);
        let tx_buffer = SOCKET_TX_BUFFER.init([0; PACKET_SIZE]);
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
        port: u16,
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
                    unwrap!(spawner.spawn(debug_listen_task(port)))
                });
            },
        );
        Self {
            phantom: core::marker::PhantomData,
        }
    }
}
