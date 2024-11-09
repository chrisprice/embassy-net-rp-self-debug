#![allow(static_mut_refs)]

use core::{mem::size_of, sync::atomic::Ordering};
use defmt::info;

use super::ipc::IpcWhat;

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
        const BASE_ADDR: usize = 0x2003FC00;
        let functions: [extern "C" fn(usize, usize, usize) -> usize; 4] =
            [on_init, uninit, program_page, erase_sector];
        for (i, function) in functions.into_iter().enumerate() {
            // Space the addresses such that the flash_algorithm functions are each at the same offset
            // from their respective address.
            core::ptr::write_volatile(
                (BASE_ADDR + (i * 4) * size_of::<usize>()) as *mut usize,
                *((&function as *const _ as usize) as *const usize),
            );
        }
    }
}

extern "C" fn on_init(
    address: usize,
    clock_or_zero: usize,
    op: usize, /* Operation */
) -> usize {
    info!(
        "flash algo, executing on_init(address={:#x}, clk_or_zero={}, op={})",
        address, clock_or_zero, op
    );
    ipc(IpcWhat::Init, &[address, clock_or_zero, op as _]);
    info!("flash algo, posted IPC, waiting...");

    ipc_wait()
}

extern "C" fn uninit(op: usize /*Operation*/, _: usize, _: usize) -> usize {
    info!("flash algo, executing uninit(op={})", op);
    ipc(IpcWhat::Deinit, &[op as _, 0, 0]);

    ipc_wait()
}

extern "C" fn program_page(address: usize, byte_len: usize, buffer: usize) -> usize {
    info!(
        "flash algo, executing program_page(address={:#x}, byte_len={}, buffer={:#x})",
        address, byte_len, buffer,
    );
    let buffer = buffer as *const u8;

    ipc(IpcWhat::Program, &[address, byte_len, buffer as _]);

    ipc_wait()
}

extern "C" fn erase_sector(address: usize, _: usize, _: usize) -> usize {
    info!("flash algo, executing erase_sector(address={:#x})", address);
    ipc(IpcWhat::Erase, &[address, 0, 0]);

    ipc_wait()
}

fn ipc(what: IpcWhat, regs: &[usize; 3]) {
    let ipc = unsafe { &mut super::ipc::IPC };

    ipc.regs.copy_from_slice(regs); // FIXME: could use a &[usize] here / in callers
    ipc.what.store(what as u8, Ordering::SeqCst);
}

fn ipc_wait() -> usize {
    let ipc = unsafe { &super::ipc::IPC };

    cortex_m::interrupt::free(|_| while ipc.what.load(Ordering::Relaxed) > 0 {});

    info!("flash algo, got fin, exiting...");

    0
}
