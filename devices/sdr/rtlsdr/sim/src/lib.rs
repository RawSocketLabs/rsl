//! A simulated RTL-SDR dongle at `rtlsdr`'s control seam, and the capture replay harness.
//!
//! [`SimDongle`] answers vendor control transfers with a generic register store and,
//! optionally behind a gate bit in that store, an R820T2 register file ([`R820t2`])
//! modelled from the Rafael register description; it also feeds a bulk RX stream, keeps a
//! virtual clock and records every operation ([`Op`]). That shape is an assumption pending
//! captures (`RTL-GAP-12`, `RTL-GAP-13` in `rtlsdr`'s `PROVENANCE.md`), and which requests
//! reach the store or the tuner, and the gate, are [`Wiring`] the caller supplies: this
//! crate holds no RTL2832U value.
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
