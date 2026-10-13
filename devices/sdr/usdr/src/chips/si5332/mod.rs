//! Si5332 clock generator (source: `hw/si5332/si5332.c`; register meanings from libusdr's
//! `hw/si5332/si5332.yaml`, vendored under `oracle/libusdr`).
//!
//! On the uSDR it turns the reference into the PLL, RX and TX clocks, and drives the external RX
//! mixer's LO on output 3.
//!
//! # What it does
//!
//! The Si5332 takes one reference clock and produces up to six output clocks, each at its own
//! frequency and logic standard. A clock passes through these stages:
//!
//! 1. **Input.** The reference comes from the crystal-oscillator pins or a clock input;
//!    `PllReference` picks which feeds the PLL.
//!    `InputMode` sets how clock input 2 is received.
//! 2. **PLL.** The PLL locks its VCO, somewhere in 2.375–2.625 GHz, to the reference.
//! 3. **Dividers.** Five high-speed dividers split the VCO by integers. Two interpolative dividers
//!    split it by fractions, and can add spread spectrum
//!    (`SpreadSpectrum`).
//! 4. **Output mux.** Each output takes either a divider's clock or, bypassing the PLL, the
//!    reference itself (`OutputSource`).
//! 5. **Output stage.** Each output divides again (`Divider`), can be delayed
//!    (`Skew`) or inverted (`Polarity`), and drives its pins as
//!    CMOS, LVDS, LVPECL or HCSL (`DriverMode`,
//!    `CmosDrive`).
//!
//! Unused stages can be powered down (`InputPowerDown`,
//! `DividerPowerDown`, `SourcePowerDown`
//! and the output power-down registers).
//!
//! # The READY/ACTIVE state machine
//!
//! The chip runs in one of two states, requested through `RequestedState`
//! and reported in `CurrentState`: READY, for changing the configuration,
//! and ACTIVE, for running it. libusdr's init and sample-rate plans request READY, write, then
//! request ACTIVE, and poll the state until it reads READY, ACTIVE or "no input clock".
//! Skyworks' Si5332 Reference Manual (rev 0.6, register-table notes) says the chip drives no
//! outputs in READY, so each of these stops the LMS6002D's PLL reference and the sample
//! clocks for a moment; for power-up and a new sample rate that is by design.
//!
//! It also happens on every band crossing. `si5332_set_port3_en`, which libusdr calls when
//! the RX path moves into or out of the board-mixer path, writes the output enables, then
//! requests READY, writes the power-down registers and requests ACTIVE, without polling. That
//! clock stop is the likely cause of the band-crossing failures; not yet confirmed on
//! hardware. The driver instead flips output 3's enable bit, which the manual allows in
//! ACTIVE ([`Si5332::set_mixer_lo`]), an intentional divergence the parity tests whitelist
//! per scenario.
//!
//! # How the driver uses it
//!
//! At power-up, `ClockSequence::program` checks a Si5332 answers, then writes a plan that routes
//! the reference straight to outputs 0 to 2 and turns the rest off; the PLL is not yet used.
//! `ClockSequence::wait_active` then waits for the chip to run. On revision 3 the reference
//! oscillator starts only after this, so the chip may report no input clock; the board
//! sequence tolerates exactly that error. Setting a sample rate then usually moves the sample
//! clocks onto the PLL with `ClockSequence::layout`, whose divider plan is a `Layout`.
//! Tuning into or out of the mixer band switches output 3 alone (the portable
//! `SelectRxBand`).
//!
//! # Register map
//!
//! One flat address space; the address map, the value types (split by stage, as above) and
//! the sequence machines live in the portable core (`crate::portable::si5332`).

mod si5332;

pub(crate) use crate::portable::si5332::{LvpeclOutput, Reference};
pub(crate) use si5332::Si5332;
