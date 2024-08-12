#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU8, Ordering};

#[repr(C)]
pub enum Operation {
    Erase = 1,
    Program = 2,
    Verify = 3,
}

// #[repr(C)]
// enum IpcWhat {
//     Initialised {
//         address: *const u8,
//         clock_or_zero: usize,
//         op: Operation,
//     },
//     Deinitalised {
//         op: Operation,
//     },
//     Programming {
//         addr: *const u8,
//         byte_len: usize,
//         buffer: *const u8,
//     },
//     Erasing {
//         addr: *const u8,
//     },
// }

// 3 * size_of::<usize>() + usize = 4 words = 16 bytes

#[repr(C)]
enum IpcWhat {
    Initialised = 1, // anything but zero
    Deinitalised,
    Programming,
    Erasing,
}

#[repr(C)]
struct Ipc {
    what: AtomicU8, // IpcWhat,
    regs: [usize; 3],
}

const IPC: *mut Ipc = (
    0x20000000 + 256 * 1024 - 4 * core::mem::size_of::<usize>()
) as _; // last chunk of memory, space for 4 usize:s

fn ipc(what: IpcWhat, regs: &[usize; 3]) {
    let ipc = unsafe { &mut *IPC };

    ipc.regs.copy_from_slice(regs);
    ipc.what.store(what as u8, Ordering::Release);
}

fn ipc_wait() -> usize {
    let ipc = unsafe { &*IPC };

    while ipc.what.load(Ordering::Relaxed) > 0 {
        unsafe {
            core::arch::asm!("nop"); // no deps
        }
    }

    0
}

#[link_section = ".text"]
#[no_mangle]
fn init(address: usize, clock_or_zero: usize, op: Operation) -> usize {
    ipc(
        IpcWhat::Initialised,
        &[address, clock_or_zero, op as _]
    );

    ipc_wait()
}

#[link_section = ".text"]
#[no_mangle]
fn uninit(op: Operation) -> usize {
    ipc(
        IpcWhat::Deinitalised,
        &[op as _, 0, 0]
    );

    ipc_wait()
}

#[link_section = ".text"]
#[no_mangle]
fn program_page(address: usize, byte_len: usize, buffer: *const u8) -> usize {
    ipc(
        IpcWhat::Programming,
        &[address, byte_len, buffer as _]
    );

    ipc_wait()
}

#[link_section = ".text"]
#[no_mangle]
fn erase_sector(address: usize) -> usize {
    ipc(
        IpcWhat::Erasing,
        &[address, 0, 0]
    );

    ipc_wait()
}

/*
// necessary to link / a hack:
use defmt_rtt as _;

embassy_rp::bind_interrupts!(struct Irqs {

});

#[cortex_m_rt::entry]
fn main() -> ! {
    loop {}
}

#[panic_handler]
fn on_panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {}
}
*/
