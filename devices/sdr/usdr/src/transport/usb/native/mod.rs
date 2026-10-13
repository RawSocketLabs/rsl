//! Native command/IRQ and stream execution over a caller-owned Mio reactor.
//!
//! Only registration and bounded message handoff run on the reactor. Once attached, one
//! worker owns every USB queue and syscall, the readiness's Drop and failure-path Drop.
//! A failed attach releases them on the setup thread, or on the worker if only
//! registration failed. `NativeUsb` (feature `mio`) is the public pilot: it runs the
//! portable I²C and register/SPI machines, and any portable sequence of them, one transfer
//! at a time (stream-engine registers through the index window, `window`), and the RX/TX
//! streams through the blocking bus's `Rx`/`Tx` adapters over filed completions; the
//! worker copies RX blocks out and cancels streams. `ManagedUsb`
//! drives one on a reactor of its own for callers without one. It does not replace the
//! blocking `UsbBus` or establish fairness or hardware qualification. Opening/claiming/
//! configuring the interface is blocking setup on `UsbBus`.

mod adapter;
mod backend;
mod handle;
mod managed;
mod window;
mod worker;

#[cfg(test)]
pub(super) use adapter::{Adapter, RX_BACKLOG, StreamOutcome};
pub(super) use backend::Queues;
#[cfg(test)]
pub(super) use backend::{Backend, Completion, Filing, Reply, Request, Turn};
pub(super) use handle::Session;
pub use handle::{NativeEvent, NativeUsb, RxBlock};
pub use managed::ManagedUsb;
#[cfg(test)]
pub(super) use worker::{Report, StreamCommand, Work, Worker};
