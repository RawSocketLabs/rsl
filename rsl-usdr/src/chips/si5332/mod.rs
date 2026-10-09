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
//!    [`PllReference`](input::PllReference) picks which feeds the PLL.
//!    [`InputMode`](input::InputMode) sets how clock input 2 is received.
//! 2. **PLL.** The PLL locks its VCO, somewhere in 2.375–2.625 GHz, to the reference.
//! 3. **Dividers.** Five high-speed dividers split the VCO by integers. Two interpolative dividers
//!    split it by fractions, and can add spread spectrum
//!    ([`SpreadSpectrum`](divider::SpreadSpectrum)).
//! 4. **Output mux.** Each output takes either a divider's clock or, bypassing the PLL, the
//!    reference itself ([`OutputSource`](output::OutputSource)).
//! 5. **Output stage.** Each output divides again ([`Divider`](output::Divider)), can be delayed
//!    ([`Skew`](output::Skew)) or inverted ([`Polarity`](output::Polarity)), and drives its pins as
//!    CMOS, LVDS, LVPECL or HCSL ([`DriverMode`](output::DriverMode),
//!    [`CmosDrive`](output::CmosDrive)).
//!
//! Unused stages can be powered down ([`InputPowerDown`](input::InputPowerDown),
//! [`DividerPowerDown`](divider::DividerPowerDown), [`SourcePowerDown`](output::SourcePowerDown)
//! and the output power-down registers).
//!
//! # The READY/ACTIVE state machine
//!
//! The chip runs in one of two states, requested through [`RequestedState`](state::RequestedState)
//! and reported in [`CurrentState`](state::CurrentState): READY, for changing the configuration,
//! and ACTIVE, for running it. libusdr's init and sample-rate plans request READY, write, then
//! request ACTIVE, and poll the state until it reads READY, ACTIVE or "no input clock". Not
//! verified here: what the outputs do while the chip is in READY. If they stop, every such change
//! briefly interrupts the LMS6002D's PLL reference and the sample clocks.
//!
//! That matters for the band-crossing problem. `si5332_set_port3_en`, which libusdr calls when the
//! RX path moves into or out of the board-mixer path, writes the output enables, then requests
//! READY, writes the power-down registers and requests ACTIVE, without polling. It gates output 3
//! (the mixer LO) by the mixer state and output 2 (the TX clock) by whether TX runs. The suspected
//! cause of the band-crossing failures is that READY cycle; the planned fix gates output 3 without
//! leaving ACTIVE, and will be checked against the oracle and hardware.
//!
//! # How the driver uses it
//!
//! At power-up, [`Si5332::program`] checks a Si5332 answers, then writes a plan that routes
//! the reference straight to outputs 0 to 2 and turns the rest off; the PLL is not yet used.
//! [`Si5332::wait_active`] then waits for the chip to run. On revision 3 the reference
//! oscillator starts only after this, so the chip may report no input clock; the board
//! sequence tolerates exactly that error. Setting a sample rate (not yet ported) will usually
//! move the sample clocks onto PLL dividers.
//!
//! # Register map
//!
//! One flat address space ([`reg`]); the value types are split by stage, as above: [`input`],
//! [`divider`], [`output`], and the [`state`] machine.

mod divider;
mod input;
mod output;
mod reg;
mod si5332;
mod state;

pub(crate) use si5332::{LvpeclOutput, Reference, Si5332};
