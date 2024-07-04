use crate::{dap, swj::Swj};

pub struct Swd();

impl dap::swd::Swd<Swj> for Swd {
    const AVAILABLE: bool = false;

    fn read_inner(&mut self, apndp: dap::swd::APnDP, a: dap::swd::DPRegister) -> dap::swd::Result<u32> {
        todo!()
    }

    fn write_inner(&mut self, apndp: dap::swd::APnDP, a: dap::swd::DPRegister, data: u32) -> dap::swd::Result<()> {
        todo!()
    }

    fn set_clock(&mut self, max_frequency: u32) -> bool {
        todo!()
    }
}

impl From<Swj> for Swd {
    fn from(_: Swj) -> Self {
        todo!()
    }
}