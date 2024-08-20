use core::sync::atomic::{AtomicU8, Ordering};
use defmt::{info, warn, error};

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
// see https://github.com/embassy-rs/embassy/blob/2537fc6f4fcbdaa0fcea45a37382d61f59cc5767/examples/boot/bootloader/rp/memory.x#L18-L21
const IPC: *mut Ipc = 0x20040000 as _;

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
            error!(
                "found program_page({:x}, {:x}, {:x}), pretending it was ok - TODO, implement",
                ipc.regs[0],
                ipc.regs[1],
                ipc.regs[2],
            );
        }
        4 => {
            error!(
                "found erase_sector({:x}), pretending it was ok - TODO, implement",
                ipc.regs[0],
            );
        }
        v => {
            warn!("unknown ipc value {}", v);
        }
    }

    ipc.what.store(0, Ordering::Relaxed);
}
