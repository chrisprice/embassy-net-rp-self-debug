#![no_std]
#![no_main]

type IpcFn = extern "C" fn(usize, usize, usize) -> !;

const P: usize = 0x20000000;
//const THUNK: *const [IpcFn; 4] = P as _;

#[link_section = ".text"]
#[no_mangle]
fn init(address: usize, clock_or_zero: usize, op: usize/*Operation*/) -> ! {
    unsafe {
        let p = core::hint::black_box(P) + 4 * 0;
        let p: IpcFn = core::mem::transmute(p);
        p(address, clock_or_zero, op)
    }
}

#[link_section = ".text"]
#[no_mangle]
fn uninit(op: usize/*Operation*/) -> ! {
    unsafe {
        let p = core::hint::black_box(P) + 4 * 1;
        let p: IpcFn = core::mem::transmute(p);
        p(op, 0, 0)
    }
}

#[link_section = ".text"]
#[no_mangle]
fn program_page(address: usize, byte_len: usize, buffer: *const u8) -> ! {
    unsafe {
        let p = core::hint::black_box(P) + 4 * 2;
        let p: IpcFn = core::mem::transmute(p);
        p(address, byte_len, buffer as _)
    }
}

#[link_section = ".text"]
#[no_mangle]
fn erase_sector(address: usize) -> ! {
    unsafe {
        let p = core::hint::black_box(P) + 4 * 3;
        let p: IpcFn = core::mem::transmute(p);
        p(address, 0, 0)
    }
}
