use core::sync::atomic::Ordering;

use crate::debug::dap::Dap;
use crate::debug::status::DebugStatus;
use crate::flash::algorithm::INIT_CALLED;
use crate::flash::spinlock::with_spinlock;
use cortex_m::asm::nop;
use dap_rs::dap::DapVersion;
use defmt::{debug, trace, warn};
use embassy_net::{driver::Driver, tcp::TcpSocket};
use embassy_rp::watchdog::Watchdog;
use embassy_rp::Peripherals;
use embassy_time::Duration;
use embedded_io_async::Write;
use static_cell::StaticCell;

const PACKET_SIZE: usize = dap_rs::usb::DAP2_PACKET_SIZE as usize;

pub struct DebugSocket {
    port: u16,
    timeout: Option<Duration>,
}

impl DebugSocket {
    pub(crate) fn new() -> Self {
        Self {
            port: 1234,
            timeout: Some(Duration::from_secs(10)),
        }
    }

    pub fn port(&mut self, port: u16) -> &mut Self {
        self.port = port;
        self
    }

    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = Some(timeout);
        self
    }

    pub async fn listen(self, stack: &'static embassy_net::Stack<impl Driver>) -> ! {
        static SOCKET_RX_BUFFER: StaticCell<[u8; PACKET_SIZE]> = StaticCell::new();
        static SOCKET_TX_BUFFER: StaticCell<[u8; PACKET_SIZE]> = StaticCell::new();
        let rx_buffer = SOCKET_RX_BUFFER.init([0; PACKET_SIZE]);
        let tx_buffer = SOCKET_TX_BUFFER.init([0; PACKET_SIZE]);
        let mut socket = TcpSocket::new(stack, rx_buffer, tx_buffer);
        socket.set_timeout(self.timeout);

        loop {
            let debug_status = DebugStatus::default();
            let mut dap = Dap::new(debug_status.dap_leds());

            debug!("Waiting for connection");

            if socket.accept(self.port).await.is_err() {
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

                        if !debug_status.disconnected() {
                            break;
                        }
                    }

                    dap.suspend();
                },
                &mut socket,
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

            if INIT_CALLED.load(Ordering::SeqCst) {
                debug!("Flash algorithm detected. Rebooting...");
                reboot();
            }
        }
    }
}

fn reboot() -> ! {
    // Safety: This will reboot the device.
    let p = unsafe { Peripherals::steal() };
    let mut watchdog = Watchdog::new(p.WATCHDOG);
    watchdog.trigger_reset();
    // Not sure why trigger_reset doesn't return !, so we loop here.
    loop {
        nop();
    }
}
