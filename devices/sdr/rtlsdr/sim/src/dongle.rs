//! [`SimDongle`]: the dongle at the control seam.

use std::collections::{HashMap, VecDeque};
use std::fmt;

use crate::trace::{Control, Direction, Op, Transfer};
use crate::tuner::{R820t2, TunerRefusal};

/// Which control transfers reach what. The model's whole shape (a register store keyed by
/// `wValue`/`wIndex`, a tuner reached through a control transfer's data stage, an optional
/// gate bit, and a target chosen by the setup alone) is an assumption pending captures
/// (`RTL-GAP-04` to `RTL-GAP-06`, `RTL-GAP-08`, `RTL-GAP-13`, `RTL-GAP-14`), as are its
/// numbers; tests supply a synthetic wiring.
#[derive(Clone, Copy, Debug)]
pub struct Wiring {
    /// Where each control transfer goes, from its setup alone.
    pub route: fn(Control) -> Route,

    /// The register-store bit that must be set for the tuner to answer, if any.
    pub gate: Option<GateBit>,
}

/// Where [`Wiring::route`] sends a control transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// The register store; `wValue` and `wIndex` key the entry.
    Store,

    /// The tuner's I2C port, if the gate is open: a write's data stage is a page write, a
    /// read's is read from R0 on.
    Tuner {
        /// The first register of a write, when the setup carries it; `None` when it is the
        /// data stage's first byte. Reads ignore it.
        register: Option<u8>,
    },

    /// Nothing: the transfer stalls.
    Stall,
}

/// One bit of one register-store entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateBit {
    /// The entry's `wValue`.
    pub value: u16,

    /// The entry's `wIndex`.
    pub index: u16,

    /// The bit of the entry's first byte; 8 or more never opens.
    pub bit: u8,
}

/// Why the dongle refused an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The control transfer stalled: no wiring routes its request, or the tuner is
    /// gated off.
    Stall,

    /// The tuner did not acknowledge.
    Tuner(TunerRefusal),

    /// An RX call out of order (start while running, read or stop while stopped).
    RxState,

    /// No RX transfer is queued.
    RxEmpty,

    /// The RX buffer is shorter than the next transfer.
    RxShort,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stall => f.write_str("control transfer stalled"),
            Self::Tuner(refusal) => write!(f, "tuner: {refusal}"),
            Self::RxState => f.write_str("RX call out of order"),
            Self::RxEmpty => f.write_str("no RX transfer queued"),
            Self::RxShort => f.write_str("RX buffer shorter than the transfer"),
        }
    }
}

impl std::error::Error for Refusal {}

/// A simulated dongle; see the crate docs.
#[derive(Debug)]
pub struct SimDongle {
    /// How requests are routed.
    wiring: Wiring,

    /// The register store, by (`wValue`, `wIndex`).
    store: HashMap<(u16, u16), Vec<u8>>,

    /// The tuner behind the repeater.
    tuner: R820t2,

    /// RX transfers queued for the stream to hand over.
    rx_feed: VecDeque<Vec<u8>>,

    /// Whether the RX stream runs.
    rx_running: bool,

    /// Virtual time, in microseconds.
    now_us: u64,

    /// Every operation, in order.
    trace: Vec<Op>,
}

impl SimDongle {
    /// A powered dongle with an empty register store and a reset tuner.
    #[must_use]
    pub fn new(wiring: Wiring) -> Self {
        Self {
            wiring,
            store: HashMap::new(),
            tuner: R820t2::default(),
            rx_feed: VecDeque::new(),
            rx_running: false,
            now_us: 0,
            trace: Vec::new(),
        }
    }

    /// A control read into `data`; returns the bytes sent. A store entry shorter than
    /// `data` reads as zeros past its end.
    ///
    /// # Errors
    ///
    /// [`Refusal::Stall`] for a transfer routed nowhere or to a gated tuner;
    /// [`Refusal::Tuner`] if the tuner does not acknowledge.
    pub fn control_in(&mut self, control: Control, data: &mut [u8]) -> Result<usize, Refusal> {
        let sent = match (self.wiring.route)(control) {
            Route::Store => {
                let entry = self.store.get(&(control.value, control.index));
                let mut sent = vec![0; data.len()];
                for (byte, stored) in sent.iter_mut().zip(entry.into_iter().flatten()) {
                    *byte = *stored;
                }
                self.record(Direction::In, control, &sent);
                sent
            }
            Route::Tuner { .. } if self.gate_open() => {
                let sent = match self.tuner.read(data.len()) {
                    Ok(sent) => sent,
                    Err(refusal) => return Err(self.nak(Direction::In, control, refusal)),
                };
                self.record(Direction::In, control, &sent);
                self.trace.push(Op::TunerRead { len: sent.len() });
                sent
            }
            Route::Tuner { .. } | Route::Stall => return Err(self.stall(Direction::In, control)),
        };
        data[..sent.len()].copy_from_slice(&sent);
        Ok(sent.len())
    }

    /// A control write of `data`.
    ///
    /// # Errors
    ///
    /// As [`control_in`](Self::control_in).
    pub fn control_out(&mut self, control: Control, data: &[u8]) -> Result<(), Refusal> {
        match (self.wiring.route)(control) {
            Route::Store => {
                self.store
                    .insert((control.value, control.index), data.to_vec());
                self.record(Direction::Out, control, data);
            }
            Route::Tuner { register } if self.gate_open() => {
                let page: Vec<u8> = register.into_iter().chain(data.iter().copied()).collect();
                if let Err(refusal) = self.tuner.write(&page) {
                    return Err(self.nak(Direction::Out, control, refusal));
                }
                self.record(Direction::Out, control, data);
                self.trace.push(Op::TunerWrite {
                    register: page[0],
                    data: page[1..].to_vec(),
                });
            }
            Route::Tuner { .. } | Route::Stall => {
                return Err(self.stall(Direction::Out, control));
            }
        }
        Ok(())
    }

