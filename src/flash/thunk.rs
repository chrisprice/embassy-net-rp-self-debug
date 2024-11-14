#![allow(static_mut_refs)]

use core::{mem::size_of, sync::atomic::Ordering};
use defmt::info;
use embassy_rp::pac;

use super::ipc::IpcWhat;

static ALGO_THUNK: [extern "C" fn(usize, usize, usize) -> usize; 4] =
    [on_init, uninit, program_page, erase_sector];

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
    // TODO: Convert this to linker magic
    let size = size_of::<extern "C" fn(usize, usize, usize) -> usize>();
    let src = ALGO_THUNK.as_ptr();
    let base_address: usize = 0x21040000 - size * ALGO_THUNK.len(); // 0x2103FFF8
    unsafe {
        core::ptr::copy_nonoverlapping(src, base_address as *mut _, ALGO_THUNK.len());
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

unsafe fn SIO_IRQ_PROC1() {
    let sio = pac::SIO;
    // Clear IRQ
    sio.fifo().st().write(|w| w.set_wof(false));

    while sio.fifo().st().read().vld() {
        // Pause CORE1 execution and disable interrupts
        if fifo_read_wfe() == PAUSE_TOKEN {
            cortex_m::interrupt::disable();
            // Signal to CORE0 that execution is paused
            fifo_write(PAUSE_TOKEN);
            // Wait for `resume` signal from CORE0
            while fifo_read_wfe() != RESUME_TOKEN {
                cortex_m::asm::nop();
            }
            cortex_m::interrupt::enable();
            // Signal to CORE0 that execution is resumed
            fifo_write(RESUME_TOKEN);
        }
    }
}

const PAUSE_TOKEN: u32 = 0xDEADBEEF;
const RESUME_TOKEN: u32 = !0xDEADBEEF;

#[link_section = ".data.ram_func"]
fn ipc_wait() -> usize {
    let ipc = unsafe { &super::ipc::IPC };

    cortex_m::interrupt::free(|_| {
        while ipc.what.load(Ordering::Relaxed) > 0 {
            let sio = pac::SIO;
            if sio.fifo().st().read().vld() {
                // Pause CORE1 execution and disable interrupts
                if fifo_read_wfe() == PAUSE_TOKEN {
                    // Signal to CORE0 that execution is paused
                    fifo_write(PAUSE_TOKEN);
                    // Wait for `resume` signal from CORE0
                    while fifo_read_wfe() != RESUME_TOKEN {
                        cortex_m::asm::nop();
                    }
                    // Signal to CORE0 that execution is resumed
                    fifo_write(RESUME_TOKEN);
                }
            }
        }
    });

    info!("flash algo, got fin, exiting...");

    0
}

// Push a value to the inter-core FIFO, block until space is available
#[inline(always)]
fn fifo_write(value: u32) {
    let sio = pac::SIO;
    // Wait for the FIFO to have enough space
    while !sio.fifo().st().read().rdy() {
        cortex_m::asm::nop();
    }
    sio.fifo().wr().write_value(value);
    // Fire off an event to the other core.
    // This is required as the other core may be `wfe` (waiting for event)
    cortex_m::asm::sev();
}

// Pop a value from inter-core FIFO, block until available
#[inline(always)]
fn fifo_read() -> u32 {
    let sio = pac::SIO;
    // Wait until FIFO has data
    while !sio.fifo().st().read().vld() {
        cortex_m::asm::nop();
    }
    sio.fifo().rd().read()
}

// Pop a value from inter-core FIFO, `wfe` until available
#[inline(always)]
#[allow(unused)]
fn fifo_read_wfe() -> u32 {
    let sio = pac::SIO;
    // Wait until FIFO has data
    while !sio.fifo().st().read().vld() {
        cortex_m::asm::wfe();
    }
    sio.fifo().rd().read()
}

// Drain inter-core FIFO
#[inline(always)]
fn fifo_drain() {
    let sio = pac::SIO;
    while sio.fifo().st().read().vld() {
        let _ = sio.fifo().rd().read();
    }
}
