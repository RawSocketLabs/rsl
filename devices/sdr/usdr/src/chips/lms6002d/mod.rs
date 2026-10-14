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
//! per block, each with its own `Reg` enum, in the portable tree (`portable/lms6002d`),
//! with the cached state, its operations and the tuning and calibration machines, which
//! the blocking driver here and the native USB handle both run. Register meanings come
//! from libusdr's `hw/lms6002d/lms6002d.yaml` (vendored under `devices/vendor/libusdr`), and the
//! values written come from `hw/lms6002d/lms6002d.c`.

pub(crate) use crate::portable::lms6002d::Lna;

#[cfg(test)]
mod tests {
    use crate::portable::lms6002d::rx_fe::RXVGA1_CODES;

    #[test]
    fn the_tabulated_rxvga1_codes_are_libusdrs_formula() {
        // `_lms6002d_rxvga1_db_to_int`, in double precision as libusdr computes it.
        let formula = |db: f64| {
            let code = 127.5 - 127.0 / 10_f64.powf((db - 5.0) / 20.0);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "libusdr truncates the double to int8_t"
            )]
            let code = code as u8;
            code & 0x7f
        };
        let gains = (5..=30).map(f64::from).chain([30.17]);
        for (db, &code) in gains.zip(RXVGA1_CODES.iter()) {
            assert_eq!(code, formula(db), "the code for {db} dB");
        }
    }
}
