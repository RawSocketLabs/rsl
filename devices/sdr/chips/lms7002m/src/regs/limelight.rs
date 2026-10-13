//! The `LimeLight` digital interface's configuration (`LML` page, 0x0022–0x002C) and its
//! clock inversions and delays (`CDS` page).
//!
//! The pinned source names these fields and options without describing them; names follow
//! the mnemonics, and docs say what libusdr's `lms7002m_limelight_configure` writes.

use bnb::{BitEnum, bitfield, u1, u2};

use crate::reg::Reg;
use crate::register::Register;

/// Port modes, drive strengths and pull enables. `LML_0x0022`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LmlPads {
    /// `TRXIQPULSE2`, bit 15.
    #[bits(15..=15)]
    port2_trxiq_pulse: bool,

    /// Port 2 in SISO DDR mode. `SISODDR2`, bit 14.
    #[bits(14..=14)]
    port2_siso_ddr: bool,

    /// `TRXIQPULSE1`, bit 13.
    #[bits(13..=13)]
    port1_trxiq_pulse: bool,

    /// Port 1 in SISO DDR mode. `SISODDR1`, bit 12.
    #[bits(12..=12)]
    port1_siso_ddr: bool,

    /// Port 2's data pins at 8 mA rather than 4. `DIQ2_DS`, bit 11.
    #[bits(11..=11)]
    port2_high_drive: bool,

    /// `DIQ2_PE`, bit 10.
    #[bits(10..=10)]
    port2_data_pull: bool,

    /// `IQ_SEL_EN_2_PE`, bit 9.
    #[bits(9..=9)]
    port2_iq_select_pull: bool,

    /// `TXNRX2_PE`, bit 8.
    #[bits(8..=8)]
    port2_txnrx_pull: bool,

    /// `FCLK2_PE`, bit 7.
    #[bits(7..=7)]
    port2_fclk_pull: bool,

    /// `MCLK2_PE`, bit 6.
    #[bits(6..=6)]
    port2_mclk_pull: bool,

    /// Port 1's data pins at 8 mA rather than 4. `DIQ1_DS`, bit 5.
    #[bits(5..=5)]
    port1_high_drive: bool,

    /// `DIQ1_PE`, bit 4.
    #[bits(4..=4)]
    port1_data_pull: bool,

    /// `IQ_SEL_EN_1_PE`, bit 3.
    #[bits(3..=3)]
    port1_iq_select_pull: bool,

    /// `TXNRX1_PE`, bit 2.
    #[bits(2..=2)]
    port1_txnrx_pull: bool,

    /// `FCLK1_PE`, bit 1.
    #[bits(1..=1)]
    port1_fclk_pull: bool,

    /// `MCLK1_PE`, bit 0.
    #[bits(0..=0)]
    port1_mclk_pull: bool,
}

impl Register for LmlPads {
    const ADDR: Reg = Reg::LmlPads;
}

/// Which samples a `LimeLight` port carries. `LMLn_RXNTXIQ`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u1)]
pub enum LmlDirection {
    /// Receive samples (`RXIQ`, 0).
    Rx,

    /// Transmit samples (`TXIQ`, 1).
    Tx,
}

/// Pin directions, the interface enable and each port's role. `LML_0x0023`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LmlModes {
    /// `DIQDIRCTR2`, bit 15.
    #[bits(15..=15)]
    port2_data_direction_override: bool,

    /// Port 2's data pins as inputs. `DIQDIR2`, bit 14.
    #[bits(14..=14)]
    port2_data_input: bool,

    /// `DIQDIRCTR1`, bit 13.
    #[bits(13..=13)]
    port1_data_direction_override: bool,

    /// Port 1's data pins as inputs. `DIQDIR1`, bit 12.
    #[bits(12..=12)]
    port1_data_input: bool,

    /// `ENABLEDIRCTR2`, bit 11.
    #[bits(11..=11)]
    port2_enable_direction_override: bool,

    /// Port 2's enable pin as an input. `ENABLEDIR2`, bit 10.
    #[bits(10..=10)]
    port2_enable_input: bool,

    /// `ENABLEDIRCTR1`, bit 9.
    #[bits(9..=9)]
    port1_enable_direction_override: bool,

    /// Port 1's enable pin as an input. `ENABLEDIR1`, bit 8.
    #[bits(8..=8)]
    port1_enable_input: bool,

    /// The interface enabled ("Enable LML"). `MOD_EN`, bit 6.
    #[bits(6..=6)]
    enabled: bool,

    /// `LML2_FIDM`, bit 5.
    #[bits(5..=5)]
    port2_frame_polarity: bool,

    /// `LML2_RXNTXIQ`, bit 4.
    #[bits(4..=4)]
    port2_direction: LmlDirection,

    /// Port 2 in JESD207 rather than TRXIQ mode. `LML2_MODE`, bit 3.
    #[bits(3..=3)]
    port2_jesd207: bool,

    /// `LML1_FIDM`, bit 2.
    #[bits(2..=2)]
    port1_frame_polarity: bool,

    /// `LML1_RXNTXIQ`, bit 1.
    #[bits(1..=1)]
    port1_direction: LmlDirection,

    /// Port 1 in JESD207 rather than TRXIQ mode. `LML1_MODE`, bit 0.
    #[bits(0..=0)]
    port1_jesd207: bool,
}

