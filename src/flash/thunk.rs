#![allow(static_mut_refs)]

use core::sync::atomic::Ordering;
use defmt::info;

use super::ipc::IpcWhat;

#[used]
#[link_section = ".ipc_thunk"]
static mut ALGO_THUNK: [fn(usize, usize, usize) -> !; 4] = [
    on_init,
    uninit,
    program_page,
    erase_sector,
];

#[allow(dead_code)]
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

pub fn init() {
    unsafe {
        // FIXME: init ram via memory.x
        ALGO_THUNK = [on_init, uninit, program_page, erase_sector];
        info!("flash::thunk::init, ALGO_THUNK is {:?}", ALGO_THUNK);
    }
}

fn on_init(address: usize, clock_or_zero: usize, op: usize /* Operation */) -> ! {
    ipc(
        IpcWhat::Init,
        &[address, clock_or_zero, op as _]
    );

    ipc_wait()
}

fn uninit(op: usize /*Operation*/, _: usize, _: usize) -> ! {
    ipc(
        IpcWhat::Deinit,
        &[op as _, 0, 0]
    );

    ipc_wait()
}

fn program_page(address: usize, byte_len: usize, buffer: usize) -> ! {
    let buffer = buffer as *const u8;

    ipc(
        IpcWhat::Program,
        &[address, byte_len, buffer as _]
    );

    ipc_wait()
}

fn erase_sector(address: usize, _: usize, _: usize) -> ! {
    ipc(
        IpcWhat::Erase,
        &[address, 0, 0]
    );

    ipc_wait()
}

fn ipc(what: IpcWhat, regs: &[usize; 3]) {
    let ipc = unsafe { &mut super::ipc::IPC };

    ipc.regs.copy_from_slice(regs); // FIXME: could use a &[usize] here / in callers
    ipc.what.store(what as u8, Ordering::SeqCst);
}

fn ipc_wait() -> ! {
    let ipc = unsafe { &super::ipc::IPC };

    cortex_m::interrupt::free(|_| {
        while ipc.what.load(Ordering::Relaxed) > 0 {
        }
    });

    let exit_code = 0;
    halt(exit_code)
}

fn halt(exit_code: usize) -> ! {
    unsafe {
        core::arch::asm!(
            "1: wfi\nb 1b",
            in("r0") exit_code,
            options(noreturn, nomem, nostack, preserves_flags)
        );
    }
}
