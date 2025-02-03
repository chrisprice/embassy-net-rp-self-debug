#![no_std]

mod dap;
mod flash_new;
mod spinlock;

use core::{future::Future, ops::DerefMut};

use dap::Dap;
use dap_rs::dap::{DapLeds, DapVersion, HostStatus};
use defmt::{debug, trace, unwrap, warn};
use embassy_executor::{Executor, SpawnToken, Spawner};
use embassy_net::{driver::Driver, tcp::TcpSocket};
use embassy_rp::{
    flash::{Async, Flash},
    multicore::{spawn_core1, Stack},
    peripherals::{CORE1, FLASH},
};
use embassy_sync::{
    blocking_mutex::raw::{CriticalSectionRawMutex, NoopRawMutex},
    mutex::Mutex,
    once_lock::OnceLock,
    signal::Signal,
};
use embassy_time::Duration;
use embedded_io_async::Write;
use flash_new::{with_spinlock, FlashMutex, FlashNew};
use static_cell::StaticCell;

const PACKET_SIZE: usize = dap_rs::usb::DAP2_PACKET_SIZE as usize;
type DebugSocketLock = OnceLock<Mutex<NoopRawMutex, TcpSocket<'static>>>;

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static DEBUG_SOCKET: DebugSocketLock = OnceLock::new();

struct Alice(&'static Signal<CriticalSectionRawMutex, ()>);

impl DapLeds for Alice {
    fn react_to_host_status(&mut self, host_status: HostStatus) {
        match host_status {
            HostStatus::Connected(true) => {
                self.0.signal(());
            }
            _ => {}
        }
    }
}

#[embassy_executor::task]
pub async fn mark_successful_boot_task(
    flash_new: &'static FlashNew,
    signal: &'static Signal<CriticalSectionRawMutex, ()>,
) {
    signal.wait().await;
    debug!("Marking successful boot");

    let mut state_buffer = embassy_boot_rp::AlignedBuffer([0; embassy_rp::flash::WRITE_SIZE]);
    flash_new.with_firmware_updater(
        &mut state_buffer,
        |mut updater, _| async move {
            match unwrap!(updater.get_state()) {
                embassy_boot_rp::State::Swap => {
                    unwrap!(updater.mark_booted());
                }
                _ => {}
            }
        },
        (),
    ).await;
}

#[embassy_executor::task]
async fn debug_listen_task(
    sucessful_boot_signal: &'static Signal<CriticalSectionRawMutex, ()>,
    port: u16,
    timeout: Duration,
) -> ! {
    let mut socket = DEBUG_SOCKET.get().await.lock().await;
    socket.set_timeout(Some(timeout));

    loop {
        debug!("Waiting for connection");

        if socket.accept(port).await.is_err() {
            warn!("Failed to accept connection");
            continue;
        }

        debug!("Connected");

        with_spinlock(
            |socket| async {
                let mut dap = Dap::new_with_leds(Alice(sucessful_boot_signal));

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
            .map_err(|_| "Socket already initialized"));
    }
}

pub struct Bob {
    flash: &'static FlashNew,
}
impl Bob {
    pub async fn new<ARGS, S>(
        core1: CORE1,
        flash: Flash<'static, FLASH, Async, { flash_new::FLASH_SIZE }>,
        init_args: ARGS,
        net_init: impl FnOnce(ARGS, Carol) -> SpawnToken<S> + Send + 'static,
        port: u16,
        timeout: Duration,
    ) -> Self
    where
        ARGS: Send + 'static,
    {
        // TODO: install flash algo trampolines

        let flash_new = FlashNew::new(flash)
            .map_err(|_| "Flash already initialised")
            .unwrap(); // TODO: handle error

        static SIGNAL: StaticCell<Signal<CriticalSectionRawMutex, ()>> = StaticCell::new();
        let successful_boot_signal = &*SIGNAL.init(Signal::new());

        let spawner = Spawner::for_current_executor().await;

        spawner.must_spawn(mark_successful_boot_task(
            flash_new,
            successful_boot_signal,
        ));

        spawn_core1(
            core1,
            unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
            move || {
                let executor1 = EXECUTOR1.init(Executor::new());
                executor1.run(|spawner| {
                    spawner.must_spawn(net_init(init_args, Carol::new()));
                    spawner.must_spawn(debug_listen_task(successful_boot_signal, port, timeout))
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