impl Register for LmlModes {
    const ADDR: Reg = Reg::LmlModes;
}

/// One sample stream on a `LimeLight` port: libusdr's `LML_AI`.. `LML_BQ`, the YAML's
/// `LMLn_SnS` options.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum LmlSample {
    /// Channel A, I (`AI`, 0).
    AI,

    /// Channel A, Q (`AQ`, 1).
    AQ,

    /// Channel B, I (`BI`, 2).
    BI,

    /// Channel B, Q (`BQ`, 3).
    BQ,
}

/// A port's sample order: which stream each of its four slots carries, and where each
/// stream sits. `LML_0x0024` (port 1) and `LML_0x0027` (port 2), which share this layout;
/// written at [`Reg::Lml1Map`] or [`Reg::Lml2Map`].
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LmlPortMap {
    /// The stream in slot 3. `LMLn_S3S`, bits 15:14.
    #[bits(14..=15)]
    slot3: LmlSample,

    /// The stream in slot 2. `LMLn_S2S`, bits 13:12.
    #[bits(12..=13)]
    slot2: LmlSample,

    /// The stream in slot 1. `LMLn_S1S`, bits 11:10.
    #[bits(10..=11)]
    slot1: LmlSample,

    /// The stream in slot 0. `LMLn_S0S`, bits 9:8.
    #[bits(8..=9)]
    slot0: LmlSample,

    /// The slot carrying B, Q. `LMLn_BQP`, bits 7:6.
    #[bits(6..=7)]
    bq_position: u2,

    /// The slot carrying B, I. `LMLn_BIP`, bits 5:4.
    #[bits(4..=5)]
    bi_position: u2,

    /// The slot carrying A, Q. `LMLn_AQP`, bits 3:2.
    #[bits(2..=3)]
    aq_position: u2,

    /// The slot carrying A, I. `LMLn_AIP`, bits 1:0.
    #[bits(0..=1)]
    ai_position: u2,
}

/// What the receive FIFO takes. `RX_MUX`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum LmlRxSource {
    /// The receive processor (`RXTSP`, 0).
    Rxtsp,

    /// The transmit FIFO, a digital loopback (`TXFIFO`, 1).
    TxFifo,

    /// The LFSR test pattern (`LFSR`, 2).
    Lfsr,

    /// Code 3 (`LFSR_`), also the LFSR.
    Lfsr3,
}

/// The FIFOs' clock and data multiplexers. `LML_0x002A`.
///
/// Clock codes, from the YAML: `TXRDCLK_MUX` 0 `FCLK1`, 1 `FCLK2`, 2 `TXTSPCLK`;
/// `TXWRCLK_MUX` 0 `FCLK1`, 1 `FCLK2`, 2 `RXTSPCLK`; `RXRDCLK_MUX` 0 `MCLK1`, 1 `MCLK2`,
/// 2 `FCLK1`, 3 `FCLK2`; `RXWRCLK_MUX` 0 `FCLK1`, 1 `FCLK2`, 2 `RXTSPCLK`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LmlClockMux {
    /// `RX_MUX`, bits 11:10.
    #[bits(10..=11)]
    rx_source: LmlRxSource,

    /// The port the transmit FIFO takes: 0 `PORT1`, 1 `PORT2`, 2 `RXTSP`. `TX_MUX`, bits
    /// 9:8.
    #[bits(8..=9)]
    tx_source: u2,

    /// `TXRDCLK_MUX`, bits 7:6.
    #[bits(6..=7)]
    tx_read_clock: u2,

    /// `TXWRCLK_MUX`, bits 5:4.
    #[bits(4..=5)]
    tx_write_clock: u2,

    /// `RXRDCLK_MUX`, bits 3:2.
    #[bits(2..=3)]
    rx_read_clock: u2,

    /// `RXWRCLK_MUX`, bits 1:0.
    #[bits(0..=1)]
    rx_write_clock: u2,
}

impl Register for LmlClockMux {
    const ADDR: Reg = Reg::LmlClockMux;
}

/// Where a port's `MCLK` comes from. `MCLKnSRC`.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub enum MclkSource {
    /// The TX processor clock, divided (`TXTSPCLKA_DIV`, 0).
    TxTspDivided,

    /// The RX processor clock, divided (`RXTSPCLKA_DIV`, 1).
    RxTspDivided,

    /// The TX processor clock (`TXTSPCLKA`, 2).
    TxTsp,

    /// The RX processor clock (`RXTSPCLKA`, 3).
    RxTsp,
}

