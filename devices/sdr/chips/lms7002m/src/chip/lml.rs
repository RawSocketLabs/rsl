//! The `LimeLight` digital interface: its configuration, sample maps and resets
//! (`lms7002m_limelight_*`).

use bnb::u2;

use super::Lms7002m;
use crate::batch::Batch;
use crate::reg::Reg;
use crate::register::Register;
use crate::regs::{
    CdsDelay, CdsInvert, LmlClockDivider, LmlClockMux, LmlClockSource, LmlDirection, LmlModes,
    LmlPads, LmlPortMap, LmlRxSource, LmlSample, MclkSource,
};

/// How the interface runs: libusdr's `lms7002m_limelight_conf_t`. The default is its
/// zeroed state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "libusdr's lms7002m_limelight_conf_t flags, kept one to one"
)]
pub struct LmlConfig {
    /// Receive samples in SISO DDR mode (`rxsisoddr`).
    pub rx_siso_ddr: bool,

    /// Transmit samples in SISO DDR mode (`txsisoddr`).
    pub tx_siso_ddr: bool,

    /// Data pins at 8 mA (`ds_high`).
    pub high_drive: bool,

    /// The receive FIFO read on `FCLK` rather than `MCLK` (`rx_ext_rd_fclk`).
    pub rx_fclk_read: bool,

    /// The receive FIFO fed the LFSR pattern (`rx_lfsr`).
    pub rx_lfsr: bool,

    /// The receive FIFO fed the transmit FIFO, a digital loopback
    /// (`rx_tx_dig_loopback`); the LFSR wins when both are set.
    pub rx_tx_loopback: bool,

    /// Receive samples on port 1, transmit on port 2 (`rx_port` = 1); else the reverse.
    pub rx_port1: bool,

    /// The receive processor clock's divider (`rxdiv`).
    pub rx_div: u8,

    /// The transmit processor clock's divider (`txdiv`).
    pub tx_div: u8,

    /// The transmit processors' clock delay code (`txtspdelay`).
    pub tx_tsp_delay: u8,

    /// The transmit FIFOs' clock delay code (`txlmldelay`).
    pub tx_lml_delay: u8,
}

impl LmlConfig {
    /// The receive FIFO's source.
    const fn rx_source(&self) -> LmlRxSource {
        if self.rx_lfsr {
            LmlRxSource::Lfsr
        } else if self.rx_tx_loopback {
            LmlRxSource::TxFifo
        } else {
            LmlRxSource::Rxtsp
        }
    }

    /// The FIFOs' multiplexers (`LML_0x002A`): the TX FIFO takes the transmit port and is
    /// read on the TX processor clock and written on the transmit port's `FCLK`; the RX
    /// FIFO is written on the RX processor clock and read on the receive port's `MCLK`, or
    /// on the *other* port's `FCLK` with [`Self::rx_fclk_read`], as libusdr selects them.
    fn clock_mux(&self) -> LmlClockMux {
        let port = |port1: bool| u2::new(u8::from(!port1));
        let rx_read = match (self.rx_fclk_read, self.rx_port1) {
            (true, true) => 2,
            (true, false) => 3,
            (false, true) => 0,
            (false, false) => 1,
        };
        LmlClockMux::new()
            .with_rx_source(self.rx_source())
            .with_tx_source(port(!self.rx_port1))
            .with_tx_read_clock(u2::new(2))
            .with_tx_write_clock(port(!self.rx_port1))
            .with_rx_read_clock(u2::new(rx_read))
            .with_rx_write_clock(u2::new(2))
    }
}

/// The order of a port's four sample slots: libusdr's `lms7002m_lml_map_t`, slot 0 first.
pub type LmlMap = [LmlSample; 4];

