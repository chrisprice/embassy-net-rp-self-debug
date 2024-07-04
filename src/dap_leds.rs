use defmt::info;

use crate::dap::dap;

pub struct DapLeds();

impl DapLeds {
    pub fn new() -> Self {
        Self()
    }
}

impl dap::DapLeds for DapLeds {
    fn react_to_host_status(&mut self, host_status: dap::HostStatus) {
        info!("Host status: {:?}", host_status);
    }
}
