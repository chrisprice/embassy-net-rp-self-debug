#![no_std]
#![no_main]

use panic_probe as _;

#[repr(C)]
enum Operation {
    Erase = 1,
    Program = 2,
    Verify = 3,
}

#[no_mangle]
fn init(_address: usize, _clock_or_zero: usize, _op: Operation) {
}

#[no_mangle]
fn uninit(_op: Operation) {
}

#[no_mangle]
fn program_page(_address: usize, _byte_len: usize, _buffer: *const u8) {
}

#[no_mangle]
fn erase_sector(_address: usize) {
}
