//! The front-end boards' own chips, as their probes and bring-ups use them. Only the boards
//! in this crate use them; a chip a driver also needs moves to `devices/sdr/chips/<chip>`
//! (`devices/AGENTS.md`).
//!
//! | Module | Chip | Board |
//! |--------|------|-------|
//! | [`tca6416`] | TCA6416 GPIO expander | `PCIe` front end: RF switches, amplifiers, LEDs |
//! | [`dac80501`] | `DACx0501` DAC | `PCIe` and M.2-to-`PCIe` front ends: oscillator trim |
//! | [`tca9555`] | TCA9555-compatible GPIO expander | XMASS: board control |
//! | [`clock_id`] | LMK05318, LMK5C33216 identification | `SimpleSync`, `SuperSync` |

pub(crate) mod clock_id;
pub(crate) mod dac80501;
pub(crate) mod tca6416;
pub(crate) mod tca9555;
