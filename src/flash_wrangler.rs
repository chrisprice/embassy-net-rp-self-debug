use core::sync::atomic::{AtomicU8, Ordering};
use defmt::{info, warn};

#[repr(C)]
struct Ipc {
    what: AtomicU8, // IpcWhat,
    regs: [usize; 3],
}

#[repr(C)]
enum IpcWhat {
    Initialised = 1, // anything but zero
    Deinitalised,
    Programming,
    Erasing,
}

// TODO: no copy+paste
const IPC: *mut Ipc = (
    0x20000000 + 256 * 1024 - 4 * core::mem::size_of::<usize>()
) as _; // last chunk of memory, space for 4 usize:s

pub fn handle_pending_flash() {
    let ipc = unsafe { &*IPC };

    match ipc.what.load(Ordering::Acquire) {
        0 => return,

        // IpcWhat::...
        1 => {
            info!(
                "found init({:x}, {:x}, {:x}), pretending it was ok",
                ipc.regs[0],
                ipc.regs[1],
                ipc.regs[2],
            );
        }
        2 => {
            info!(
                "found deinit({:x}), pretending it was ok",
                ipc.regs[0],
            );
        }
        3 => {
            info!(
                "found program_page({:x}, {:x}, {:x}), pretending it was ok",
                ipc.regs[0],
                ipc.regs[1],
                ipc.regs[2],
            );
        }
        4 => {
            info!(
                "found erase_sector({:x}), pretending it was ok",
                ipc.regs[0],
            );
        }
        v => {
            warn!("unknown ipc value {}", v);
        }
    }

    ipc.what.store(0, Ordering::Relaxed);
}
