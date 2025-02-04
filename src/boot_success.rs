use core::cell::RefCell;

use crate::flash::spinlock::with_spinlock_blocking;
use dap_rs::dap::{DapLeds, HostStatus};
use defmt::{debug, unwrap};
use embassy_boot_rp::FirmwareUpdaterConfig;
use embassy_rp::{
    flash::{Async, Flash},
    peripherals::FLASH,
};
use embassy_sync::{
    blocking_mutex::{raw::CriticalSectionRawMutex, NoopMutex},
    signal::Signal,
};

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
    flash: &'static NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>,
    signal: &'static Signal<CriticalSectionRawMutex, ()>,
}

impl<const FLASH_SIZE: usize> BootSuccessMarker<FLASH_SIZE> {
    pub fn new(
        flash: &'static NoopMutex<RefCell<Flash<'static, FLASH, Async, FLASH_SIZE>>>,
        signal: &'static Signal<CriticalSectionRawMutex, ()>,
    ) -> Self {
        Self { flash, signal }
    }

    pub async fn run(&self) {
        self.signal.wait().await;
        debug!("Marking successful boot");

        with_spinlock_blocking(
            |_| {
                let mut buffer = embassy_boot_rp::AlignedBuffer([0; embassy_rp::flash::WRITE_SIZE]);
                let mut firmware_updater = embassy_boot_rp::BlockingFirmwareUpdater::new(
                    FirmwareUpdaterConfig::from_linkerfile_blocking(&self.flash, &self.flash),
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
