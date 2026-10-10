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
#[cfg(target_os = "linux")]
mod usbfs;

#[cfg(target_os = "linux")]
pub use buffer::Buffer;
#[cfg(target_os = "linux")]
pub use device::{Device, Interface, Setup};
#[cfg(target_os = "linux")]
pub use enumerate::{DeviceInfo, PortPath, devices};
#[cfg(target_os = "linux")]
pub use error::{Error, TransferError};
#[cfg(target_os = "linux")]
pub use queue::{Completion, InQueue, OutQueue, Rejected};
