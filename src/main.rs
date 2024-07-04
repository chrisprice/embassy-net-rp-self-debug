#![no_std]
#![no_main]

mod dap;
mod network;

use cyw43_pio::PioSpi;
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::tcp::TcpSocket;
use embassy_net::{Ipv4Address, Ipv4Cidr};
use embassy_rp::bind_interrupts;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::PIO0;
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_time::Duration;
use embedded_io_async::Write;

use {defmt_rtt as _, panic_probe as _};

bind_interrupts!(struct Irqs0 {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    info!("Start");
    let p = embassy_rp::init(Default::default());

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
    let stack = network::init_network(
        spawner,
        network::Mode::Station,
        env!("WIFI_SSID"),
        env!("WIFI_PASSPHRASE"),
        Ipv4Cidr::new(Ipv4Address::new(192, 168, 1, 217), 24),
        spi,
        Output::new(p.PIN_23, Level::Low),
    )
    .await;

    loop {
        let mut buf = [0; dap::usb::DAP2_PACKET_SIZE as usize];
        let mut rx_buffer = [0; dap::usb::DAP2_PACKET_SIZE as usize];
        let mut tx_buffer = [0; dap::usb::DAP2_PACKET_SIZE as usize];

        let mut socket = TcpSocket::new(
            stack,
            &mut rx_buffer,
            &mut tx_buffer,
        );
        socket.set_timeout(Some(Duration::from_secs(10)));

        socket.accept(1234).await.expect("accept failed");

        loop {
            let n = match socket.read(&mut buf).await {
                Ok(0) => {
                    warn!("read EOF");
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    warn!("read error: {:?}", e);
                    break;
                }
            };

            let x = &buf[..n];

            match socket.write_all(&buf[..n]).await {
                Ok(()) => {}
                Err(e) => {
                    warn!("write error: {:?}", e);
                    break;
                }
            };
        }
    }
}
