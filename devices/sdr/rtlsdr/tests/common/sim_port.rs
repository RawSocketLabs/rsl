//! `control::ControlPort` over `rtlsdr-sim`: every transfer goes to the dongle and every
//! wait to its virtual clock.

use std::io;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use rtlsdr::control::{BusError, ControlPort, Setup};
use rtlsdr_sim::{Control, Refusal, SimDongle};

/// A shared simulated dongle; clone it to inspect the dongle while a `Device` owns the port.
#[derive(Clone)]
pub(crate) struct SimPort(Arc<Mutex<SimDongle>>);

impl SimPort {
    /// Wraps a dongle.
    pub(crate) fn new(dongle: SimDongle) -> Self {
        Self(Arc::new(Mutex::new(dongle)))
    }

    /// Locks the dongle.
    pub(crate) fn dongle(&self) -> MutexGuard<'_, SimDongle> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The simulator's form of a setup, its direction bit (USB 2.0 Table 9-2, bit 7) set for
/// IN as the usb port sets it.
fn control(setup: Setup, direction_in: bool) -> Control {
    Control {
        request_type: if direction_in {
            setup.request_type | 0x80
        } else {
            setup.request_type & !0x80
        },
        request: setup.request,
        value: setup.value,
        index: setup.index,
    }
}

/// A refusal as the seam reports it.
fn bus_error(refusal: Refusal) -> BusError {
    match refusal {
        Refusal::RxEmpty => BusError::Timeout,
        other => BusError::Io(io::Error::other(other)),
    }
}

impl ControlPort for SimPort {
    fn control_in(
        &mut self,
        setup: Setup,
        data: &mut [u8],
        _timeout: Duration,
    ) -> Result<usize, BusError> {
        self.dongle()
            .control_in(control(setup, true), data)
            .map_err(bus_error)
    }

    fn control_out(
        &mut self,
        setup: Setup,
        data: &[u8],
        _timeout: Duration,
    ) -> Result<(), BusError> {
        self.dongle()
            .control_out(control(setup, false), data)
            .map_err(bus_error)
    }

    fn rx_start(&mut self, transfer_len: usize, depth: NonZeroUsize) -> Result<(), BusError> {
        self.dongle()
            .rx_start(transfer_len, depth.get())
            .map_err(bus_error)
    }

    fn rx_read(&mut self, data: &mut [u8], _timeout: Duration) -> Result<usize, BusError> {
        self.dongle().rx_read(data).map_err(bus_error)
    }

    fn rx_stop(&mut self) -> Result<(), BusError> {
        self.dongle().rx_stop().map_err(bus_error)
    }

    fn sleep(&mut self, duration: Duration) {
        let us = u64::try_from(duration.as_micros())
            .expect("invariant: simulated delays fit in u64 microseconds");
        self.dongle().sleep_us(us);
    }
}
