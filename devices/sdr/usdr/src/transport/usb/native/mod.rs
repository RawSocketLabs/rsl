//! Native command/IRQ execution over a caller-owned Mio reactor.
//!
//! Only registration and bounded message handoff run on the reactor. Once attached, one
//! worker owns every USB queue and syscall, the readiness's Drop and failure-path Drop.
//! A failed attach releases them on the setup thread, or on the worker if only
//! registration failed. `NativeUsb` (feature `mio`) is the
//! public temperature-only pilot; it does not replace the blocking `UsbBus` or establish
//! RX/TX fairness or hardware qualification. Opening/claiming/configuring the interface is
//! blocking setup on `UsbBus`.

mod adapter;
mod backend;
mod handle;
mod worker;

#[cfg(test)]
pub(super) use adapter::Adapter;
pub(super) use backend::Queues;
#[cfg(test)]
pub(super) use backend::{Backend, Completion, Reply, Request, Transfer, Turn};
pub(super) use handle::Session;
pub use handle::{NativeEvent, NativeUsb};
#[cfg(test)]
pub(super) use worker::{Report, Work, Worker};
