#![no_std]
#![no_main]

use cortex_m::asm::nop;
use cyw43_pio::PioSpi;
use embassy_executor::Spawner;
use embassy_net::{Config, DhcpConfig, Stack, StackResources};
use embassy_net_rp_self_debug::Carol;
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIN_23, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_time::Duration;
use rand::RngCore;
use static_cell::StaticCell;

bind_interrupts!(struct Irqs0 {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

static NET_CONTROL_CHANNEL: StaticCell<Channel<CriticalSectionRawMutex, usize, 1>> =
    StaticCell::new();

struct NetInitArgs<'a> {
    spi: PioSpi<'a, PIO0, 0, DMA_CH0>,
    pin_23: PIN_23,
    sender: Sender<'a, CriticalSectionRawMutex, usize, 1>,
}

#[embassy_executor::task]
async fn net_init(args: NetInitArgs<'static>, carol: Carol) -> ! {
    args.sender.send(0).await;

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());

    let fw: &[u8; 230321] = include_bytes!("../network/43439A0.bin");
    let clm: &[u8; 4752] = include_bytes!("../network/43439A0_clm.bin");

    let (net_device, mut control, runner) =
        cyw43::new(state, Output::new(args.pin_23, Level::Low), args.spi, fw).await;

    let spawner = Spawner::for_current_executor().await;
    spawner.must_spawn(wifi_task(runner));

    control.init(clm).await;

    let config = {
        let mut cfg = DhcpConfig::default();
        cfg.hostname = Some(["pico0"].into_iter().collect());
        Config::dhcpv4(cfg)
    };

    static STACK: StaticCell<Stack<cyw43::NetDriver<'static>>> = StaticCell::new();
    static RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();
    let stack = &*STACK.init(Stack::new(
        net_device,
        config,
        RESOURCES.init(StackResources::<8>::new()),
        RoscRng.next_u64(),
    ));

    spawner.must_spawn(net_task(stack));

    carol.listen(stack);

    control.join_wpa2("ssid", "passphrase").await.map_err(|_| "failed to join network");

    stack.wait_config_up().await;

    loop {
        nop();
    }
}

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<cyw43::NetDriver<'static>>) -> ! {
    stack.run().await
}

#[embassy_executor::main]
async fn main(_s: Spawner) -> ! {
    let net_control_channel: &mut Channel<CriticalSectionRawMutex, usize, 1> =
        NET_CONTROL_CHANNEL.init(Channel::new());

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
    let net_init_args = NetInitArgs {
        spi,
        pin_23: p.PIN_23,
        sender: net_control_channel.sender(),
    };

    const PORT: u16 = 1234;
    const TIMEOUT: Duration = Duration::from_secs(30);
    embassy_net_rp_self_debug::Bob::new(p.CORE1, net_init_args, net_init, PORT, TIMEOUT);

    loop {
        nop();
    }
}