/// Clock inversions, `MCLK` sources and divider enables. `LML_0x002B`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LmlClockSource {
    /// `FCLK2_INV`, bit 15.
    #[bits(15..=15)]
    fclk2_inverted: bool,

    /// `FCLK1_INV`, bit 14; libusdr sets it.
    #[bits(14..=14)]
    fclk1_inverted: bool,

    /// `MCLK2_INV`, bit 9.
    #[bits(9..=9)]
    mclk2_inverted: bool,

    /// `MCLK1_INV`, bit 8.
    #[bits(8..=8)]
    mclk1_inverted: bool,

    /// `MCLK2SRC`, bits 5:4.
    #[bits(4..=5)]
    mclk2_source: MclkSource,

    /// `MCLK1SRC`, bits 3:2.
    #[bits(2..=3)]
    mclk1_source: MclkSource,

    /// The TX clock divider enabled. `TXDIVEN`, bit 1.
    #[bits(1..=1)]
    tx_divider_enabled: bool,

    /// The RX clock divider enabled. `RXDIVEN`, bit 0.
    #[bits(0..=0)]
    rx_divider_enabled: bool,
}

impl Register for LmlClockSource {
    const ADDR: Reg = Reg::LmlClockSource;
}

/// The processor clock dividers: half the division minus one. `LML_0x002C`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LmlClockDivider {
    /// `TXTSPCLK_DIV`, bits 15:8.
    #[bits(8..=15)]
    tx: u8,

    /// `RXTSPCLK_DIV`, bits 7:0.
    #[bits(0..=7)]
    rx: u8,
}

impl Register for LmlClockDivider {
    const ADDR: Reg = Reg::LmlClockDivider;
}

/// Clock inversions at the processors and the `LimeLight` FIFOs. `CDS_0x00AD`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CdsInvert {
    /// `MCLK2` delay, bits 15:14.
    #[bits(14..=15)]
    mclk2: u2,

    /// `MCLK1` delay, bits 13:12.
    #[bits(12..=13)]
    mclk1: u2,

    /// `RESERVED`, bits 11:10.
    #[bits(10..=11)]
    reserved: u2,

    /// `NTXBTSP`, bit 9.
    #[bits(9..=9)]
    tx_b_tsp_inverted: bool,

    /// `NTXATSP`, bit 8.
    #[bits(8..=8)]
    tx_a_tsp_inverted: bool,

    /// `NRXBTSP`, bit 7.
    #[bits(7..=7)]
    rx_b_tsp_inverted: bool,

    /// `NRXATSP`, bit 6.
    #[bits(6..=6)]
    rx_a_tsp_inverted: bool,

    /// `NTXBLML`, bit 5.
    #[bits(5..=5)]
    tx_b_lml_inverted: bool,

    /// `NTXALML`, bit 4.
    #[bits(4..=4)]
    tx_a_lml_inverted: bool,

    /// `NRXBLML`, bit 3.
    #[bits(3..=3)]
    rx_b_lml_inverted: bool,

    /// `NRXALML`, bit 2.
    #[bits(2..=2)]
    rx_a_lml_inverted: bool,

    /// `NMCLK2`, bit 1.
    #[bits(1..=1)]
    mclk2_inverted: bool,

    /// `NMCLK1`, bit 0.
    #[bits(0..=0)]
    mclk1_inverted: bool,
}

impl Register for CdsInvert {
    const ADDR: Reg = Reg::CdsInvert;
}

/// Clock delays at the processors and the `LimeLight` FIFOs. `CDS_0x00AE`.
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CdsDelay {
    /// `TXBTSP`, bits 15:14.
    #[bits(14..=15)]
    tx_b_tsp: u2,

    /// `TXATSP`, bits 13:12.
    #[bits(12..=13)]
    tx_a_tsp: u2,

    /// `RXBTSP`, bits 11:10.
    #[bits(10..=11)]
    rx_b_tsp: u2,

    /// `RXATSP`, bits 9:8.
    #[bits(8..=9)]
    rx_a_tsp: u2,

    /// `TXBLML`, bits 7:6.
    #[bits(6..=7)]
    tx_b_lml: u2,

    /// `TXALML`, bits 5:4.
    #[bits(4..=5)]
    tx_a_lml: u2,

    /// `RXBLML`, bits 3:2.
    #[bits(2..=3)]
    rx_b_lml: u2,

    /// `RXALML`, bits 1:0.
    #[bits(0..=1)]
    rx_a_lml: u2,
}

impl Register for CdsDelay {
    const ADDR: Reg = Reg::CdsDelay;
}
