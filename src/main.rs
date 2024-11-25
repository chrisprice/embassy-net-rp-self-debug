#![no_std]
#![no_main]

mod dap;
mod dap_leds;
mod flash;
mod jtag;
mod network;
mod swd;
mod swj;
mod swo;

use core::cell::RefCell;

use cortex_m::asm::nop;
use cyw43_pio::PioSpi;
use dap::dap::DapVersion;
use dap_leds::DapLeds;
use defmt::*;
use embassy_boot_rp::{BlockingFirmwareUpdater, BootLoaderConfig, FirmwareUpdaterConfig};
use embassy_executor::{Executor, Spawner};
use embassy_net::tcp::TcpSocket;
use embassy_rp::flash::{Async, Flash};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_rp::pac::SYSCFG;
use embassy_rp::peripherals::{DMA_CH0, FLASH, PIN_23, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::watchdog::Watchdog;
use embassy_rp::{bind_interrupts, clocks};
use embassy_time::{Duration, Ticker};
use embedded_io_async::Write;
use static_cell::StaticCell;
use swj::Swj;
use swo::Swo;

use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs0 {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR0: StaticCell<Executor> = StaticCell::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();
static WATCHDOG: StaticCell<Watchdog> = StaticCell::new();

const FLASH_SIZE: usize = 2 * 1024 * 1024;

#[cortex_m_rt::entry]
fn main() -> ! {
    info!("Start");
    flash::init();

    let p = embassy_rp::init(Default::default());

    //let mut watchdog = embassy_rp::watchdog::Watchdog::new(p.WATCHDOG);
    //watchdog.enable(false);
    //^ disabled in embassy-boot's bootloader

    spawn_core1(
        p.CORE1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner| unwrap!(spawner.spawn(core1_task())));
        },
    );

    let mut pio = Pio::new(p.PIO0, Irqs0);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        pio.irq0,
        Output::new(p.PIN_25, Level::High),
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH0,
    );

    let flash = embassy_rp::flash::Flash::new(p.FLASH, p.DMA_CH1);

    let watchdog = WATCHDOG.init(Watchdog::new(p.WATCHDOG));

    let executor0 = EXECUTOR0.init(Executor::new());
    executor0.run(|spawner| {
        unwrap!(spawner.spawn(din_dins(watchdog)));
        unwrap!(spawner.spawn(core0_task(spawner, spi, p.PIN_23, flash)));
    });
}

#[embassy_executor::task]
async fn din_dins(watchdog: &'static mut Watchdog) {
    let mut ticker = Ticker::every(Duration::from_secs(5));
    loop {
        watchdog.feed();
        ticker.next().await;
    }
}

#[embassy_executor::task]
async fn core0_task(
    spawner: Spawner,
    spi: PioSpi<'static, PIO0, 0, DMA_CH0>,
    pin_23: PIN_23,
    flash: Flash<'static, FLASH, Async, FLASH_SIZE>,
) -> ! {
    info!("init'ing network...");
    let stack = network::init_network(
        spawner,
        network::Mode::Station,
        env!("WIFI_SSID"),
        env!("WIFI_PASSPHRASE"),
        network::Address::Dhcp,
        spi,
        Output::new(pin_23, Level::Low),
    )
    .await;

    info!("network ready");

    let mut rx_buffer = [0; dap::dap::DAP2_PACKET_SIZE as usize];
    let mut tx_buffer = [0; dap::dap::DAP2_PACKET_SIZE as usize];

    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(30)));
    info!("socket setup");

    let swj = Swj::new(swd::Swd::new(clocks::clk_sys_freq(), SYSCFG.dbgforce()));
    let mut dap = dap::dap::Dap::new(swj, DapLeds::new(), Swo::new(), "VERSION");
    info!("dap setup");

    let flash = embassy_sync::blocking_mutex::Mutex::new(RefCell::new(flash));
    let config = FirmwareUpdaterConfig::from_linkerfile_blocking(&flash, &flash);
    let mut aligned = embassy_boot_rp::AlignedBuffer([0; 1]);
    let mut updater = BlockingFirmwareUpdater::new(config, &mut aligned.0);

    
    // feels like we should be doing something like this here...
    // updater.mark_booted().unwrap();

    'outer: loop {
        info!("Waiting for connection");
        if socket.accept(1234).await.is_err() {
            warn!("Failed to accept connection");
            continue;
        }

        info!("Connected");

        loop {
            let mut request_buffer = [0; dap::dap::DAP2_PACKET_SIZE as usize];

            trace!("Waiting for request");

            let n = match socket.read(&mut request_buffer).await {
                Ok(0) => {
                    warn!("read EOF");
                    dap.suspend();
                    socket.abort();

                    continue 'outer;
                }
                Ok(n) => n,
                Err(e) => {
                    warn!("read error: {:?}", e);
                    break;
                }
            };

            trace!("Received {} bytes", n);

            let mut response_buffer = [0; dap::dap::DAP2_PACKET_SIZE as usize];
            let n = dap
                .process_command(&request_buffer[..n], &mut response_buffer, DapVersion::V2)
                .await;

            // possibly move this to a polling task
            // or just use proper IPC / SIO.fifo
            flash::monitor::handle_pending_flash(&mut updater);

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
        let _ = socket.flush().await;
    }
}

#[embassy_executor::task]
async fn core1_task() {
    loop {
        nop();
    }
}
