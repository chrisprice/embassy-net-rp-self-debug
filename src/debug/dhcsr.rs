const TRANSFER_COMMAND_ID: u8 = 0x05;
const DAP_INDEX: u8 = 0x00;
const TRANSFER_COUNT: u8 = 0x02;
const TRANSFER_1_HEADER: u8 = 0x05;
const DHCSR_ADDRESS: [u8; 4] = [0xf0, 0xed, 0x00, 0xe0];
const TRANSFER_2_HEADER: u8 = 0x0d;
const DHCSR_ENABLE_WRITE: [u8; 4] = [0x00, 0x00, 0x5f, 0xa0];

const fn dhcsr(debugen: bool) -> [u8; 13] {
    let mut data = [0; 13];
    data[0] = TRANSFER_COMMAND_ID;
    data[1] = DAP_INDEX;
    data[2] = TRANSFER_COUNT;
    data[3] = TRANSFER_1_HEADER;
    data[4] = DHCSR_ADDRESS[0];
    data[5] = DHCSR_ADDRESS[1];
    data[6] = DHCSR_ADDRESS[2];
    data[7] = DHCSR_ADDRESS[3];
    data[8] = TRANSFER_2_HEADER;
    data[9] = if debugen { 0x01 } else { 0x00 } & DHCSR_ENABLE_WRITE[0];
    data[10] = DHCSR_ENABLE_WRITE[1];
    data[11] = DHCSR_ENABLE_WRITE[2];
    data[12] = DHCSR_ENABLE_WRITE[3];
    data
}

pub const DHCSR_CLEAR_DEBUGEN: [u8; 13] = dhcsr(false);
