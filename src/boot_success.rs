use crate::flash::guard::FlashGuard;
use dap_rs::dap::{DapLeds, HostStatus};
use defmt::{debug, unwrap};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use static_cell::StaticCell;

static SIGNAL: StaticCell<BootSuccessSignaler> = StaticCell::new();

pub struct BootSuccessSignaler(Signal<CriticalSectionRawMutex, ()>);

impl BootSuccessSignaler {
    pub fn new() -> &'static Self {
        // TODO: handle error
        &*SIGNAL.init(Self(Signal::new()))
    }
    pub fn dap_leds(&'static self) -> Signaler {
        Signaler(self)
    }
    fn signal(&self) {
        self.0.signal(());
    }
}

pub struct Signaler(&'static BootSuccessSignaler);

impl DapLeds for Signaler {
    fn react_to_host_status(&mut self, host_status: HostStatus) {
        match host_status {
            HostStatus::Connected(true) => {
                self.0.signal();
            }
            _ => {}
        }
    }
}

#[embassy_executor::task]
pub async fn mark_booted_task(
    flash_new: &'static FlashGuard,
    signal: &'static BootSuccessSignaler,
) {
    signal.0.wait().await;
    debug!("Marking successful boot");

    let mut state_buffer = embassy_boot_rp::AlignedBuffer([0; embassy_rp::flash::WRITE_SIZE]);
    flash_new
        .with_firmware_updater(
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
        )
        .await;
}
