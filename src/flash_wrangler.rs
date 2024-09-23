use core::sync::atomic::{AtomicU8, Ordering};
use core::mem::MaybeUninit;
use defmt::{info, warn, error};

#[repr(C)]
pub struct Ipc {
    what: AtomicU8, // IpcWhat,
    regs: [usize; 3],
}

impl Ipc {
    const fn new() -> Self {
        Self  {
            what: AtomicU8::new(0),
            regs: [0; 3],
        }
    }
}

#[repr(C)]
enum IpcWhat {
    Initialised = 1, // anything but zero
    Deinitalised,
    Programming,
    Erasing,
}

// reserve the memory range for probe-rs to use:
#[used]
#[link_section = ".probe_rs_scratch"] // 0x2003a000..0x20042000
pub static mut SCRATCH: MaybeUninit<[u8; 10 * 1024]> = MaybeUninit::uninit();

#[used]
#[link_section = ".probe_rs_scratch"]
pub static mut IPC: Ipc = Ipc::new();

// TODO: no copy+paste of address
// see https://github.com/embassy-rs/embassy/blob/2537fc6f4fcbdaa0fcea45a37382d61f59cc5767/examples/boot/bootloader/rp/memory.x#L18-L21

pub fn init() {
    // `IPC` should be in .bss but if it goes in there, we can't fix its address, so it's a choice of:
    // - fixed address, we initialise
    // - unknown address, runtime initialises
    //
    // former has been chosen
    unsafe {
        // don't drop, since it's not initaliised
        use core::ptr::{write, addr_of_mut};
        write(addr_of_mut!(IPC), Ipc::new());
    }
}

pub fn handle_pending_flash() {
    let ipc = unsafe { &IPC };

    match ipc.what.load(Ordering::Acquire) {
        0 => return,

        // IpcWhat::...
        1 => {
            info!(
                "found init({:#x}, {:#x}, {:#x}), pretending it was ok",
                ipc.regs[0],
                ipc.regs[1],
                ipc.regs[2],
            );
        }
        2 => {
            info!(
                "found deinit({:#x}), pretending it was ok",
                ipc.regs[0],
            );
        }
        3 => {
            error!(
                "found program_page({:#x}, {:#x}, {:#x}), pretending it was ok - TODO, implement",
                ipc.regs[0],
                ipc.regs[1],
                ipc.regs[2],
            );
        }
        4 => {
            error!(
                "found erase_sector({:#x}), pretending it was ok - TODO, implement",
                ipc.regs[0],
            );
        }
        v => {
            warn!("unknown ipc value {}", v);
        }
    }

    ipc.what.store(0, Ordering::SeqCst);
}
