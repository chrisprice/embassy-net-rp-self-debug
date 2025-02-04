use crate::OtaDebugger;
use dap_rs::dap::{DapLeds, HostStatus};
use defmt::{debug, unwrap};
use embassy_boot_rp::FirmwareUpdaterConfig;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

pub struct BootSuccessSignaler(&'static Signal<CriticalSectionRawMutex, ()>);

impl BootSuccessSignaler {
    pub fn new(signal: &'static Signal<CriticalSectionRawMutex, ()>) -> Self {
        Self(signal)
    }
}

impl DapLeds for BootSuccessSignaler {
    fn react_to_host_status(&mut self, host_status: HostStatus) {
        match host_status {
            HostStatus::Connected(true) => {
                self.0.signal(());
            }
            _ => {}
        }
    }
}

pub struct BootSuccessMarker<const FLASH_SIZE: usize> {
    signal: &'static Signal<CriticalSectionRawMutex, ()>,
}

impl<const FLASH_SIZE: usize> BootSuccessMarker<FLASH_SIZE> {
    pub fn new(signal: &'static Signal<CriticalSectionRawMutex, ()>) -> Self {
        Self { signal }
    }

    pub async fn run(&self, ota_debugger: &OtaDebugger<FLASH_SIZE>) {
        self.signal.wait().await;
        debug!("Marking successful boot");

        ota_debugger
            .with_flash_blocking(
                |flash, _| {
                    let mut buffer =
                        embassy_boot_rp::AlignedBuffer([0; embassy_rp::flash::WRITE_SIZE]);
                    let mut firmware_updater = embassy_boot_rp::BlockingFirmwareUpdater::new(
                        FirmwareUpdaterConfig::from_linkerfile_blocking(flash, flash),
                        &mut buffer.0,
                    );

                    match unwrap!(firmware_updater.get_state()) {
                        embassy_boot_rp::State::Swap => {
                            unwrap!(firmware_updater.mark_booted());
                        }
                        _ => {}
                    }
                },
                (),
            );
    }
}