    /// Queues one RX transfer for the stream to hand over.
    pub fn feed_rx(&mut self, transfer: Vec<u8>) {
        self.rx_feed.push_back(transfer);
    }

    /// Starts the RX stream.
    ///
    /// # Errors
    ///
    /// [`Refusal::RxState`] if it runs.
    pub fn rx_start(&mut self, transfer_len: usize, depth: usize) -> Result<(), Refusal> {
        if self.rx_running {
            return Err(Refusal::RxState);
        }
        self.rx_running = true;
        self.trace.push(Op::RxStart {
            transfer_len,
            depth,
        });
        Ok(())
    }

    /// Hands over the oldest queued RX transfer into `data`; returns its length.
    ///
    /// # Errors
    ///
    /// [`Refusal::RxState`] if the stream is stopped, [`Refusal::RxEmpty`] if nothing is
    /// queued, [`Refusal::RxShort`] if `data` cannot hold it (it stays queued).
    pub fn rx_read(&mut self, data: &mut [u8]) -> Result<usize, Refusal> {
        if !self.rx_running {
            return Err(Refusal::RxState);
        }
        let next = self.rx_feed.front().ok_or(Refusal::RxEmpty)?;
        let target = data.get_mut(..next.len()).ok_or(Refusal::RxShort)?;
        target.copy_from_slice(next);
        let len = next.len();
        self.rx_feed.pop_front();
        self.trace.push(Op::RxRead { len });
        Ok(len)
    }

    /// Stops the RX stream.
    ///
    /// # Errors
    ///
    /// [`Refusal::RxState`] if it is stopped.
    pub fn rx_stop(&mut self) -> Result<(), Refusal> {
        if !self.rx_running {
            return Err(Refusal::RxState);
        }
        self.rx_running = false;
        self.trace.push(Op::RxStop);
        Ok(())
    }

    /// Advances virtual time.
    pub fn sleep_us(&mut self, us: u64) {
        self.now_us += us;
        self.trace.push(Op::Sleep { us });
    }

    /// Virtual time since creation, in microseconds.
    #[must_use]
    pub fn now_us(&self) -> u64 {
        self.now_us
    }

    /// Every operation so far, in order.
    #[must_use]
    pub fn trace(&self) -> &[Op] {
        &self.trace
    }

    /// The tuner.
    #[must_use]
    pub fn tuner(&self) -> &R820t2 {
        &self.tuner
    }

    /// Whether the tuner answers: no gate, or its bit set.
    fn gate_open(&self) -> bool {
        let Some(GateBit { value, index, bit }) = self.wiring.gate else {
            return true;
        };
        self.store
            .get(&(value, index))
            .and_then(|entry| entry.first())
            .and_then(|byte| byte.checked_shr(u32::from(bit)))
            .is_some_and(|byte| byte & 1 == 1)
    }

    /// Records a completed transfer.
    fn record(&mut self, direction: Direction, control: Control, data: &[u8]) {
        let data = data.to_vec();
        self.trace.push(Op::Control(match direction {
            Direction::In => Transfer::In {
                control,
                len: u16::try_from(data.len()).unwrap_or(u16::MAX),
                data,
            },
            Direction::Out => Transfer::Out { control, data },
        }));
    }

    /// Records the transfer the tuner's refusal stalled, and returns the refusal.
    fn nak(&mut self, direction: Direction, control: Control, refusal: TunerRefusal) -> Refusal {
        self.stall(direction, control);
        Refusal::Tuner(refusal)
    }

    /// Records and returns a stall.
    fn stall(&mut self, direction: Direction, control: Control) -> Refusal {
        self.trace
            .push(Op::Control(Transfer::Stall { direction, control }));
        Refusal::Stall
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SYNTHETIC wiring, not RTL2832U values: one request whose `wIndex` picks the target,
    /// with the tuner's first register in `wValue`.
    fn by_index(control: Control) -> Route {
        match (control.request, control.index) {
            (0x01, 0x0000) => Route::Store,
            (0x01, 0x0010) => Route::Tuner {
                register: Some(control.value.to_le_bytes()[0]),
            },
            _ => Route::Stall,
        }
    }

    fn setup(index: u16, value: u16) -> Control {
        Control {
            request_type: 0x40,
            request: 0x01,
            value,
            index,
        }
    }

    #[test]
    fn a_wiring_can_route_on_windex() {
        let mut dongle = SimDongle::new(Wiring {
            route: by_index,
            gate: None,
        });
        dongle
            .control_out(setup(0x0010, 0x0005), &[0x0A, 0x0B])
            .unwrap();
        assert_eq!(dongle.tuner().registers()[5..7], [0x0A, 0x0B]);
        assert_eq!(
            dongle.trace()[1],
            Op::TunerWrite {
                register: 0x05,
                data: vec![0x0A, 0x0B],
            }
        );
        dongle.control_out(setup(0x0000, 0x0005), &[0x0C]).unwrap();
        assert_eq!(
            dongle.tuner().registers()[5],
            0x0A,
            "the store, not the tuner"
        );
        assert_eq!(
            dongle.control_out(setup(0x0011, 0x0005), &[0x0C]),
            Err(Refusal::Stall)
        );
    }
}
