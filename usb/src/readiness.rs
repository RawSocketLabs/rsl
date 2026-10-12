//! Attaching an open device's completions to a caller-owned Mio reactor.

use std::io;
use std::os::fd::{AsRawFd, RawFd};

use mio::event::Source;
use mio::unix::SourceFd;
use mio::{Interest, Registry, Token};

use crate::device::Interface;
use crate::dispatch::Driving;

/// An open device's completion readiness, for a caller-owned [`mio::Poll`]: the device's only
/// completion driver while it lives.
///
/// Register it with [`Interest::WRITABLE`], which is how completions are reported; other
/// interests alone are refused. A writable event means transfers may have completed;
/// an error or write-closed event means the device went away. Each event, and each turn
/// that ends in [`crate::ReapProgress::BudgetExhausted`] or [`crate::Error::Busy`], calls
/// for another turn of [`Interface::reap`] and the queues' `try_complete` before waiting
/// again (see the crate docs). Spurious events are possible.
///
/// While it exists, blocking calls on the device's queues (`wait`, `read`, `write`,
/// `cancel_all` and Drop) collect only what has already completed and otherwise wait for the
/// reactor to file their transfers. Never call `wait`, `read` or `write` on the reactor's own
/// thread: nothing would reap for them, so they time out even when their transfer
/// completed. Cancellation and Drop still finish there without a reactor turn, because the
/// kernel completes a discarded transfer before the discard returns; a failed discard leaves
/// its transfer to the reactor, or to the grace-period leak.
///
/// It keeps its interface claimed and the device open, so [`crate::Device::reset`] refuses
/// while it lives. Dropping it deregisters it from the registry it was registered with.
/// Deregister it before dropping that [`mio::Poll`]: the retained registry keeps the epoll
/// instance, and this registration, alive, so another `Poll` cannot register it; recreate
/// it instead.
#[derive(Debug)]
pub struct Readiness {
    /// The claim, which keeps the device's node open.
    _interface: Interface,

    /// The node's descriptor, valid while `_interface` lives.
    node: RawFd,

    /// Excludes other readiness drivers and makes blocking waiters leave readiness to it.
    _driving: Driving,

    /// Where it is registered, kept for deregistration on Drop.
    registry: Option<Registry>,
}

impl Readiness {
    /// Readiness for `interface`'s device, driven under `driving`.
    pub(crate) fn new(interface: Interface, driving: Driving) -> Self {
        Self {
            node: interface.node().as_raw_fd(),
            _interface: interface,
            _driving: driving,
            registry: None,
        }
    }

    /// The node, as Mio registers it.
    fn fd(&self) -> SourceFd<'_> {
        SourceFd(&self.node)
    }
}

/// Checks that `interests` will deliver completions: usbfs reports them only as writable.
fn completions(interests: Interest) -> io::Result<Interest> {
    if interests.is_writable() {
        Ok(interests)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "USB completions are reported as writable readiness",
        ))
    }
}

impl Source for Readiness {
    fn register(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        if self.registry.is_some() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "already registered: reregister or deregister it first",
            ));
        }
        let interests = completions(interests)?;
        // Cloned first, so a failure cannot leave a registration that Drop cannot remove.
        let retained = registry.try_clone()?;
        self.fd().register(registry, token, interests)?;
        self.registry = Some(retained);
        Ok(())
    }

    fn reregister(
        &mut self,
        registry: &Registry,
        token: Token,
        interests: Interest,
    ) -> io::Result<()> {
        let interests = completions(interests)?;
        self.fd().reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
        self.fd().deregister(registry)?;
        self.registry = None;
        Ok(())
    }
}

impl Drop for Readiness {
    fn drop(&mut self) {
        // The node outlives this handle through other interfaces and queues, so its epoll
        // entry would otherwise keep firing this token and block the next registration.
        if let Some(registry) = self.registry.take() {
            let _ = self.fd().deregister(&registry);
        }
    }
}
