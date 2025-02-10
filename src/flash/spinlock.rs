//! Spinlock implementation copied from embassy-rp::multicore::critical_section_impl
use core::{
    future::{poll_fn, Future},
    sync::atomic::{fence, Ordering}, task::Poll,
};

use embassy_rp::pac;

/// This is a cross-core spinlock designed to prevent a deadlock, whereby both cores succeed
/// in simultaneously pausing each other.
///
/// The embassy-rp::flash::Flash methods check they are running on core0 (they will error if
/// invoked from core1) before explicitly pausing core1 via a private spinlock mechanism.
///
/// During debugging, the flash algorithm (proxied via core1) halts core0 via the debug port.
///
/// We can't be sure that the point at which this occurs isn't exactly as core0 has instructed
/// core1 to halt.
///
/// To prevent the deadlock, we acquire this spinlock prior to flash operations from core0 and
/// prior to establishing a debugger connection.
///
/// We do not use a critical section for this as it would severely limit the usefulness of the
/// debugging (i.e. the debugger would deadlock on every critical section).
///
/// Additionally to faciliate sending application messages between cores (e.g. application
/// network messages or higher-level network control). There needs to be a mechanism to prevent
/// the mutexes used by these techniques from deadlocking. This is achieved by using the same
/// spinlock mechanism.
pub type Spinlock30 = Spinlock<30>;

/// Guarded access to flash to prevent potential deadlock - see [`crate::flash_new::FlashSpinlock`].
pub async fn try_with_spinlock<A, F: Future<Output = R>, R>(
    func: impl FnOnce(A) -> F,
    args: A,
) -> Result<R, ()> {
    let Some(spinlock) = Spinlock30::try_claim() else {
        return Err(());
    };
    // Ensure the spinlock is acquired before calling the flash operation
    fence(Ordering::SeqCst);
    let result = func(args).await;
    // Ensure the spinlock is released after calling the flash operation
    fence(Ordering::SeqCst);
    drop(spinlock);
    Ok(result)
}

/// Guarded access to flash to prevent potential deadlock - see [`crate::flash_new::FlashSpinlock`].
pub async fn with_spinlock<A, F: Future<Output = R>, R>(
    func: impl FnOnce(A) -> F,
    args: A,
) -> R {
    let spinlock = poll_fn(|_| {
        if let Some(spinlock) = Spinlock30::try_claim() {
            Poll::Ready(spinlock)
        } else {
            Poll::Pending
        }
    }).await;
    // Ensure the spinlock is acquired before calling the flash operation
    fence(Ordering::SeqCst);
    let result = func(args).await;
    // Ensure the spinlock is released after calling the flash operation
    fence(Ordering::SeqCst);
    drop(spinlock);
    result
}

pub struct Spinlock<const N: usize>(core::marker::PhantomData<()>)
where
    Spinlock<N>: SpinlockValid;

impl<const N: usize> Spinlock<N>
where
    Spinlock<N>: SpinlockValid,
{
    /// Try to claim the spinlock. Will return `Some(Self)` if the lock is obtained, and `None` if the lock is
    /// already in use somewhere else.
    pub fn try_claim() -> Option<Self> {
        let lock = pac::SIO.spinlock(N).read();
        if lock > 0 {
            Some(Self(core::marker::PhantomData))
        } else {
            None
        }
    }

    /// Clear a locked spin-lock.
    ///
    /// # Safety
    ///
    /// Only call this function if you hold the spin-lock.
    pub unsafe fn release() {
        // Write (any value): release the lock
        pac::SIO.spinlock(N).write_value(1);
    }
}

impl<const N: usize> Drop for Spinlock<N>
where
    Spinlock<N>: SpinlockValid,
{
    fn drop(&mut self) {
        // This is safe because we own the object, and hence hold the lock.
        unsafe { Self::release() }
    }
}

pub trait SpinlockValid {}
impl SpinlockValid for Spinlock<30> {}
