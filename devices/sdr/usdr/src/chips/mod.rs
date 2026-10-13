//! Drivers for the chips on the uSDR board. Each knows its own registers, not the board.
//!
//! | Module | Chip | What it does on the uSDR |
//! |--------|------|--------------------------|
//! | [`lms6002d`] | Lime LMS6002D | The radio: LNAs, mixers, filters, gain stages, synthesizers, ADC and DAC |
//! | [`si5332`] | Si5332 | Turns the 26 MHz reference into the radio's PLL reference, sample clocks and mixer LO |
//! | [`lp8758`] | LP8758 | Four step-down converters: FPGA and LMS6002D I/O rails |
//! | [`tps6381x`] | TPS63811 | Buck-boost converter for a 3.45 V rail |
//! | [`tmp114`] | TMP114 | Board temperature, for the thermal policy |
//!
//! Every chip but the LMS6002D sits on the FPGA's I2C bus 0 and shares the byte-register
//! access in [`register`]. The LMS6002D is on SPI and has its own word format.
//!
//! The front-end boards (`crate::frontend`) carry their own chips on I2C bus 1, the M.2
//! connector's bus. Those the shared probe and the `PCIe` front end and breakout need (the
//! TCA6416 and TCA9555 expanders, the `DACx0501`) are `rsl-sdr-frontend`'s; the ones the
//! uSDR's bring-ups need are here:
//!
//! | Module | Chip | Front end |
//! |--------|------|-----------|
//! | [`lmk5c33216`] | LMK5C33216 clock synchronizer | `SuperSync` |
//! | [`lmk05318`] | LMK05318 clock synchronizer | `SimpleSync`, XMASS |
//! | [`at24`] | AT24 EEPROM (security registers) | XMASS revision 1a: serial number |
//!
//! A chip driver is a small `Copy` value naming where the chip answers. `at(addr)` builds
//! one anywhere; `usdr()` builds the one the uSDR has, and its doc says where the address
//! comes from. Drivers borrow the bus per call and keep no state, except the LMS6002D
//! driver: like libusdr, it caches the registers it updates field by field instead of
//! reading them back.

pub(crate) mod at24;
pub(crate) mod lmk05318;
pub(crate) mod lmk5c33216;
pub(crate) mod lms6002d;
pub(crate) mod lp8758;
pub(crate) mod register;
pub(crate) mod si5332;
pub(crate) mod tmp114;
pub(crate) mod tps6381x;
