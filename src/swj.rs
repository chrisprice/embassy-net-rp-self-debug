use crate::{dap, jtag::Jtag, swd::Swd};

pub struct Swj {
    pub(super) swd: Swd,
}

impl Swj {
    pub fn new(swd: Swd) -> Self {
        Self { swd }
    }
}

impl dap::swj::Dependencies<Swd, Jtag> for Swj {
    fn process_swj_pins(
        &mut self,
        output: crate::dap::swj::Pins,
        mask: crate::dap::swj::Pins,
        wait_us: u32,
    ) -> crate::dap::swj::Pins {
        todo!()
    }

    fn process_swj_sequence(&mut self, data: &[u8], nbits: usize) {
        todo!()
    }

    fn process_swj_clock(&mut self, max_frequency: u32) -> bool {
        todo!()
    }

    fn high_impedance_mode(&mut self) {
        todo!()
    }
}

impl From<Swd> for Swj {
    fn from(_: Swd) -> Self {
        todo!()
    }
}

impl From<Jtag> for Swj {
    fn from(_: Jtag) -> Self {
        todo!()
    }
}