impl Lms7002m {
    /// Configures the interface (`lms7002m_limelight_configure`): pads, directions and
    /// port roles, multiplexers, clock sources and dividers, clock inversions and delays,
    /// then a FIFO reset pulse with the bank selection kept.
    pub fn limelight_configure(&self, config: LmlConfig) -> Batch {
        let LmlConfig {
            rx_siso_ddr,
            tx_siso_ddr,
            high_drive,
            rx_port1,
            rx_div,
            tx_div,
            ..
        } = config;
        let tx_mclk = if tx_div <= 1 {
            MclkSource::TxTsp
        } else {
            MclkSource::TxTspDivided
        };
        let rx_mclk = if rx_div <= 1 {
            MclkSource::RxTsp
        } else {
            MclkSource::RxTspDivided
        };
        let (port1_mclk, port2_mclk) = if rx_port1 {
            (rx_mclk, tx_mclk)
        } else {
            (tx_mclk, rx_mclk)
        };
        // libusdr writes `LMLn_RXNTXIQ` = 1 (TXIQ) on port 2 when RX is on port 2, and on
        // port 1 when RX is on port 1: the field names the port's role from the chip's
        // side, so it is reproduced as written.
        let direction = |set: bool| {
            if set {
                LmlDirection::Tx
            } else {
                LmlDirection::Rx
            }
        };
        // `div / 2 - 1` in unsigned arithmetic, masked to the 8-bit field: 1 gives 0xFF.
        let half_less_one = |div: u8| (div / 2).wrapping_sub(1);
        let delay = |code: u8| u2::from_raw(code);

        let mut batch = Batch::new(self.target);
        batch.push(
            LmlPads::new()
                .with_port2_siso_ddr(if rx_port1 { tx_siso_ddr } else { rx_siso_ddr })
                .with_port1_siso_ddr(if rx_port1 { rx_siso_ddr } else { tx_siso_ddr })
                .with_port2_high_drive(high_drive)
                .with_port1_high_drive(high_drive)
                .entry(),
        );
        batch.push(
            LmlModes::new()
                .with_port2_data_input(true)
                .with_port1_data_input(true)
                .with_port2_enable_input(true)
                .with_port1_enable_input(true)
                .with_enabled(true)
                .with_port2_direction(direction(!rx_port1))
                .with_port1_direction(direction(rx_port1))
                .entry(),
        );
        batch.push(config.clock_mux().entry());
        batch.push(
            LmlClockSource::new()
                .with_fclk1_inverted(true)
                .with_mclk2_source(port2_mclk)
                .with_mclk1_source(port1_mclk)
                .with_tx_divider_enabled(tx_div > 1)
                .with_rx_divider_enabled(rx_div > 1)
                .entry(),
        );
        batch.push(
            LmlClockDivider::new()
                .with_tx(half_less_one(tx_div))
                .with_rx(half_less_one(rx_div))
                .entry(),
        );
        batch.push(
            CdsInvert::new()
                .with_tx_b_tsp_inverted(true)
                .with_tx_a_tsp_inverted(true)
                .with_rx_b_tsp_inverted(true)
                .with_rx_a_tsp_inverted(true)
                .with_tx_b_lml_inverted(true)
                .with_tx_a_lml_inverted(!(tx_siso_ddr && tx_div == 1))
                .with_rx_b_lml_inverted(true)
                .with_rx_a_lml_inverted(!(rx_siso_ddr && rx_div == 1))
                .with_mclk2_inverted(true)
                .with_mclk1_inverted(true)
                .entry(),
        );
        batch.push(
            CdsDelay::new()
                .with_tx_b_tsp(delay(config.tx_tsp_delay))
                .with_tx_a_tsp(delay(config.tx_tsp_delay))
                .with_tx_b_lml(delay(config.tx_lml_delay))
                .with_tx_a_lml(delay(config.tx_lml_delay))
                .entry(),
        );
        batch.push(
            self.mac
                .with_srst_rx_fifo(true)
                .with_srst_tx_fifo(true)
                .entry(),
        );
        batch.push(self.mac.entry());
        batch
    }

    /// Switches the receive FIFO's source alone (`lms7002m_limelight_switch_rx_mode`).
    pub fn limelight_switch_rx_mode(&self, config: LmlConfig) -> Batch {
        let mut batch = Batch::new(self.target);
        batch.push(config.clock_mux().entry());
        batch
    }

    /// Pulses every `LimeLight` reset, logic and FIFO, keeping the bank selection
    /// (`lms7002m_limelight_reset`).
    pub fn limelight_reset(&self) -> Batch {
        let mut batch = Batch::new(self.target);
        batch.push(
            self.mac
                .with_lrst_tx_b(true)
                .with_mrst_tx_b(true)
                .with_lrst_tx_a(true)
                .with_mrst_tx_a(true)
                .with_lrst_rx_b(true)
                .with_mrst_rx_b(true)
                .with_lrst_rx_a(true)
                .with_mrst_rx_a(true)
                .with_srst_rx_fifo(true)
                .with_srst_tx_fifo(true)
                .entry(),
        );
        batch.push(self.mac.entry());
        batch
    }

