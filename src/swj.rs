use defmt::trace;

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
        _output: crate::dap::swj::Pins,
        _mask: crate::dap::swj::Pins,
        _wait_us: u32,
    ) -> crate::dap::swj::Pins {
        todo!()
    }

    async fn process_swj_sequence(&mut self, data: &[u8], bits: usize) {
        self.swd.dbgforce.modify(|r| r.set_proc1_attach(true));

        self.swd.delay_half_period().await;

        trace!("Running SWJ sequence: {:08b}, len = {}", data, bits);
        self.swd.txn(data, bits).await;
    }

    fn process_swj_clock(&mut self, _max_frequency: u32) -> bool {
        todo!()
    }

    fn high_impedance_mode(&mut self) {
        self.swd.dbgforce.modify(|r| r.set_proc1_attach(false));
    }
}

impl From<Swd> for Swj {
    fn from(swd: Swd) -> Self {
        Self { swd }
    }
}

impl From<Jtag> for Swj {
    fn from(_: Jtag) -> Self {
        todo!()
    }
}
