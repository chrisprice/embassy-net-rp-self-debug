#![no_std]
#![no_main]

use cyw43_pio::PioSpi;
use defmt::{info, unwrap, warn};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_net::{Config, DhcpConfig, Stack, StackResources};
use embassy_net_rp_self_debug::debug::socket::DebugSocket;
use embassy_net_rp_self_debug::{OtaDebugger, State};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH1, PIN_23, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_rp::watchdog::Watchdog;
use embassy_time::{Duration, Ticker};
use panic_probe as _;
use rand::RngCore;
use static_cell::StaticCell;

const FLASH_SIZE: usize = 2048 * 1024;

bind_interrupts!(struct Irqs0 {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

#[embassy_executor::task]
async fn net_init(
    spi: PioSpi<'static, PIO0, 0, DMA_CH1>,
    pwr: PIN_23,
    mut debug_socket: DebugSocket,
) {
    info!("Initializing network");
    
    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init_with(|| cyw43::State::new());

    let fw: &[u8; 230321] = include_bytes!("./network/43439A0.bin");
    let clm: &[u8; 4752] = include_bytes!("./network/43439A0_clm.bin");

    // To make flashing faster for development, you may want to flash the firmwares independently
    // at hardcoded addresses, instead of baking them into the program with `include_bytes!`:
    //     probe-rs download examples/network/43439A0.bin --binary-format bin --chip RP2040 --base-address 0x10108000
    //     probe-rs download examples/network/43439A0_clm.bin --binary-format bin --chip RP2040 --base-address 0x10148000
    // let fw = unsafe { core::slice::from_raw_parts(0x10108000 as *const u8, 230321) };
    // let clm = unsafe { core::slice::from_raw_parts(0x10148000 as *const u8, 4752) };


    let (net_device, mut control, runner) =
        cyw43::new(state, Output::new(pwr, Level::Low), spi, fw).await;

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
    let stack = &*STACK.init_with(|| Stack::new(
        net_device,
        config,
        RESOURCES.init_with(|| StackResources::<8>::new()),
        RoscRng.next_u64(),
    ));

    spawner.must_spawn(net_task(stack));

    debug_socket.port(1234).timeout(Duration::from_secs(30));

    spawner.must_spawn(debug_task(stack, debug_socket));

    unwrap!(control
        .join_wpa2(env!("WIFI_SSID"), env!("WIFI_PASSPHRASE"))
        .await
        .map_err(|_| "failed to join network"));

    stack.wait_config_up().await;

    info!("Network up {}", stack.config_v4().unwrap().address);
}

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH1>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<cyw43::NetDriver<'static>>) -> ! {
    stack.run().await
}

#[embassy_executor::task]
async fn debug_task(
    stack: &'static Stack<cyw43::NetDriver<'static>>,
    debug_socket: DebugSocket,
) -> ! {
    debug_socket.listen(stack).await
}

#[embassy_executor::task]
async fn feed_watchdog(mut watchdog: Watchdog) {
    let mut ticker = Ticker::every(Duration::from_secs(1));
    loop {
        watchdog.feed();
        ticker.next().await;
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());

    let mut pio = Pio::new(p.PIO0, Irqs0);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        pio.irq0,
        Output::new(p.PIN_25, Level::High),
        p.PIN_24,
        p.PIN_29,
        p.DMA_CH1,
    );
    let pin_23 = p.PIN_23;

    let mut watchdog = Watchdog::new(p.WATCHDOG);
    watchdog.pause_on_debug(true);
    spawner.must_spawn(feed_watchdog(watchdog));

    static OTA_DEBUGGER_STATE: StaticCell<State<FLASH_SIZE, {32 * 1024}>> = StaticCell::new();
    let state = OTA_DEBUGGER_STATE.init_with(|| State::new(p.FLASH, p.DMA_CH0));
    
    let ota_debugger = OtaDebugger::new(state, p.CORE1, |spawner, debug_socket| {
        // Spawn the network initialization task on core1 so that it can continue
        // running during debugging of core0.
        spawner.must_spawn(net_init(spi, pin_23, debug_socket));
    })
    .await;

    if ota_debugger
        .with_firmware_updater_blocking(|flash| flash.mark_booted())
        .await
        .is_err()
    {
        warn!("Failed to mark booted");
    }
}
