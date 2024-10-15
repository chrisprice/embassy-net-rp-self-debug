#![no_std]
#![no_main]

const THUNK: *const [fn(usize, usize, usize) -> !; 4] = 0x20000000 as _;

#[link_section = ".text"]
#[no_mangle]
fn init(address: usize, clock_or_zero: usize, op: usize/*Operation*/) -> ! {
    unsafe { (*THUNK)[0](address, clock_or_zero, op) }
}

#[link_section = ".text"]
#[no_mangle]
fn uninit(op: usize/*Operation*/) -> ! {
    unsafe { (*THUNK)[1](op, 0, 0) }
}

#[link_section = ".text"]
#[no_mangle]
fn program_page(address: usize, byte_len: usize, buffer: *const u8) -> ! {
    unsafe { (*THUNK)[2](address, byte_len, buffer as _) }
}

#[link_section = ".text"]
#[no_mangle]
fn erase_sector(address: usize) -> ! {
    unsafe { (*THUNK)[3](address, 0, 0) }
}
