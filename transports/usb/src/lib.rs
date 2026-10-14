//! Owned USB host access: find a device, open, reset and claim it, then queue transfers on
//! its endpoints and wait for them on the calling thread.
//!
//! ```no_run
//! # use std::time::Duration;
//! # fn main() -> Result<(), rsl_usb::Error> {
//! let info = rsl_usb::devices()?
//!     .into_iter()
//!     .find(|d| (d.vendor_id(), d.product_id()) == (0x3727, 0x1001))
//!     .ok_or(rsl_usb::Error::NotFound)?;
//! let device = info.open()?.reset()?;
//! let interface = device.claim_interface(0)?;
//! let mut rx = interface.in_queue(0x83)?;
//! for _ in 0..8 {
//!     let buffer = rx.alloc(64 * 1024)?;
//!     rx.submit(buffer).map_err(|rejected| rejected.error)?;
//! }
//! if let Some(done) = rx.wait(Duration::from_millis(5))? {
//!     done.status?;
//!     println!("{} bytes", done.buffer.len());
//!     rx.submit(done.buffer).map_err(|rejected| rejected.error)?;
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Model
//!
//! - [`Interface`] and the queues are owned handles that share the open device, so a driver
//!   can keep a receive queue running while it uses other endpoints, from one thread or
//!   several. The device stays open until the last of them is dropped.
//! - A queue owns its transfers' buffers while they are in flight; [`InQueue::wait`] hands
//!   each one back, in submission order, with its status. Dropping a queue cancels its
//!   transfers and waits for them to come back before freeing anything.
//! - Completions are collected by whichever thread is waiting, one at a time, and filed under
//!   the endpoint they belong to: there is no background thread.
//!
//! # Bounded completion driving
//!
//! [`Interface::reap`] collects a bounded number of completed transfers across the open
//! device. Each queue's `try_complete` takes only already-filed completions; it never
//! performs I/O. Both return [`Error::Busy`] rather than waiting for the dispatcher lock.
//! Do not wait for a new readiness edge after [`ReapProgress::BudgetExhausted`] or
//! [`Error::Busy`]: the completions behind the last edge may still be in the backend, or
//! filed by the thread that held the lock. Caller processing/admission budgets remain the
//! driver's concern. Register a `Readiness` (feature `mio`) to wait for completions; while it
//! lives, blocking calls on the device leave waiting for readiness to its reactor.
//!
//! ```no_run
//! # use std::num::NonZeroUsize;
//! # use rsl_usb::{Error, Interface, InQueue, ReapProgress};
//! /// One bounded turn; `true` means turn again before waiting for readiness.
//! fn turn(interface: &Interface, rx: &mut InQueue) -> Result<bool, Error> {
//! let budget = NonZeroUsize::new(16).expect("nonzero budget");
//! let progress = match interface.reap(budget) {
//!     Err(Error::Busy) => return Ok(true),
//!     other => other?,
//! };
//! let mut filed_drained = false;
//! for _ in 0..budget.get() {
//!     let done = match rx.try_complete() {
//!         Err(Error::Busy) => return Ok(true),
//!         other => other?,
//!     };
//!     let Some(done) = done else {
//!         filed_drained = true;
//!         break;
//!     };
//!     done.status?;
//!     // Consume the received bytes before handing this buffer back to the kernel.
//!     rx.submit(done.buffer).map_err(|rejected| rejected.error)?;
//! }
//! // Repeat this for every active queue; pending transfers alone do not require a turn.
//! Ok(progress == ReapProgress::BudgetExhausted || !filed_drained)
//! }
//! ```
//!
//! # Caller-owned cleanup workers
//!
//! Cancellation and queue Drop are blocking. Linux's `USBDEVFS_DISCARDURB` uses
//! [`usb_kill_urb`](https://github.com/torvalds/linux/blob/v6.12/drivers/usb/core/devio.c#L1880-L1899);
//! an ioctl can also wait on the kernel's device lock. Moving the *entire queue* to a
//! caller-owned worker preserves every in-flight allocation through discard, reap and
//! failure-path Drop. Never join that worker on a reactor thread. Dropping its observer
//! does not cancel cleanup or return kernel-owned memory. A driver must also isolate
//! other ioctls on that device while cancellation can hold its kernel lock; these
//! primitives are not a complete nonblocking OS executor or managed driver.
//!
//! ```no_run
//! # use std::sync::mpsc::{self, Receiver};
//! # use std::thread;
//! # use rsl_usb::{Buffer, Error, InQueue};
//! # fn retire(mut rx: InQueue, wake: impl FnOnce() + Send + 'static)
//! #     -> Receiver<Result<Vec<Buffer>, Error>> {
//! let (finished, observer) = mpsc::channel();
//! thread::spawn(move || {
//!     let result = rx.cancel_all();
//!     drop(rx); // Failure-path cleanup stays on the worker too.
//!     drop(finished.send(result)); // Observer may already be gone; owned buffers are safe.
//!     wake(); // Notify through the caller's existing shared wake hook.
//! });
//! observer
//! # }
//! ```
//!
//! # Platforms
//!
//! Linux only so far, through usbfs (`/dev/bus/usb`); elsewhere the crate is empty. The API
//! carries no Linux concepts, so other backends can follow. Opening a device needs write
//! access to its node, normally granted by a udev rule.
#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(target_os = "linux")]
mod buffer;
#[cfg(target_os = "linux")]
mod device;
#[cfg(target_os = "linux")]
mod dispatch;
#[cfg(target_os = "linux")]
mod enumerate;
#[cfg(target_os = "linux")]
mod error;
#[cfg(target_os = "linux")]
mod queue;
#[cfg(all(target_os = "linux", feature = "mio"))]
mod readiness;
#[cfg(target_os = "linux")]
mod usbfs;

#[cfg(target_os = "linux")]
pub use buffer::Buffer;
#[cfg(target_os = "linux")]
pub use device::{Device, Interface, Setup};
#[cfg(target_os = "linux")]
pub use dispatch::ReapProgress;
#[cfg(target_os = "linux")]
pub use enumerate::{DeviceInfo, PortPath, devices};
#[cfg(target_os = "linux")]
pub use error::{Error, TransferError};
#[cfg(target_os = "linux")]
pub use queue::{Completion, InQueue, OutQueue, Rejected};
#[cfg(all(target_os = "linux", feature = "mio"))]
pub use readiness::Readiness;
