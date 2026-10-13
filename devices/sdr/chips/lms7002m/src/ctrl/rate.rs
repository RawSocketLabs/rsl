//! The sample-rate plan of `lms7002m_samplerate`: the CGEN frequency, converter and
//! processor dividers for the rates asked, and the writes that follow a locked CGEN.

use core::num::NonZeroU32;

use bnb::u2;

use super::calls::{Calls, ChipCall};
use super::{Clocks, Control, ControlError, Direction, order, push_bandwidth};
use crate::chip::{LmlConfig, Tsp};
use crate::regs::Mac;

/// libusdr's `XSDR_SR_*` and `XSDR_LML_*` sample-rate flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "libusdr's flag word, one field per flag"
)]
pub struct RateFlags {
    /// Raise CGEN, and with it the converters' rates, as far as the decimators allow
    /// (`XSDR_SR_MAXCONVRATE`).
    pub max_conversion: bool,

    /// Try CGEN up to 370 MHz before 320 MHz (`XSDR_SR_EXTENDED_CGEN`).
    pub extended_cgen: bool,

    /// RX samples in SISO DDR mode (`XSDR_LML_SISO_DDR_RX`).
    pub rx_siso_ddr: bool,

    /// TX samples in SISO DDR mode (`XSDR_LML_SISO_DDR_TX`).
    pub tx_siso_ddr: bool,

    /// The RX FIFO read on the external `FCLK` (`XSDR_LML_EXT_FIFOCLK_RX`).
    pub rx_external_fifo_clock: bool,

    /// The TX FIFO written on the external clock (`XSDR_LML_EXT_FIFOCLK_TX`).
    pub tx_external_fifo_clock: bool,
}

/// What `lms7002m_samplerate` is asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateRequest {
    /// The RX rate at the chip in samples per second; 0 or 1 for none.
    pub rx: u32,

    /// The TX rate at the chip; 0 or 1 for none.
    pub tx: u32,

    /// A minimum ADC clock, or 0.
    pub adc: u32,

    /// A minimum DAC clock, or 0.
    pub dac: u32,

    /// The flags.
    pub flags: RateFlags,

    /// RX samples on `LimeLight` port 1 (`rx_port_1`).
    pub rx_port1: bool,

    /// The board's decimation after the chip (`rx_dec`).
    pub rx_decimation: u32,

    /// The board's interpolation before the chip (`tx_int`).
    pub tx_interpolation: u32,
}

/// The ADC's divider from CGEN: always 4 (`mpy_adc`).
const ADC_DIVIDER: u32 = 4;
/// The most a processor decimates or interpolates (`LMS7_DECIM_MAX`, `LMS7_INTER_MAX`).
const DIVIDER_MAX: u32 = 32;
/// Below this rate libusdr raises CGEN for anti-aliasing.
const LOW_RATE: u32 = 2_000_000;

/// One CGEN candidate: the frequency and the dividers that go with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Candidate {
    /// CGEN in hertz.
    pub(super) cgen: u32,

    /// The DAC's divider from CGEN: 4 or 1 (`mpy_dac`).
    pub(super) dac_divider: u32,

    /// The RX processor's decimation (`rxdiv`).
    pub(super) rx_div: u32,

    /// The TX processor's interpolation (`txdiv`).
    pub(super) tx_div: u32,
}

/// Whether the processors can divide by `divider` (`_check_lime_decimation`): a power of
/// two up to 32.
const fn valid(divider: u32) -> bool {
    divider.is_power_of_two() && divider <= DIVIDER_MAX
}

impl RateRequest {
    /// How many CGEN ceilings libusdr tries: 370 then 320 MHz when extended.
    pub(super) const fn passes(&self) -> u8 {
        if self.flags.extended_cgen { 2 } else { 1 }
    }

    /// Whether libusdr could run the request without dividing by zero or looping: a board
    /// decimation and interpolation, and a non-zero bandwidth for each direction asked.
    pub(super) const fn sound(&self) -> bool {
        self.rx_decimation != 0
            && self.tx_interpolation != 0
            && (self.rx <= 1 || self.rx / self.rx_decimation != 0)
            && (self.tx <= 1 || self.tx / self.tx_interpolation != 0)
    }

