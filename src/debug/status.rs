use core::cell::RefCell;

use dap_rs::dap::{DapLeds, HostStatus};
use defmt::trace;

/// Materializes the debug status of the device.
#[derive(Default)]
pub struct DebugStatus {
    connected: RefCell<bool>,
    disconnected: RefCell<bool>,
}

impl DebugStatus {
    /// The host has signalled (at least once) that it has connected and then subsequently disconnected.
    pub fn disconnected(&self) -> bool {
        *self.disconnected.borrow()
    }
    pub fn dap_leds(&self) -> DebugStatusInner<'_> {
        DebugStatusInner { status: self }
    }
}

pub struct DebugStatusInner<'a> {
    status: &'a DebugStatus,
}

impl DapLeds for DebugStatusInner<'_> {
    #[allow(clippy::if_same_then_else)]
    fn react_to_host_status(&mut self, host_status: HostStatus) {
        match host_status {
            HostStatus::Connected(connected) => {
                if connected {
                    trace!("Connected");
                    self.status.connected.replace(true);
                } else {
                    trace!("Disconnected");
                    if *self.status.connected.borrow() {
                        self.status.disconnected.replace(true);
                    }
                }
            }
            HostStatus::Running(running) => {
                if running {
                    trace!("Running");
                } else {
                    trace!("Stopped");
                }
            }
        }
    }
}
