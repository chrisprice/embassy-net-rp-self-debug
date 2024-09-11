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

const IPC: *mut Ipc = 0x20032000 as _;

#[link_section = ".text"]
fn ipc(what: IpcWhat, regs: &[usize; 3]) {
    let ipc = unsafe { &mut *IPC };

    ipc.regs.copy_from_slice(regs);
    ipc.what.store(what as u8, Ordering::SeqCst);
}

#[link_section = ".text"]
fn ipc_wait() -> ! {
    let ipc = unsafe { &*IPC };

    while ipc.what.load(Ordering::Relaxed) > 0 {
    }

    let exit_code = 0;
    halt(exit_code)
}

#[link_section = ".text"]
fn halt(exit_code: usize) -> ! {
    unsafe {
        core::arch::asm!(
            "1: wfi\nb 1b",
            in("r0") exit_code,
            options(noreturn, nomem, nostack, preserves_flags)
        );
    }
}

#[link_section = ".text"]
#[no_mangle]
fn init(address: usize, clock_or_zero: usize, op: Operation) -> ! {
    ipc(
        IpcWhat::Initialised,
        &[address, clock_or_zero, op as _]
    );

    ipc_wait()
}

#[link_section = ".text"]
#[no_mangle]
fn uninit(op: Operation) -> ! {
    ipc(
        IpcWhat::Deinitalised,
        &[op as _, 0, 0]
    );

    ipc_wait()
}

#[link_section = ".text"]
#[no_mangle]
fn program_page(address: usize, byte_len: usize, buffer: *const u8) -> ! {
    ipc(
        IpcWhat::Programming,
        &[address, byte_len, buffer as _]
    );

    ipc_wait()
}

#[link_section = ".text"]
#[no_mangle]
fn erase_sector(address: usize) -> ! {
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