    /// Pass `pass`'s candidate (one iteration of `citer`): [`ControlError::Decimation`]
    /// when a processor cannot divide down to its rate (libusdr's `-EINVAL`), or
    /// [`ControlError::InvalidRate`] when CGEN would be 0 (libusdr then loops forever or
    /// divides by zero). Unsigned arithmetic wraps, as in C.
    pub(super) fn candidate(&self, pass: u8) -> Result<Candidate, ControlError> {
        let Self { rx, tx, flags, .. } = *self;
        let rx_min_div = if flags.rx_siso_ddr || flags.rx_external_fifo_clock {
            1
        } else {
            2
        };
        let tx_min_div = if flags.tx_siso_ddr || flags.tx_external_fifo_clock {
            1
        } else {
            2
        };
        let cgen_max = if flags.extended_cgen && pass == 0 {
            370_000_000
        } else {
            320_000_000
        };
        let mut dac_divider = 4;
        let mut cgen = self
            .dac
            .wrapping_mul(dac_divider)
            .max(self.adc.wrapping_mul(ADC_DIVIDER));
        if cgen < 1 {
            cgen = rx
                .wrapping_mul(rx_min_div)
                .wrapping_mul(ADC_DIVIDER)
                .max(tx.wrapping_mul(tx_min_div).wrapping_mul(dac_divider));
            if cgen == 0 {
                return Err(ControlError::InvalidRate);
            }
            let low = |rate: u32| rate > 1 && rate < LOW_RATE;
            if low(rx) || low(tx) || flags.max_conversion {
                while cgen <= cgen_max {
                    let ndiv = |rate: u32, divider: u32| {
                        if rate > 1 {
                            cgen.wrapping_mul(2) / rate / divider
                        } else {
                            0
                        }
                    };
                    if ndiv(rx, ADC_DIVIDER) > DIVIDER_MAX || ndiv(tx, dac_divider) > DIVIDER_MAX {
                        break;
                    }
                    cgen = cgen.wrapping_mul(2);
                }
            }
        }
        let mut rx_div = 1;
        let mut tx_div = 1;
        if rx > 1 {
            rx_div = cgen / rx / ADC_DIVIDER;
        }
        if tx > 1 {
            tx_div = cgen / tx / dac_divider;
            if tx_div <= 8 {
                tx_div *= 4;
                dac_divider /= 4;
            }
        }
        if (rx > 1 && !valid(rx_div)) || (tx > 1 && !valid(tx_div)) {
            return Err(ControlError::Decimation);
        }
        Ok(Candidate {
            cgen,
            dac_divider,
            rx_div,
            tx_div,
        })
    }

    /// The candidate's clocks, as libusdr stores them before tuning.
    pub(super) const fn clocks(&self, candidate: Candidate, cgen: u32) -> Clocks {
        Clocks {
            cgen,
            rx_converter_div: ADC_DIVIDER,
            tx_converter_div: candidate.dac_divider,
            rx_tsp_div: candidate.rx_div,
            tx_tsp_div: candidate.tx_div,
            rx_decimation: self.rx_decimation,
            tx_interpolation: self.tx_interpolation,
        }
    }

    /// The DAC clock's order relative to the ADC's for `CGEN_0x0089` (`clkdiv`).
    pub(super) const fn dac_order(candidate: Candidate) -> u2 {
        u2::new(match candidate.dac_divider {
            1 => 0,
            2 => 1,
            4 => 2,
            _ => 3,
        })
    }

    /// The `LimeLight` configuration for the locked candidate.
    fn lml(&self, candidate: Candidate) -> LmlConfig {
        let Self { rx, tx, flags, .. } = *self;
        let tsp_div = |rate_set: bool, siso: bool, div: u32| {
            let div = if rate_set && siso {
                div
            } else if rate_set && div > 1 {
                div / 2
            } else {
                1
            };
            u8::try_from(div).expect("invariant: a valid divider fits a byte")
        };
        LmlConfig {
            rx_siso_ddr: flags.rx_siso_ddr,
            tx_siso_ddr: flags.tx_siso_ddr,
            high_drive: rx > 40_000_000 || tx > 40_000_000,
            rx_fclk_read: rx > 0 && flags.rx_external_fifo_clock,
            rx_lfsr: false,
            rx_tx_loopback: false,
            rx_port1: self.rx_port1,
            // libusdr tests the RX rate against 0 and the TX rate against 1 here.
            rx_div: tsp_div(rx > 0, flags.rx_siso_ddr, candidate.rx_div),
            tx_div: tsp_div(tx > 1, flags.tx_siso_ddr, candidate.tx_div),
            tx_tsp_delay: if tx > 40_000_000 && tx < 45_000_000 {
                2
            } else {
                1
            },
            tx_lml_delay: 0,
        }
    }
}

