use dap_rs::jtag::Jtag;
use embassy_rp::pac::common::{Reg, RW};
use embassy_rp::pac::syscfg::regs::Dbgforce;

use dap_rs::{
    dap::{self, DapLeds},
    swd::Swd,
    swj::{self, Dependencies},
    swo::Swo,
};
use defmt::trace;
use embassy_rp::pac::SYSCFG;

pub struct Dap {
    dbgforce: Reg<Dbgforce, RW>,
}

impl Dap {
    pub fn new() -> dap_rs::dap::Dap<'static, Dap, Leds, embassy_time::Delay, Dap, Dap, Dap> {
        Self::new_with_leds(Leds())
    }
    fn new_with_leds<LEDS: DapLeds>(
        leds: LEDS,
    ) -> dap_rs::dap::Dap<'static, Dap, LEDS, embassy_time::Delay, Dap, Dap, Dap> {
        let inner = Dap {
            dbgforce: SYSCFG.dbgforce(),
        };
        dap_rs::dap::Dap::new(inner, leds, embassy_time::Delay, None, "")
    }
}

impl Dap {
    pub fn txn(&mut self, data: &[u8], mut bits: usize) {
        for byte in data {
            let mut byte = *byte;
            let frame_bits = core::cmp::min(bits, 8);
            for _ in 0..frame_bits {
                let bit = byte & 1;
                byte >>= 1;
                self.write_bit(bit);
            }
            bits -= frame_bits;
        }
    }

    fn tx<const N: usize>(&mut self, mut data: u8) {
        for _ in 0..N {
            self.write_bit(data & 1);
            data >>= 1;
        }
    }