    /// Pulses the receive and/or transmit FIFO resets (`lms7002m_limelight_fifo_reset`).
    pub fn limelight_fifo_reset(&self, rx: bool, tx: bool) -> Batch {
        let mut pulse = self.mac;
        if rx {
            pulse.set_srst_rx_fifo(true);
        }
        if tx {
            pulse.set_srst_tx_fifo(true);
        }
        let mut batch = Batch::new(self.target);
        batch.push(pulse.entry());
        batch.push(self.mac.entry());
        batch
    }

    /// Sets both ports' sample order (`lms7002m_limelight_map`). A SISO port carries one
    /// channel in two slots: libusdr reads slots 0 and 1 only and places both channels'
    /// I (or Q) where it finds either.
    pub fn limelight_map(&self, siso: [bool; 2], maps: [LmlMap; 2]) -> Batch {
        let mut batch = Batch::new(self.target);
        for ((reg, map), siso) in [Reg::Lml1Map, Reg::Lml2Map].into_iter().zip(maps).zip(siso) {
            batch.push((reg, port_map(map, siso).to_raw()));
        }
        batch
    }
}

/// A port's map word: the slots, and where each stream sits (`_lms7002m_fill_pos`).
fn port_map(map: LmlMap, siso: bool) -> LmlPortMap {
    let mut positions = [0u8; 4];
    if siso {
        for (slot, sample) in (0u8..).zip(&map[..2]) {
            let pair = match sample {
                LmlSample::AI | LmlSample::BI => [LmlSample::AI, LmlSample::BI],
                LmlSample::AQ | LmlSample::BQ => [LmlSample::AQ, LmlSample::BQ],
            };
            for stream in pair {
                positions[stream as usize] = slot;
            }
        }
    } else {
        for (slot, &sample) in (0u8..).zip(&map) {
            positions[sample as usize] = slot;
        }
    }
    let [ai, aq, bi, bq] = positions.map(u2::new);
    let [slot0, slot1, slot2, slot3] = map;
    LmlPortMap::new()
        .with_slot3(slot3)
        .with_slot2(slot2)
        .with_slot1(slot1)
        .with_slot0(slot0)
        .with_bq_position(bq)
        .with_bi_position(bi)
        .with_aq_position(aq)
        .with_ai_position(ai)
}

#[cfg(test)]
mod tests {
    use rsl_lowlevel::SpiAddr;

    use super::*;
    use crate::regs::Mac;

    #[test]
    fn a_divider_of_one_writes_the_wrapped_field_and_the_undivided_clock() {
        let chip = Lms7002m::created(SpiAddr(0));
        let config = LmlConfig {
            rx_div: 1,
            tx_div: 4,
            rx_port1: true,
            ..LmlConfig::default()
        };
        let batch = chip.limelight_configure(config);
        let word = |reg| {
            batch
                .writes()
                .iter()
                .find(|(r, _)| *r == reg)
                .map(|&(_, v)| v)
                .expect("written")
        };
        let divider = LmlClockDivider::from_raw(word(Reg::LmlClockDivider));
        assert_eq!((divider.rx(), divider.tx()), (0xff, 1), "1/2 - 1 wraps");
        let source = LmlClockSource::from_raw(word(Reg::LmlClockSource));
        assert_eq!(source.mclk1_source(), MclkSource::RxTsp, "RX on port 1");
        assert_eq!(source.mclk2_source(), MclkSource::TxTspDivided);
        let [.., pulse, restore] = batch.writes() else {
            panic!("the FIFO pulse ends the batch")
        };
        assert_eq!(*restore, (Reg::LmlControl, chip.mac.to_raw()));
        assert_eq!(pulse.1 & 0xc0, 0xc0, "both FIFO resets set");
        assert_eq!(chip.mac(), Mac::Both);
    }

    #[test]
    fn a_siso_port_places_both_channels_where_it_finds_either() {
        let map = port_map(
            [LmlSample::BQ, LmlSample::AI, LmlSample::AI, LmlSample::AI],
            true,
        );
        assert_eq!(map.ai_position().value(), 1);
        assert_eq!(map.bi_position().value(), 1);
        assert_eq!(map.aq_position().value(), 0);
        assert_eq!(map.bq_position().value(), 0);
    }
}