impl Control {
    /// The writes after CGEN locked on `candidate`, and the `LimeLight` configuration they
    /// set (cached once written): the processors' dividers where a path runs, the
    /// `LimeLight` configuration, and each running channel's bandwidth (and, for TX, its
    /// amplifier and pad gains) where none was set.
    pub(super) fn rate_writes(
        &self,
        request: &RateRequest,
        candidate: Candidate,
    ) -> (Calls, LmlConfig) {
        let mut calls = Calls::new();
        if self.rx_running.contains(&true) {
            calls
                .push(ChipCall::MacSet(Mac::Both))
                .push(ChipCall::TspIntDec(Tsp::Rx, order(self.clocks.rx_tsp_div)));
        }
        if self.tx_running.contains(&true) {
            calls
                .push(ChipCall::MacSet(Mac::Both))
                .push(ChipCall::TspIntDec(Tsp::Tx, order(self.clocks.tx_tsp_div)));
        }
        let lml = request.lml(candidate);
        calls.push(ChipCall::LimelightConfigure(lml));
        let bandwidth = |rate: u32, divider: u32| {
            NonZeroU32::new(rate / divider)
                .expect("invariant: sample_rate refuses a zero bandwidth")
        };
        for (index, bank) in [Mac::A, Mac::B].into_iter().enumerate() {
            if request.rx > 1 && self.rx_running[index] && self.rx_bandwidth[index].is_none() {
                calls.push(ChipCall::MacSet(bank));
                push_bandwidth(
                    &mut calls,
                    Direction::Rx,
                    bandwidth(request.rx, request.rx_decimation),
                );
            }
            if request.tx > 1 && self.tx_running[index] && self.tx_bandwidth[index].is_none() {
                calls.push(ChipCall::MacSet(bank));
                push_bandwidth(
                    &mut calls,
                    Direction::Tx,
                    bandwidth(request.tx, request.tx_interpolation),
                );
                // `lms7002m_set_gain(.., RFIC_LMS7_TX_PGA_GAIN, 13)` and
                // `(.., RFIC_LMS7_TX_PAD_GAIN, 0)`, each selecting the channel first.
                calls
                    .push(ChipCall::MacSet(bank))
                    .push(ChipCall::TbbGain(13))
                    .push(ChipCall::MacSet(bank))
                    .push(ChipCall::TrfGain(crate::TxGainStage::Pad, 0));
            }
        }
        (calls, lml)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// libusdr's `/dm/rate/master` request for `rate` on both directions.
    fn master(rate: u32) -> RateRequest {
        RateRequest {
            rx: rate,
            tx: rate,
            adc: 0,
            dac: 0,
            flags: RateFlags {
                max_conversion: true,
                extended_cgen: true,
                ..RateFlags::default()
            },
            rx_port1: true,
            rx_decimation: 1,
            tx_interpolation: 1,
        }
    }

    #[test]
    fn a_low_rate_raises_cgen_until_the_decimators_are_full() {
        let candidate = master(1_000_000).candidate(0).expect("deliverable");
        assert_eq!(candidate.cgen, 128_000_000);
        assert_eq!((candidate.rx_div, candidate.tx_div), (32, 32));
        assert_eq!(candidate.dac_divider, 4, "TX divides 32, not 8 or less");
    }

    #[test]
    fn a_tx_divider_of_eight_or_less_moves_into_the_processor() {
        // CGEN doubles from 245.76 MHz once, past 370 MHz: 491.52 MHz, RX dividing by 4.
        let candidate = master(30_720_000).candidate(0).expect("deliverable");
        assert_eq!(candidate.cgen, 491_520_000);
        assert_eq!((candidate.rx_div, candidate.tx_div), (4, 16));
        assert_eq!(candidate.dac_divider, 1);
    }

    #[test]
    fn an_undeliverable_divider_is_refused() {
        let request = RateRequest {
            rx: 3_000_000,
            tx: 0,
            adc: 0,
            dac: 0,
            flags: RateFlags::default(),
            rx_port1: true,
            rx_decimation: 1,
            tx_interpolation: 1,
        };
        // CGEN = 2 * 3 MHz * 4: RX divides by 2. A 10 MHz ADC floor makes CGEN 40 MHz,
        // and 40 / 3 / 4 truncates to 3, which no processor divides by.
        assert!(request.candidate(0).is_ok());
        let floored = RateRequest {
            adc: 10_000_000,
            ..request
        };
        assert_eq!(floored.candidate(0), Err(ControlError::Decimation));
    }

    #[test]
    fn requests_libusdr_would_hang_or_divide_by_zero_on_are_unsound() {
        let none = RateRequest {
            rx: 0,
            tx: 0,
            ..master(0)
        };
        assert_eq!(
            none.candidate(0),
            Err(ControlError::InvalidRate),
            "CGEN 0: libusdr doubles it forever"
        );
        assert!(
            !RateRequest {
                rx_decimation: 0,
                ..master(1_000_000)
            }
            .sound()
        );
        assert!(
            !RateRequest {
                tx_interpolation: 2_000_000,
                ..master(1_000_000)
            }
            .sound()
        );
        assert!(master(1_000_000).sound());
    }
}
