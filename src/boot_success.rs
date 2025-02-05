use crate::OtaDebugger;
use dap_rs::dap::{DapLeds, HostStatus};
use defmt::{trace, warn};
use embassy_boot::FirmwareUpdaterError;
use embassy_boot_rp::AlignedBuffer;
use embassy_rp::flash::WRITE_SIZE;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};

/// Makes a best effort to send host status without blocking.
pub struct HostStatusSender<'a> {
    signal: &'a Signal<CriticalSectionRawMutex, HostStatus>,
}

impl<'a> HostStatusSender<'a> {
    pub fn new(signal: &'a Signal<CriticalSectionRawMutex, HostStatus>) -> Self {
        Self { signal }
    }
}

impl<'a> DapLeds for HostStatusSender<'a> {
    fn react_to_host_status(&mut self, host_status: HostStatus) {
        self.signal.signal(host_status);
    }
}

/// Makes a best effort attempt to mark the firmware as booted when the host connects.
/// If this fails, it will be retried on the next connection.
pub struct BootSuccessMarker<'a, const FLASH_SIZE: usize> {
    signal: &'a Signal<CriticalSectionRawMutex, HostStatus>,
    ota_debugger: &'a OtaDebugger<FLASH_SIZE>,
}

impl<'a, const FLASH_SIZE: usize> BootSuccessMarker<'a, FLASH_SIZE> {
    pub fn new(
        signal: &'a Signal<CriticalSectionRawMutex, HostStatus>,
        ota_debugger: &'a OtaDebugger<FLASH_SIZE>,
    ) -> Self {
        Self {
            signal,
            ota_debugger,
        }
    }

    pub async fn run(&self) {
        loop {
            match self.signal.wait().await {
                HostStatus::Connected(true) => match self.mark_booted().await {
                    Ok(_) => {
                        trace!("Marked booted");
                        break;
                    }
                    Err(e) => {
                        warn!("Failed to mark booted: {:?}", e);
                    }
                },
                _ => {}
            }
        }
    }

    async fn mark_booted(&self) -> Result<(), FirmwareUpdaterError> {
        let mut buffer = AlignedBuffer([0; WRITE_SIZE]);
        self.ota_debugger
            .with_firmware_updater_blocking(&mut buffer, |updater| updater.mark_booted())
            .await
    }
}
