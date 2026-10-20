//! A simulated RTL-SDR dongle at `rtlsdr`'s control seam, and the capture replay harness.
//!
//! [`SimDongle`] answers vendor control transfers with a generic register store and, behind
//! an I2C repeater gate, an R820T2 register file ([`R820t2`]) modelled from the Rafael
//! register description; it also feeds a bulk RX stream, keeps a virtual clock and records
//! every operation ([`Op`]). Which requests reach the store or the tuner, and which bit is
//! the gate, are [`Wiring`] the caller supplies: the RTL2832U's real ones are GAP rows in
//! `rtlsdr`'s `PROVENANCE.md` until captures give them, so this crate holds none.
//!
//! [`Trace`] is the recorded-session format (a header with the capture id, then one control
//! transfer per line) and [`Replay`] plays one back against a driver session, reporting
//! the first [`Divergence`].
#![forbid(unsafe_code)]

mod dongle;
mod replay;
mod trace;
mod tuner;

pub use dongle::{GateBit, Refusal, SimDongle, Wiring};
pub use replay::{Divergence, ParseError, Replay, Trace};
pub use trace::{Control, Direction, Op, Transfer};
pub use tuner::{R820t2, TunerRefusal};
