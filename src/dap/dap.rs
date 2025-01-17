mod command;
mod request;
mod response;
mod state;

pub use command::*;
pub use request::*;
pub use response::*;

use state::State;

use super::{swd, swj};

pub const DAP1_PACKET_SIZE: u16 = 64;
pub const DAP2_PACKET_SIZE: u16 = 512;

/// LED control trait.
pub trait DapLeds {
    /// React to host status, usually setting some LEDs
    fn react_to_host_status(&mut self, host_status: HostStatus);
}

/// DAP handler.
pub struct Dap<'a, DEPS, LEDS, SWD> {
    state: State<DEPS, SWD>,
    swd_wait_retries: usize,
    match_retries: usize,
    version_string: &'a str,
    // mode: Option<DapMode>,
    leds: LEDS,
}

impl<'a, DEPS, LEDS, SWD> Dap<'a, DEPS, LEDS, SWD>
where
    DEPS: swj::Dependencies<SWD>,
    LEDS: DapLeds,
    SWD: swd::Swd<DEPS>,
{
    /// Create a Dap handler
    pub fn new(dependencies: DEPS, leds: LEDS, version_string: &'a str) -> Self {
        assert!(SWD::AVAILABLE);

        Dap {
            state: State::new(dependencies),
            swd_wait_retries: 5,
            match_retries: 8,
            version_string,
            // mode: None,
            leds,
        }
    }

    /// Process a new CMSIS-DAP command from `report`.
    ///
    /// Returns number of bytes written to response buffer.
    pub fn process_command(
        &mut self,
        report: &[u8],
        rbuf: &mut [u8],
        version: DapVersion,
    ) -> usize {
        let req = match Request::from_report(report) {
            Some(req) => req,
            None => return 0,
        };

        let resp = &mut ResponseWriter::new(req.command, rbuf);

        // defmt::trace!("Dap command: {}", req.command);

        match req.command {
            Command::DAP_Info => self.process_info(req, resp, version),
            Command::DAP_HostStatus => self.process_host_status(req, resp),
            Command::DAP_Connect => self.process_connect(req, resp),
            Command::DAP_Disconnect => self.process_disconnect(req, resp),
            Command::DAP_WriteABORT => self.process_write_abort(req, resp),
            Command::DAP_Delay => self.process_delay(req, resp),
            Command::DAP_ResetTarget => self.process_reset_target(req, resp),
            Command::DAP_SWJ_Pins => self.process_swj_pins(req, resp),
            Command::DAP_SWJ_Clock => self.process_swj_clock(req, resp),
            Command::DAP_SWJ_Sequence => self.process_swj_sequence(req, resp),
            Command::DAP_SWD_Configure => self.process_swd_configure(req, resp),
            Command::DAP_SWD_Sequence => todo!(),
            Command::DAP_SWO_Transport => todo!(),
            Command::DAP_SWO_Mode => todo!(),
            Command::DAP_SWO_Baudrate => todo!(),
            Command::DAP_SWO_Control => todo!(),
            Command::DAP_SWO_Status => todo!(),
            Command::DAP_SWO_ExtendedStatus => todo!(),
            Command::DAP_SWO_Data => todo!(),
            Command::DAP_JTAG_Configure => todo!(),
            Command::DAP_JTAG_IDCODE => todo!(),
            Command::DAP_JTAG_Sequence => todo!(),
            Command::DAP_TransferConfigure => self.process_transfer_configure(req, resp),
            Command::DAP_Transfer => self.process_transfer(req, resp),
            Command::DAP_TransferBlock => self.process_transfer_block(req, resp),
            Command::DAP_TransferAbort => {
                self.process_transfer_abort();
                // Do not send a response for transfer abort commands
                return 0;
            }
            Command::DAP_ExecuteCommands => todo!(),
            Command::DAP_QueueCommands => todo!(),
            Command::Unimplemented => {}
        }

        resp.idx
    }

    /// Suspend the interface.
    pub fn suspend(&mut self) {
        self.state.to_none();

        if let State::None { deps, .. } = &mut self.state {
            deps.high_impedance_mode();
        } else {
            unreachable!();
        }
    }

    fn process_info(&mut self, mut req: Request, resp: &mut ResponseWriter, version: DapVersion) {
        match DapInfoID::try_from(req.next_u8()) {
            // Return 0-length string for VendorID, ProductID, SerialNumber
            // to indicate they should be read from USB descriptor instead
            Ok(DapInfoID::VendorID) => resp.write_u8(0),
            Ok(DapInfoID::ProductID) => resp.write_u8(0),
            Ok(DapInfoID::SerialNumber) => resp.write_u8(0),
            // Return git version as firmware version
            Ok(DapInfoID::FirmwareVersion) => {
                resp.write_u8(self.version_string.len() as u8);
                resp.write_slice(self.version_string.as_bytes());
            }
            // Return 0-length string for TargetVendor and TargetName to indicate
            // unknown target device.
            Ok(DapInfoID::TargetVendor) => resp.write_u8(0),
            Ok(DapInfoID::TargetName) => resp.write_u8(0),
            Ok(DapInfoID::Capabilities) => {
                resp.write_u8(1);
                // Bit 0: SWD supported
                // Bit 1: JTAG supported
                // Bit 2: SWO UART supported
                // Bit 3: SWO Manchester not supported
                // Bit 4: Atomic commands not supported
                // Bit 5: Test Domain Timer not supported
                // Bit 6: SWO Streaming Trace supported
                let swd = (SWD::AVAILABLE as u8) << 0;
                let jtag = 0 << 1;
                let swo = 0 << 2 | 0 << 3;
                let atomic = 0 << 4;
                let swo_streaming = 0 << 6;
                resp.write_u8(swd | jtag | swo | atomic | swo_streaming);
            }
            Ok(DapInfoID::SWOTraceBufferSize) => {
                resp.write_u8(4);
                let size = 0;
                resp.write_u32(size as u32);
            }
            Ok(DapInfoID::MaxPacketCount) => {
                resp.write_u8(1);
                // Maximum of one packet at a time
                resp.write_u8(1);
            }
            Ok(DapInfoID::MaxPacketSize) => {
                resp.write_u8(2);
                match version {
                    DapVersion::V1 => {
                        // Maximum of 64 bytes per packet
                        resp.write_u16(DAP1_PACKET_SIZE);
                    }
                    DapVersion::V2 => {
                        // Maximum of 512 bytes per packet
                        resp.write_u16(DAP2_PACKET_SIZE);
                    }
                }
            }
            _ => resp.write_u8(0),
        }
    }

    fn process_host_status(&mut self, mut req: Request, resp: &mut ResponseWriter) {
        let status_type = req.next_u8();
        let status_status = req.next_u8();
        // Use HostStatus to set our LED when host is connected to target
        if let Ok(status) = HostStatusType::try_from(status_type) {
            let status_value = status_status != 0;
            let status = match status {
                HostStatusType::Connect => HostStatus::Connected(status_value),
                HostStatusType::Running => HostStatus::Running(status_value),
            };

            self.leds.react_to_host_status(status);
        }
        resp.write_u8(0);
    }

    fn process_connect(&mut self, mut req: Request, resp: &mut ResponseWriter) {
        let port = req.next_u8();
        let port = match ConnectPort::try_from(port) {
            Ok(port) => port,
            Err(_) => {
                resp.write_u8(ConnectPortResponse::Failed as u8);
                return;
            }
        };

        // defmt::info!(
        // "DAP connect: {}, SWD: {}, JTAG: {}",
        // port,
        // SWD::AVAILABLE,
        // JTAG::AVAILABLE
        // );

        match (SWD::AVAILABLE, port) {
            // SWD
            (true, ConnectPort::Default) | (true, ConnectPort::SWD) => {
                self.state.to_swd();
                resp.write_u8(ConnectPortResponse::SWD as u8);
            }

            // Error (tried to connect JTAG or SWD when not available)
            (true, ConnectPort::JTAG)
            | (false, ConnectPort::Default)
            | (false, ConnectPort::JTAG)
            | (false, ConnectPort::SWD) => {
                resp.write_u8(ConnectPortResponse::Failed as u8);
            }
        }
    }

    fn process_disconnect(&mut self, _req: Request, resp: &mut ResponseWriter) {
        self.state.to_none();

        if let State::None { deps, .. } = &mut self.state {
            deps.high_impedance_mode();
        } else {
            unreachable!();
        }

        resp.write_ok();
    }

    fn process_write_abort<'b>(&mut self, mut req: Request<'b>, resp: &mut ResponseWriter<'b>) {
        self.state.to_last_mode();

        let word = req.next_u32();
        match (SWD::AVAILABLE, &mut self.state) {
            (true, State::Swd(swd)) => {
                match swd.write_dp(self.swd_wait_retries, swd::DPRegister::DPIDR, word) {
                    Ok(_) => resp.write_ok(),
                    Err(_) => resp.write_err(),
                }
            }
            _ => {
                resp.write_err();
            }
        }
    }

    fn process_delay<'b>(&mut self, mut req: Request<'b>, _resp: &mut ResponseWriter<'b>) {
        let delay = req.next_u16() as u64;
        todo!("Delay for {} us", delay);
        // resp.write_ok();
    }

    fn process_reset_target(&mut self, _req: Request, resp: &mut ResponseWriter) {
        resp.write_ok();
        // "No device specific reset sequence is implemented"
        resp.write_u8(0);
    }

    fn process_swj_pins(&mut self, mut req: Request, resp: &mut ResponseWriter) {
        let output = swj::Pins::from_bits_truncate(req.next_u8());
        let mask = swj::Pins::from_bits_truncate(req.next_u8());
        let wait_us = req.next_u32().min(3_000_000); // Defined as max 3 seconds

        self.state.to_none();

        if let State::None { deps, .. } = &mut self.state {
            resp.write_u8(deps.process_swj_pins(output, mask, wait_us).bits());
        } else {
            unreachable!();
        }
    }

    fn process_swj_clock(&mut self, mut req: Request, resp: &mut ResponseWriter) {
        let max_frequency = req.next_u32();
        let valid = self.state.set_clock(max_frequency);

        if valid {
            resp.write_ok();
        } else {
            resp.write_err();
        }
    }

    fn process_swj_sequence<'b>(&mut self, mut req: Request<'b>, resp: &mut ResponseWriter<'b>) {
        let nbits: usize = match req.next_u8() {
            // CMSIS-DAP says 0 means 256 bits
            0 => 256,
            // Other integers are normal.
            n => n as usize,
        };

        let payload = req.rest();
        let nbytes = (nbits + 7) / 8;
        let seq = if nbytes <= payload.len() {
            &payload[..nbytes]
        } else {
            resp.write_err();
            return;
        };

        self.state.to_none();

        if let State::None { deps, .. } = &mut self.state {
            deps.process_swj_sequence(seq, nbits);
        } else {
            unreachable!();
        }

        resp.write_ok();
    }

    fn process_swd_configure(&mut self, mut req: Request, resp: &mut ResponseWriter) {
        // TODO: Do we want to support other configs?
        let config = req.next_u8();
        let clk_period = config & 0b011;
        let always_data = (config & 0b100) != 0;
        if clk_period == 0 && !always_data {
            resp.write_ok();
        } else {
            resp.write_err();
        }
    }

    fn process_transfer_configure(&mut self, mut req: Request, resp: &mut ResponseWriter) {
        // We don't support variable idle cycles
        // TODO: Should we?
        let _idle_cycles = req.next_u8();

        // Send number of wait retries through to SWD
        self.swd_wait_retries = req.next_u16() as usize;

        // Store number of match retries
        self.match_retries = req.next_u16() as usize;

        resp.write_ok();
    }

    fn process_transfer<'b>(&mut self, mut req: Request<'b>, resp: &mut ResponseWriter<'b>) {
        self.state.to_last_mode();

        let _idx = req.next_u8();
        let ntransfers = req.next_u8();
        let mut match_mask = 0xFFFF_FFFFu32;

        match &mut self.state {
            State::Swd(swd) => {
                // Skip two bytes in resp to reserve space for final status,
                // which we update while processing.
                resp.write_u16(0);

                for transfer_idx in 0..ntransfers {
                    // Store how many transfers we execute in the response
                    resp.write_u8_at(1, transfer_idx + 1);

                    // Parse the next transfer request
                    let transfer_req = req.next_u8();
                    let apndp = swd::APnDP::try_from(transfer_req & (1 << 0)).unwrap();
                    let rnw = swd::RnW::try_from((transfer_req & (1 << 1)) >> 1).unwrap();
                    let a = swd::DPRegister::try_from((transfer_req & (3 << 2)) >> 2).unwrap();
                    let vmatch = (transfer_req & (1 << 4)) != 0;
                    let mmask = (transfer_req & (1 << 5)) != 0;
                    let _ts = (transfer_req & (1 << 7)) != 0;

                    if rnw == swd::RnW::R {
                        // Issue register read
                        let mut read_value = if apndp == swd::APnDP::AP {
                            // Reads from AP are posted, so we issue the
                            // read and subsequently read RDBUFF for the data.
                            // This requires an additional transfer so we'd
                            // ideally keep track of posted reads and just
                            // keep issuing new AP reads, but our reads are
                            // sufficiently fast that for now this is simpler.
                            let rdbuff = swd::DPRegister::RDBUFF;
                            if swd
                                .read_ap(self.swd_wait_retries, a)
                                .check(resp.mut_at(2))
                                .is_none()
                            {
                                break;
                            }
                            match swd
                                .read_dp(self.swd_wait_retries, rdbuff)
                                .check(resp.mut_at(2))
                            {
                                Some(v) => v,
                                None => break,
                            }
                        } else {
                            // Reads from DP are not posted, so directly read the register.
                            match swd.read_dp(self.swd_wait_retries, a).check(resp.mut_at(2)) {
                                Some(v) => v,
                                None => break,
                            }
                        };

                        // Handle value match requests by retrying if needed.
                        // Since we're re-reading the same register the posting
                        // is less important and we can just use the returned value.
                        if vmatch {
                            let target_value = req.next_u32();
                            let mut match_tries = 0;
                            while (read_value & match_mask) != target_value {
                                match_tries += 1;
                                if match_tries > self.match_retries {
                                    break;
                                }

                                read_value = match swd
                                    .read(self.swd_wait_retries, apndp.into(), a)
                                    .check(resp.mut_at(2))
                                {
                                    Some(v) => v,
                                    None => break,
                                }
                            }

                            // If we didn't read the correct value, set the value mismatch
                            // flag in the response and quit early.
                            if (read_value & match_mask) != target_value {
                                resp.write_u8_at(1, resp.read_u8_at(1) | (1 << 4));
                                break;
                            }
                        } else {
                            // Save read register value
                            resp.write_u32(read_value);
                        }
                    } else {
                        // Write transfer processing

                        // Writes with match_mask set just update the match mask
                        if mmask {
                            match_mask = req.next_u32();
                            continue;
                        }

                        // Otherwise issue register write
                        let write_value = req.next_u32();
                        if swd
                            .write(self.swd_wait_retries, apndp, a, write_value)
                            .check(resp.mut_at(2))
                            .is_none()
                        {
                            break;
                        }
                    }
                }
            }
            _ => return,
        }
    }

    fn process_transfer_block<'b>(&mut self, mut req: Request<'b>, resp: &mut ResponseWriter<'b>) {
        self.state.to_last_mode();

        let _idx = req.next_u8();
        let ntransfers = req.next_u16();
        let transfer_req = req.next_u8();
        let apndp = swd::APnDP::try_from(transfer_req & (1 << 0)).unwrap();
        let rnw = swd::RnW::try_from((transfer_req & (1 << 1)) >> 1).unwrap();
        let a = swd::DPRegister::try_from((transfer_req & (3 << 2)) >> 2).unwrap();

        match &mut self.state {
            State::Swd(swd) => {
                // Skip three bytes in resp to reserve space for final status,
                // which we update while processing.
                resp.write_u16(0);
                resp.write_u8(0);

                // Keep track of how many transfers we executed,
                // so if there is an error the host knows where
                // it happened.
                let mut transfers = 0;

                // If reading an AP register, post first read early.
                if rnw == swd::RnW::R
                    && apndp == swd::APnDP::AP
                    && swd
                        .read_ap(self.swd_wait_retries, a)
                        .check(resp.mut_at(3))
                        .is_none()
                {
                    // Quit early on error
                    resp.write_u16_at(1, 1);
                    return;
                }

                for transfer_idx in 0..ntransfers {
                    transfers = transfer_idx;
                    if rnw == swd::RnW::R {
                        // Handle repeated reads
                        let read_value = if apndp == swd::APnDP::AP {
                            // For AP reads, the first read was posted, so on the final
                            // read we need to read RDBUFF instead of the AP register.
                            if transfer_idx < ntransfers - 1 {
                                match swd.read_ap(self.swd_wait_retries, a).check(resp.mut_at(3)) {
                                    Some(v) => v,
                                    None => break,
                                }
                            } else {
                                let rdbuff = swd::DPRegister::RDBUFF.into();
                                match swd
                                    .read_dp(self.swd_wait_retries, rdbuff)
                                    .check(resp.mut_at(3))
                                {
                                    Some(v) => v,
                                    None => break,
                                }
                            }
                        } else {
                            // For DP reads, no special care required
                            match swd.read_dp(self.swd_wait_retries, a).check(resp.mut_at(3)) {
                                Some(v) => v,
                                None => break,
                            }
                        };

                        // Save read register value to response
                        resp.write_u32(read_value);
                    } else {
                        // Handle repeated register writes
                        let write_value = req.next_u32();
                        let result = swd.write(self.swd_wait_retries, apndp, a, write_value);
                        if result.check(resp.mut_at(3)).is_none() {
                            break;
                        }
                    }
                }

                // Write number of transfers to response
                resp.write_u16_at(1, transfers + 1);
            }
            _ => return,
        }
    }

    fn process_transfer_abort(&mut self) {
        // We'll only ever receive an abort request when we're not already
        // processing anything else, since processing blocks checking for
        // new requests. Therefore there's nothing to do here.
    }
}

trait CheckResult<T> {
    /// Check result of an SWD transfer, updating the response status byte.
    ///
    /// Returns Some(T) on successful transfer, None on error.
    fn check(self, resp: &mut u8) -> Option<T>;
}

impl<T> CheckResult<T> for swd::Result<T> {
    fn check(self, resp: &mut u8) -> Option<T> {
        match self {
            Ok(v) => {
                *resp = 1;
                Some(v)
            }
            Err(swd::Error::AckWait) => {
                *resp = 2;
                None
            }
            Err(swd::Error::AckFault) => {
                *resp = 4;
                None
            }
            Err(_) => {
                *resp = (1 << 3) | 7;
                None
            }
        }
    }
}
