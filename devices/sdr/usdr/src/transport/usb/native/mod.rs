//! Private native command/IRQ execution over a caller-owned Mio reactor.
//!
//! Only registration and bounded message handoff run on the reactor. Once attached, one
//! worker owns every USB queue and syscall, the readiness's Drop and failure-path Drop;
//! a failed attach releases them on the setup thread. This pilot does not replace
//! the blocking `UsbBus`, provide public handles, or establish RX/TX fairness or hardware
//! qualification. Opening/claiming/configuring the interface is blocking setup.

mod adapter;
mod backend;
mod worker;

pub(super) use adapter::Adapter;
pub(super) use backend::{Backend, Completion, Queues, Reply, Request, Transfer, Turn};
pub(super) use worker::{Report, Work, Worker};
