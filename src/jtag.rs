use crate::{dap, swj::Swj};

pub struct Jtag();

impl dap::jtag::Jtag<Swj> for Jtag {
    const AVAILABLE: bool = false;

    fn sequences(&mut self, data: &[u8], rxbuf: &mut [u8]) -> u32 {
        todo!()
    }

    fn set_clock(&mut self, max_frequency: u32) -> bool {
        todo!()
    }
}

impl From<Swj> for Jtag {
    fn from(_: Swj) -> Self {
        todo!()
    }
}