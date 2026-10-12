//! TPS63811 buck-boost converter, which supplies a 3.45 V rail (libusdr's "DC-DC boost").
//!
//! # What it does
//!
//! A buck-boost converter regulates its output whether its input is above or below it.
//! Assumed, not documented in libusdr: it is fed from the M.2 slot's 3.3 V supply, below
//! the 3.45 V the uSDR asks for. The TPS63811 starts with its output
//! off (`ENABLE` resets to 0) until software turns it on.
//!
//! The part has two output-voltage registers, and its VSEL pin picks which one is in
//! force. The driver writes 3.45 V to both, so the board's wiring of VSEL does not matter.
//!
//! libusdr calls the part `I2C_DEV_DCDCBOOST` and does not say which loads it feeds. It is
//! programmed before the clocks start, and the separate booster GPO line
//! ([`Gpo::Booster`](crate::fpga::Gpo::Booster)) switches on later, just before the
//! LMS6002D leaves reset.
//!
//! # How the driver uses it
//!
//! Power-up (the portable `PowerRails` machine) checks the part is TI silicon revision B0,
//! then enables it in forced PWM (fixed-frequency switching, for the reason given in the
//! [`lp8758`](super::lp8758) docs) at 3.45 V.
//!
//! # Sources
//!
//! Register meanings: TI SLVSEK4C, "TPS63810, TPS63811", §8.6 (also libusdr's
//! `hw/tps6381x/tps6381x.yaml`). Values written: libusdr `hw/tps6381x/tps6381x.c`.
//!
//! # Register map
//!
//! One flat address space; the address map and value types (control, output voltages,
//! device ID) live in the portable core (`crate::portable::tps6381x`).

mod tps6381x;

pub(crate) use crate::portable::tps6381x::OutputVoltage;
pub(crate) use tps6381x::Tps6381x;
