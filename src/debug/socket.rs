use core::ops::DerefMut;

use crate::boot_success::BootSuccessSignaler;
use crate::debug::dap::Dap;
use crate::flash::guard::with_spinlock;
use dap_rs::dap::DapVersion;
use defmt::{debug, trace, unwrap, warn};
use embassy_net::{driver::Driver, tcp::TcpSocket};
use embassy_sync::{blocking_mutex::raw::NoopRawMutex, mutex::Mutex, once_lock::OnceLock};
use embassy_time::Duration;
use embedded_io_async::Write;
use static_cell::StaticCell;

const PACKET_SIZE: usize = dap_rs::usb::DAP2_PACKET_SIZE as usize;

type DebugSocketLock = OnceLock<Mutex<NoopRawMutex, TcpSocket<'static>>>;

static DEBUG_SOCKET: DebugSocketLock = OnceLock::new();

pub struct DebugSocket(&'static DebugSocketLock);

impl DebugSocket {
    pub fn new() -> Self {
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
            .map_err(|_| "Socket already initialized"));
    }
}

#[embassy_executor::task]
pub(crate) async fn listen_task(
    boot_success_signaler: &'static BootSuccessSignaler,
    port: u16,
    timeout: Duration,
) -> ! {
    let mut socket = DEBUG_SOCKET.get().await.lock().await;
    socket.set_timeout(Some(timeout));

    let mut dap = Dap::new_with_leds(boot_success_signaler.dap_leds());

    loop {
        debug!("Waiting for connection");

        if socket.accept(port).await.is_err() {
            warn!("Failed to accept connection");
            continue;
        }

        debug!("Connected");

        with_spinlock(
            |socket| async {
                loop {
                    let mut request_buffer = [0; dap_rs::usb::DAP2_PACKET_SIZE as usize];

                    trace!("Waiting for request");

                    let n = match socket.read(&mut request_buffer).await {
                        Ok(0) => {
                            debug!("Read EOF");
                            break;
                        }
                        Ok(n) => n,
                        Err(e) => {
                            warn!("Read error: {:?}", e);
                            break;
                        }
                    };

                    trace!("Received {} bytes", n);

                    let mut response_buffer = [0; dap_rs::usb::DAP2_PACKET_SIZE as usize];
                    let n = dap.process_command(
                        &request_buffer[..n],
                        &mut response_buffer,
                        DapVersion::V2,
                    );

                    trace!("Responding with {} bytes", n);

                    match socket.write_all(&response_buffer[..n]).await {
                        Ok(()) => {}
                        Err(e) => {
                            warn!("Write error: {:?}", e);
                            break;
                        }
                    };
                }

                dap.suspend();
            },
            socket.deref_mut(),
        )
        .await;

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
