use defmt::trace;
use embassy_rp::pac::common::{Reg, RW};
use embassy_rp::pac::syscfg::regs::Dbgforce;

use crate::{dap, swj::Swj};

pub struct Swd {
    max_frequency: u32,
    cpu_frequency: u32,
    pub(super) dbgforce: Reg<Dbgforce, RW>,
}

impl Swd {
    pub fn new(cpu_frequency: u32, dbgforce: Reg<Dbgforce, RW>) -> Self {
        // TODO: this number is very not accurate
        let max_frequency = cpu_frequency;
        Self {
            max_frequency,
            cpu_frequency,
            dbgforce,
        }
    }
}

impl dap::swd::Swd<Swj> for Swd {
    const AVAILABLE: bool = true;

    fn read_inner(
        &mut self,
        apndp: dap::swd::APnDP,
        a: dap::swd::DPRegister,
    ) -> dap::swd::Result<u32> {
        trace!("SWD read, apndp: {}, addr: {}", apndp, a,);
        // Send request
        let req = dap::swd::make_request(apndp, dap::swd::RnW::R, a);
        trace!("SWD tx request");
        self.tx8(req);

        trace!("SWD rx ack");
        // Read ack, 1 clock for turnaround and 3 for ACK
        let ack = self.rx4() >> 1;

        match dap::swd::Ack::try_ok(ack as u8) {
            Ok(_) => trace!("    ack ok"),
            Err(e) => {
                trace!("    ack error: {}", e);
                // On non-OK ACK, target has released the bus but
                // is still expecting a turnaround clock before
                // the next request, and we need to take over the bus.
                self.tx8(0);
                return Err(e);
            }
        }

        // Read data and parity
        trace!("SWD rx data");
        let (data, parity) = self.read_data();

        // Turnaround + trailing
        self.read_bit();
        self.tx8(0); // Drive the SWDIO line to 0 to not float

        if parity as u8 == (data.count_ones() as u8 & 1) {
            trace!("    data: 0x{:x}", data);
            Ok(data)
        } else {
            Err(dap::swd::Error::BadParity)
        }
    }

    fn write_inner(
        &mut self,
        apndp: dap::swd::APnDP,
        a: dap::swd::DPRegister,
        data: u32,
    ) -> dap::swd::Result<()> {
        trace!(
            "SWD write, apndp: {}, addr: {}, data: 0x{:x}",
            apndp,
            a,
            data
        );

        // Send request
        let req = dap::swd::make_request(apndp, dap::swd::RnW::W, a);
        trace!("SWD tx request");
        self.tx8(req);

        // Read ack, 1 clock for turnaround and 3 for ACK and 1 for turnaround
        trace!("SWD rx ack");
        let ack = (self.rx5() >> 1) & 0b111;
        match dap::swd::Ack::try_ok(ack as u8) {
            Ok(_) => trace!("    ack ok"),
            Err(e) => {
                trace!("    ack err: {}, data: {:b}", e, ack);
                // On non-OK ACK, target has released the bus but
                // is still expecting a turnaround clock before
                // the next request, and we need to take over the bus.
                self.tx8(0);
                return Err(e);
            }
        }

        // Send data and parity
        trace!("SWD tx data");
        let parity = data.count_ones() & 1 == 1;
        self.send_data(data, parity);

        // Send trailing idle
        self.tx8(0);

        Ok(())
    }

    fn set_clock(&mut self, max_frequency: u32) -> bool {
        trace!("SWD set clock: freq = {}", max_frequency);
        if max_frequency < self.cpu_frequency {
            self.max_frequency = max_frequency;
            trace!("  freq = {}", max_frequency);
            true
        } else {
            false
        }
    }
}

impl Swd {
    fn tx8(&mut self, mut data: u8) {
        for _ in 0..8 {
            self.write_bit(data & 1);
            data >>= 1;
        }
    }

    fn rx4(&mut self) -> u8 {
        let mut data = 0;

        for i in 0..4 {
            data |= (self.read_bit() & 1) << i;
        }

        data
    }

    fn rx5(&mut self) -> u8 {
        let mut data = 0;

        for i in 0..5 {
            data |= (self.read_bit() & 1) << i;
        }

        data
    }

    fn send_data(&mut self, mut data: u32, parity: bool) {
        for _ in 0..32 {
            self.write_bit((data & 1) as u8);
            data >>= 1;
        }

        self.write_bit(parity as u8);
    }

    fn read_data(&mut self) -> (u32, bool) {
        let mut data = 0;

        for i in 0..32 {
            data |= (self.read_bit() as u32 & 1) << i;
        }

        let parity = self.read_bit() != 0;

        (data, parity)
    }

    #[inline(always)]
    fn write_bit(&mut self, bit: u8) {
        if bit != 0 {
            self.dbgforce.modify(|r| r.set_proc1_swdi(true));
        } else {
            self.dbgforce.modify(|r| r.set_proc1_swdi(false));
        }

        self.dbgforce.modify(|r| r.set_proc1_swclk(false));
        self.dbgforce.modify(|r| r.set_proc1_swclk(true));
    }

    #[inline(always)]
    fn read_bit(&mut self) -> u8 {
        self.dbgforce.modify(|r| r.set_proc1_swclk(false));
        let bit = self.dbgforce.read().proc1_swdo() as u8;
        self.dbgforce.modify(|r| r.set_proc1_swclk(true));

        bit
    }
}

impl From<Swj> for Swd {
    fn from(swj: Swj) -> Self {
        swj.swd
    }
}
