use cyw43_pio::PioSpi;
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_net::{Config, DhcpConfig, Ipv4Cidr, Stack, StackResources, StaticConfigV4};
use embassy_rp::{
    clocks::RoscRng,
    gpio::Output,
    peripherals::{DMA_CH0, PIO0},
};
use embassy_time::Timer;
use heapless::Vec;
use rand::RngCore;
use static_cell::StaticCell;

#[allow(dead_code)]
pub enum Mode {
    AccessPoint { channel: u8 },
    Station,
}

#[allow(dead_code)]
pub enum Address {
    Dhcp,
    StaticV4(Ipv4Cidr),
}

pub async fn init_network(
    spawner: Spawner,
    mode: Mode,
    ssid: &'static str,
    passphrase: &'static str,
    ip_address: Address,
    pio_spi: PioSpi<'static, PIO0, 0, DMA_CH0>,
    pwr: Output<'static>,
) -> &'static Stack<cyw43::NetDriver<'static>> {
    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());

    let fw: &[u8; 230321] = include_bytes!("43439A0.bin");
    let clm: &[u8; 4752] = include_bytes!("43439A0_clm.bin");

    let (net_device, mut control, runner) = cyw43::new(state, pwr, pio_spi, fw).await;
    spawner.must_spawn(wifi_task(runner));

    control.init(clm).await;
    control
        .set_power_management(cyw43::PowerManagementMode::PowerSave)
        .await;

    let config = match ip_address {
        Address::Dhcp => {
            let mut cfg = DhcpConfig::default();

            cfg.hostname = Some(["pico0"].into_iter().collect());

            Config::dhcpv4(cfg)
        },
        Address::StaticV4(ip_address) => Config::ipv4_static(StaticConfigV4 {
            address: ip_address,
            gateway: None,
            dns_servers: Vec::new(),
        }),
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

    loop {
        match mode {
            Mode::AccessPoint { channel } => {
                control.start_ap_wpa2(ssid, passphrase, channel).await;
                break;
            }
            Mode::Station => {
                let r = control
                    .join_wpa2(ssid, passphrase)
                    .await;

                match r {
                    Ok(_) => break,
                    Err(e) => {
                        error!("couldn't join {}: status={}, retrying...", ssid, e.status);
                    }
                }
            }
        }
    }

    while !stack.is_config_up() {
        Timer::after_millis(100).await;
    }

    info!("network initialized {}", stack.config_v4().unwrap().address);

    stack
}

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<
        'static,
        Output<'static>,
        PioSpi<'static, PIO0, 0, DMA_CH0>,
    >,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(stack: &'static Stack<cyw43::NetDriver<'static>>) -> ! {
    stack.run().await
}