    fn rx<const N: usize>(&mut self) -> u8 {
        let mut data = 0;

        for i in 0..N {
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
        self.dbgforce.modify(|r| r.set_proc1_swclk(false));
        self.dbgforce.modify(|r| r.set_proc1_swdi(bit != 0));
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

impl Dependencies<Dap, Dap> for Dap {
    fn process_swj_pins(
        &mut self,
        _output: swj::Pins,
        _mask: swj::Pins,
        _wait_us: u32,
    ) -> swj::Pins {
        unimplemented!("process_swj_pins not available")
    }

    fn process_swj_sequence(&mut self, data: &[u8], bits: usize) {
        self.dbgforce.modify(|r| r.set_proc1_attach(true));
        trace!("Running SWJ sequence: {:08b}, len = {}", data, bits);
        self.txn(data, bits);
    }

    fn process_swj_clock(&mut self, _max_frequency: u32) -> bool {
        unimplemented!("process_swj_clock not available")
    }

    fn high_impedance_mode(&mut self) {
        self.dbgforce.modify(|r| r.set_proc1_attach(false));
    }
}

impl Jtag<Dap> for Dap {
    const AVAILABLE: bool = false;

    fn sequences(&mut self, _data: &[u8], _rxbuf: &mut [u8]) -> u32 {
        unimplemented!("Jtag::sequences not available")
    }

    fn set_clock(&mut self, _max_frequency: u32) -> bool {
        unimplemented!("Jtag::set_clock not available")
    }
}

impl Swd<Dap> for Dap {
    const AVAILABLE: bool = true;

    fn read_inner(
        &mut self,
        apndp: dap_rs::swd::APnDP,
        a: dap_rs::swd::DPRegister,
    ) -> dap_rs::swd::Result<u32> {
        trace!("SWD read, apndp: {}, addr: {}", apndp, a,);
        // Send request
        let req = dap_rs::swd::make_request(apndp, dap_rs::swd::RnW::R, a);
        trace!("SWD tx request");
        self.tx::<8>(req);

        trace!("SWD rx ack");
        // Read ack, 1 clock for turnaround and 3 for ACK
        let ack = self.rx::<4>() >> 1;

        match dap_rs::swd::Ack::try_ok(ack as u8) {
            Ok(_) => trace!("    ack ok"),
            Err(e) => {
                trace!("    ack error: {}", e);
                // On non-OK ACK, target has released the bus but
                // is still expecting a turnaround clock before
                // the next request, and we need to take over the bus.
                self.tx::<8>(0);
                return Err(e);
            }
        }

        // Read data and parity
        trace!("SWD rx data");
        let (data, parity) = self.read_data();

        // Turnaround + trailing
        self.read_bit();
        self.tx::<8>(0); // Drive the SWDIO line to 0 to not float

        if parity as u8 == (data.count_ones() as u8 & 1) {
            trace!("    data: 0x{:x}", data);
            Ok(data)
        } else {
            Err(dap_rs::swd::Error::BadParity)
        }
    }

    fn write_inner(
        &mut self,
        apndp: dap_rs::swd::APnDP,
        a: dap_rs::swd::DPRegister,
        data: u32,
    ) -> dap_rs::swd::Result<()> {
        trace!(
            "SWD write, apndp: {}, addr: {}, data: 0x{:x}",
            apndp,
            a,
            data
        );

        // Send request
        let req = dap_rs::swd::make_request(apndp, dap_rs::swd::RnW::W, a);
        trace!("SWD tx request");
        self.tx::<8>(req);

        // Read ack, 1 clock for turnaround and 3 for ACK and 1 for turnaround
        trace!("SWD rx ack");
        let ack = (self.rx::<5>() >> 1) & 0b111;
        match dap_rs::swd::Ack::try_ok(ack as u8) {
            Ok(_) => trace!("    ack ok"),
            Err(e) => {
                trace!("    ack err: {}, data: {:b}", e, ack);
                // On non-OK ACK, target has released the bus but
                // is still expecting a turnaround clock before
                // the next request, and we need to take over the bus.
                self.tx::<8>(0);
                return Err(e);
            }
        }

        // Send data and parity
        trace!("SWD tx data");
        let parity = data.count_ones() & 1 == 1;
        self.send_data(data, parity);

        // Send trailing idle
        self.tx::<8>(0);

        Ok(())
    }

    fn set_clock(&mut self, max_frequency: u32) -> bool {
        assert_eq!(max_frequency, 1_000_000, "probe-rs hard-coded frequency");
        true
    }

    fn write_sequence(&mut self, _num_bits: usize, _data: &[u8]) -> dap_rs::swd::Result<()> {
        unimplemented!("Swd::write_sequence not available")
    }

    fn read_sequence(&mut self, _num_bits: usize, _data: &mut [u8]) -> dap_rs::swd::Result<()> {
        unimplemented!("Swd::read_sequence not available")
    }
}

impl Swo for Dap {
    fn set_transport(&mut self, _transport: dap_rs::swo::SwoTransport) {
        unimplemented!("Swo::set_transport not available")
    }

    fn set_mode(&mut self, _mode: dap_rs::swo::SwoMode) {
        unimplemented!("Swo::set_mode not available")
    }

    fn set_baudrate(&mut self, _baudrate: u32) -> u32 {
        unimplemented!("Swo::set_baudrate not available")
    }

    fn set_control(&mut self, _control: dap_rs::swo::SwoControl) {
        unimplemented!("Swo::set_control not available")
    }

    fn polling_data(&mut self, _buf: &mut [u8]) -> u32 {
        unimplemented!("Swo::polling_data not available")
    }

    fn streaming_data(&mut self) {
        unimplemented!("Swo::streaming_data not available")
    }

    fn is_active(&self) -> bool {
        unimplemented!("Swo::is_active not available")
    }

    fn bytes_available(&self) -> u32 {
        unimplemented!("Swo::bytes_available not available")
    }

    fn buffer_size(&self) -> u32 {
        unimplemented!("Swo::buffer_size not available")
    }

    fn support(&self) -> dap_rs::swo::SwoSupport {
        unimplemented!("Swo::support not available")
    }

    fn status(&mut self) -> dap_rs::swo::SwoStatus {
        unimplemented!("Swo::status not available")
    }
}

pub struct Leds();

impl DapLeds for Leds {
    fn react_to_host_status(&mut self, host_status: dap::HostStatus) {
        match host_status {
            dap::HostStatus::Connected(true) => trace!("Connected"),
            dap::HostStatus::Connected(false) => trace!("Disconnected"),
            dap::HostStatus::Running(true) => trace!("Running"),
            dap::HostStatus::Running(false) => trace!("Stopped"),
        }
    }
}
