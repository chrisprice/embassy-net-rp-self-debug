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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connected_then_disconnected() {
        let status = DebugStatus::default();
        let mut led = status.dap_leds();

        // Initially, no disconnect has been recorded.
        assert_eq!(status.disconnected(), false);

        // Signal a connection.
        led.react_to_host_status(HostStatus::Connected(true));
        // Now signal a disconnection, which should only set the disconnected flag because a connection was seen.
        led.react_to_host_status(HostStatus::Connected(false));

        assert_eq!(status.disconnected(), true);
    }

    #[test]
    fn test_disconnect_without_prior_connection() {
        let status = DebugStatus::default();
        let mut led = status.dap_leds();

        // Signal a disconnection without a prior connection.
        led.react_to_host_status(HostStatus::Connected(false));

        // The disconnected flag should remain false.
        assert_eq!(status.disconnected(), false);
    }

    #[test]
    fn test_running_status_has_no_effect_on_connection() {
        let status = DebugStatus::default();
        let mut led = status.dap_leds();

        // Sending running messages should not change connection related flags.
        led.react_to_host_status(HostStatus::Running(true));
        led.react_to_host_status(HostStatus::Running(false));

        // Since no connection/disconnection occurred, disconnected should be false.
        assert_eq!(status.disconnected(), false);
    }
}
