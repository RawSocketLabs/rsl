//! TI LMK05318 network synchronizer clock: the clock tree of the `SimpleSync` and XMASS
//! front ends.
//!
//! # What it does
//!
//! A DPLL plus two APLLs that discipline a reference and fan it out on eight outputs.
//! APLL1 runs a 2.5 GHz BAW VCO from the XO; APLL2 (5.5–6.25 GHz) is cascaded from it, with
//! two post-dividers. Each output's mux picks APLL1 or one of APLL2's post-dividers, then
//! an output divider (output 7's has a second stage). Registers are bytes behind a 16-bit
//! address, sent most significant byte first.
//!
//! # How the driver uses it
//!
//! As libusdr's `hw/lmk05318/lmk05318.c` does for the uSDR's front ends: [`Lmk05318::create`]
//! checks the identity, plans the XO, APLL1 and the outputs ([`plan`]), stages those
//! registers with the common settings by address and writes them in address order
//! ([`map`]), then soft-resets. The boards then calibrate and wait for each APLL's lock,
//! synchronize the outputs, and later re-plan outputs on request. Without the DPLL, which
//! neither board enables (not ported), APLL1 runs free.
//!
//! These run as blocking code: the 16-bit addresses and four-byte identity read do not fit
//! the portable `Sequence` seam.
//!
//! The plan, staging map, address map and value types are the shared `lmk05318` crate
//! (`PE_SYNC` uses the chip too), re-exported here under their former module names; the
//! bus code in `lmk05318.rs` is the uSDR driver's.
//!
//! # Sources
//!
//! libusdr's `hw/lmk05318/lmk05318.c` and register YAML `lmk05318.yaml` (vendored beside
//! it) for fields and meanings. The YAML does not document addresses 0 to 3, so the
//! identity stays four raw bytes.

mod lmk05318;

pub(crate) use ::lmk05318::plan;
use ::lmk05318::{apll, control, map, output, reg, staging};

pub(crate) use self::lmk05318::Lmk05318;
