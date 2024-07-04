use crate::dap;

pub enum Swo {}

impl Swo {
    pub fn new() -> Option<Self> {
        None
    }
}

impl dap::swo::Swo for Swo {
    fn set_transport(&mut self, transport: dap::swo::SwoTransport) {
        todo!()
    }

    fn set_mode(&mut self, mode: dap::swo::SwoMode) {
        todo!()
    }

    fn set_baudrate(&mut self, baudrate: u32) -> u32 {
        todo!()
    }

    fn set_control(&mut self, control: dap::swo::SwoControl) {
        todo!()
    }

    fn polling_data(&mut self, buf: &mut [u8]) -> u32 {
        todo!()
    }

    fn streaming_data(&mut self) {
        todo!()
    }

    fn is_active(&self) -> bool {
        todo!()
    }

    fn bytes_available(&self) -> u32 {
        todo!()
    }

    fn buffer_size(&self) -> u32 {
        todo!()
    }

    fn support(&self) -> dap::swo::SwoSupport {
        todo!()
    }

    fn status(&mut self) -> dap::swo::SwoStatus {
        todo!()
    }
}