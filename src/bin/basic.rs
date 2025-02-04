#![no_std]
#![no_main]

use cortex_m::asm::nop;
use cyw43_pio::PioSpi;
use embassy_executor::Spawner;
use embassy_net::{Config, DhcpConfig, Stack, StackResources};
use embassy_net_rp_self_debug::boot_success::BootSuccessMarker;
use embassy_net_rp_self_debug::debug::socket::DebugSocket;
use embassy_net_rp_self_debug::{OtaDebugger, State};
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::RoscRng;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH1, PIN_23, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_time::Duration;
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
    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());

    let fw: &[u8; 230321] = include_bytes!("./network/43439A0.bin");
    let clm: &[u8; 4752] = include_bytes!("./network/43439A0_clm.bin");

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
    let stack = &*STACK.init(Stack::new(
        net_device,
        config,
        RESOURCES.init(StackResources::<8>::new()),
        RoscRng.next_u64(),
    ));

    spawner.must_spawn(net_task(stack));

    debug_socket.port(1234).timeout(Duration::from_secs(30));

    spawner.must_spawn(debug_task(stack, debug_socket));

    control
        .join_wpa2("ssid", "passphrase")
        .await
        .map_err(|_| "failed to join network");

    stack.wait_config_up().await;
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
async fn boot_success_task(boot_success_marker: BootSuccessMarker<FLASH_SIZE>, ota_debugger: &'static OtaDebugger<FLASH_SIZE>) {
    boot_success_marker.run(ota_debugger).await
}

#[embassy_executor::main]
async fn main(_s: Spawner) -> ! {
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


    static OTA_DEBUGGER_STATE: StaticCell<State> = StaticCell::new();
    let state = OTA_DEBUGGER_STATE.init(State::new());

    let (ota_debugger, boot_success_marker) = OtaDebugger::<FLASH_SIZE>::new(
        state,
        p.FLASH,
        p.DMA_CH0,
        p.CORE1,
        |spawner, debug_socket| {
            // TODO: Ensure debug_socket is !Send/!Sync
            // Spawn the network initialization task on core1 so that it can continue
            // running during debugging of core0.
            spawner.must_spawn(net_init(spi, pin_23, debug_socket));
        },
    ).await;

    boot_success_marker.run(&ota_debugger).await;

    loop {
        nop();
    }
}
