//! LMS6002D RF transceiver.
//!
//! # What it does
//!
//! The LMS6002D is a complete radio on one chip: everything between the antenna balun and
//! the digital samples, from 0.3 to 3.8 GHz. It is a zero-IF (direct-conversion) design:
//! the mixers move the tuned frequency straight to 0 Hz, so the samples are complex
//! baseband I/Q centred on the tuned frequency.
//!
//! ```text
//! RX: LNA1/2/3 -> RX mixer -> RXVGA1 -> RX LPF -> RXVGA2 -> ADC -> 12-bit I/Q to FPGA
//!                    ^
//!                 RX PLL
//! TX: 12-bit I/Q from FPGA -> DAC -> TX LPF -> TXVGA1 -> TX mixer -> TXVGA2 -> PA1/PA2
//!                                                            ^
//!                                                         TX PLL
//! ```
//!
//! - **LNAs** amplify the weak antenna signal before anything adds noise. Each of the
//!   three is matched to a different input and band; see [`Lna`].
//! - **Mixers and PLLs.** Each direction has its own synthesizer, a fractional-N PLL
//!   locked to the reference clock from the Si5332, so RX and TX tune independently.
//! - **VGAs** (variable-gain amplifiers) set the level reaching the ADC or leaving the
//!   PA. libusdr's ranges: RXVGA2 0 to 30 dB; TXVGA1 −35 to −4 dB; TXVGA2 0 to 25 dB.
//! - **LPFs** are the channel filters. Their bandwidth limits how much spectrum the ADC
//!   sees, and has to suit the sample rate.
//! - **ADC and DAC** are clocked by the Si5332's sample clocks and exchange samples
//!   with the FPGA over a 12-bit parallel bus, I and Q in turn.
//! - **PAs.** Two output drivers, each matched to its own band; see [`PowerAmp`].
//!
//! # How the driver uses it
//!
//! The chip sits on the FPGA's SPI bus 0 ([`Lms6002d::USDR_TARGET`]); every access is one 16-bit
//! word carrying a register address and a byte ([`spi`]). Power-up releases the chip from reset,
//! then [`Lms6002d::create`] reads its ID and writes the start-up configuration, part of it from
//! Lime's LMS6002D FAQ. The board then turns both chains off and selects LNA1 and PA1, so a powered
//! board is idle until tuned. Tuning, filters, gains and calibration follow with the rest of the
//! port.
//!
//! # Register map
//!
//! The chip's register map is split into blocks by address, as in the datasheet: one module
//! per block, each with its own `Reg` enum. Register meanings come from libusdr's
//! `hw/lms6002d/lms6002d.yaml` (vendored under `oracle/libusdr`), and the values written
//! come from `hw/lms6002d/lms6002d.c`.

mod afe;
mod dc_cal;
mod lms6002d;
mod lpf;
mod pll;
mod rx_fe;
mod rx_vga2;
mod spi;
mod top;
mod tx_rf;

pub(crate) use lms6002d::{CapacitorWindow, Lms6002d};
pub(crate) use rx_fe::Lna;
pub(crate) use tx_rf::PowerAmp;
