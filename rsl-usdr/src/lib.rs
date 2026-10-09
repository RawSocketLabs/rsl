//! Pure-Rust driver for the Wavelet Lab uSDR (`m2_lm6_1`).
//!
//! [`Device`] is the radio. It talks to the board through a [`lowlevel::Bus`]: the same
//! register/SPI/I2C seam libusdr's `ls_op` provides, so the board logic here is checked
//! against libusdr's on a simulated board (`rsl-usdr-oracle`).
//!
//! Status: board power-up and power-down, temperature, the thermal policy, the RX sample
//! rate, bandwidth and frequency, and the RX stream over any [`lowlevel::Bus`] that streams.
//! The USB and `PCIe` transports follow.
//!
//! # The uSDR board
//!
//! The uSDR is an M.2 software-defined radio from Wavelet Lab. libusdr calls this board
//! design `m2_lm6_1`, and this crate supports its hardware revisions 1 to 3. Besides an
//! FPGA, the board carries five chips the driver programs, plus parts it only switches
//! (RF switches, a mixer, an oscillator). The host never talks to the chips directly.
//! Every access is a request to the FPGA: read or write one of its registers, run an SPI
//! transaction, or run an I2C transaction. These three operations are the
//! [`lowlevel::Bus`] trait.
//!
//! | Chip | Job on the board | Reached over |
//! |------|------------------|--------------|
//! | FPGA (gateware) | Bridges the host link to every other chip; moves and resamples samples; drives the control lines | USB or PCI Express |
//! | Lime LMS6002D | The radio: receives and transmits from 0.3 to 3.8 GHz and converts to and from baseband samples | SPI bus 0 |
//! | Skyworks (formerly Silicon Labs) Si5332 | Clock generator: makes the radio's PLL reference and sample clocks, and the board mixer's LO | I2C bus 0, address 0x6A |
//! | TI LP8758 | Power-management IC: four step-down converters for the FPGA and radio supplies | I2C bus 0, address 0x60 |
//! | TI TPS63811 | Buck-boost converter: a 3.45 V rail that libusdr turns on before the RF section | I2C bus 0, address 0x75 |
//! | TI TMP114 | Temperature sensor: the board temperature the thermal policy acts on | I2C bus 0, address 0x4E |
//!
//! ## Receive path
//!
//! An antenna port feeds one of the radio's three low-noise amplifiers (LNAs). An RF
//! switch, driven by an FPGA output, picks which balun connects to the port. libusdr's
//! automatic band plan:
//!
//! | Path (libusdr name) | Covers | LNA | RX switch |
//! |---------------------|--------|-----|-----------|
//! | `LNAL`, low band | below 230 MHz, through the board's upconverting mixer | LNA3 | 1 |
//! | `LNAW`, wideband | up to 2.8 GHz | LNA1 | 0 |
//! | `LNAH`, high band | up to 3.8 GHz | LNA2 | 1 |
//!
//! The LMS6002D tunes no lower than 0.3 GHz. For the low band, the board's mixer adds the
//! Si5332's output 3 (at most a few hundred MHz) to the incoming signal, and the
//! LMS6002D is tuned to the sum. libusdr's wiring table also names the M.2 connector's RF
//! port as an LNA3 input.
//!
//! Inside the LMS6002D, the signal goes through the LNA, then the mixer, which the RX
//! synthesizer drives to bring the wanted frequency down to 0 Hz. Next come RXVGA1, the
//! low-pass channel filter, RXVGA2 and the ADC. The ADC's 12-bit I/Q samples cross a
//! parallel bus to the FPGA, which decimates them to the requested rate and sends them to
//! the host.
//!
//! The transmit path mirrors this. The FPGA interpolates the host's samples up to the
//! converter rate and sends them to the DAC. Then come the TX filter, TXVGA1, the TX
//! mixer, TXVGA2 and one of two power amplifiers. A second switch picks that amplifier's
//! balun.
//!
//! ## Clocks
//!
//! The Si5332 takes a 26 MHz reference (libusdr's `USDR_INT_REFCLK`). On revision 3 it
//! comes from an on-board oscillator that the FPGA switches on. Revisions 1 and 2 take it
//! on the Si5332's clock input 2. The Si5332 has six outputs:
//!
//! | Output | Revision 3 | Revision 2 | Revision 1 |
//! |--------|------------|------------|------------|
//! | 0 | LMS6002D PLL reference | RX sample clock | RX sample clock |
//! | 1 | RX sample clock | LMS6002D PLL reference | LMS6002D PLL reference |
//! | 2 | TX sample clock | TX sample clock | TX sample clock |
//! | 3 | board mixer LO | board mixer LO | board mixer LO |
//! | 4 | FPGA transceiver reference | FPGA transceiver reference | not connected |
//! | 5 | FPGA reference | USB clock | not connected |
//!
//! The PLL reference output is the one driven as LVPECL; the sample clocks are CMOS. At
//! power-up, outputs 0 to 2 carry the 26 MHz reference undivided, and outputs 3 to 5 are
//! off.
//!
//! Setting a sample rate ([`Device::set_rx_sample_rate`]) follows libusdr. On a board with
//! both chains, it runs the converters faster than the host rate, by a power of two from 1 to
//! 32, aiming for 30.72–60 MS/s, and the FPGA decimates (RX) or interpolates (TX) by the
//! same factor. Each sample clock runs at twice the converter rate, because I and Q take
//! turns on the LMS6002D's 12-bit sample bus. The clocks usually come from the Si5332's
//! internal 2.375–2.625 GHz VCO, divided down; a clock that is an exact fraction of the
//! reference stays on the reference. The same step starts output 3. Moving the RX path
//! into or out of the mixer path later gates output 3 (and output 2 with the TX state).
//!
//! ## Power
//!
//! Some rails are up before the driver runs: the FPGA must be, to answer at all. On
//! revision 3, libusdr reads the TMP114 before touching the power-management IC, so the
//! sensor is powered from reset there; this crate assumes the same on revisions 1 and 2.
//! Opening a [`Device`] does the rest, in this order:
//!
//! 1. It reads the board revision and holds the LMS6002D in reset.
//! 2. It checks the TMP114 and applies the [`ThermalPolicy`]. Nothing has been switched
//!    on yet, so a board that is too hot is refused before it draws more power.
//! 3. It checks the LP8758, sets two of its rails to 1.8 V, and turns on all four
//!    converters.
//! 4. It turns on the TPS63811 at 3.45 V.
//! 5. It programs the Si5332. On revision 3, it then starts the oscillator.
//! 6. It turns on the RF booster and the status LED, releases the LMS6002D from reset, and
//!    sets it idle: both chains off, the wideband RX path, PA1.
//!
//! Closing the device, or dropping it, undoes step 6: the LMS6002D goes back into reset,
//! and the LED and booster turn off. The converters and clocks stay on, as in libusdr.
//!
//! ## Temperature
//!
//! The TMP114 sits on the board and reports its temperature in 1/128 °C steps.
//! [`ThermalPolicy`] sets the temperatures at which this crate refuses to start or stops;
//! [`HARD_STOP_CELSIUS`] is the limit no policy lifts.
#![forbid(unsafe_code)]

mod board;
mod chips;
mod device;
mod error;
mod fpga;
pub mod lowlevel;
mod thermal;

pub use board::RxPacket;
pub use device::{Device, DeviceBuilder};
pub use error::{Access, Error};
pub use thermal::{HARD_STOP_CELSIUS, ThermalLimits, ThermalPolicy};
