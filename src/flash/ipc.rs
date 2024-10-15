use core::mem;
use core::sync::atomic::{AtomicU8, Ordering};

#[repr(C)]
pub struct Ipc {
    pub what: AtomicU8, // IpcWhat,
    pub regs: [usize; 3], // TODO: use IpcWhat from ./thunk
}

#[repr(u8)]
pub enum IpcWhat {
    Init = 1, // anything but zero
    Deinit,
    Program,
    Erase,
}

#[used]
pub static mut IPC: Ipc = Ipc::new();

impl Ipc {
    const fn new() -> Self {
        Self  {
            what: AtomicU8::new(0),
            regs: [0; 3],
        }
    }

    pub fn read_what(&self) -> Result<Option<IpcWhat>, u8> {
        let w: u8 = self.what.load(Ordering::Acquire);
        match w {
            0 => Ok(None),
            1 ..= 4 => Ok(Some(unsafe {
                // SAFETY: repr(u8) on IpcWhat
                mem::transmute(w)
            })),
            w => Err(w),
        }
    }
}

