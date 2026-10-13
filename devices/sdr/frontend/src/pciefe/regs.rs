//! The `PCIe` front end's expander outputs (`device/ext_pciefe/pciefe.yaml`): what each
//! TCA6416 output pin switches, per board version.
//!
//! The V1 YAML names some fields `EN_*` ("Enable ...") although libusdr writes the inverse
//! (`pa_en ? 0 : 1`). Those fields are named for what a set bit means in libusdr's use
//! (`pa1_off`, ...); which of the two the hardware follows is not verified here.

use bnb::{BitEnum, bitfield, u2, u3};

/// RX attenuator setting, 6 dB per step (`ATTN`; V0 `FE1` bits 5:4, V1 `FE` bits 15:14).
/// Its two bits are swapped against the step count.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum AttenuatorCode {
    /// Insertion loss only (`IL`).
    Through = 0b00,

    /// 12 dB (`12DB`).
    Db12 = 0b01,

    /// 6 dB (`6DB`).
    Db6 = 0b10,

    /// 18 dB (`18DB`).
    Db18 = 0b11,
}

/// Duplexer switch position (`DUPL_PATH`; V0 `FE1` bits 2:0, V1 `FE` bits 9:7).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
#[repr(u8)]
pub(super) enum DuplexerCode {
    /// RF1: the band 5 duplexer (`RF1_BAND5`).
    Band5 = 0b001,

    /// RF2: the band 8 duplexer (`RF2_BAND8`).
    Band8 = 0b010,

    /// RF3: the band 2 duplexer (`RF3_BAND2`).
    Band2 = 0b011,

    /// RF4: no duplexer (`RF4_BYPASS`).
    Bypass = 0b100,

    /// RF5: the band 3 duplexer (`RF5_BAND3`).
    Band3 = 0b101,

    /// RF6: the band 7 duplexer (`RF6_BAND7`).
    Band7 = 0b110,

    /// Codes the YAML does not list.
    #[catch_all]
    Other(u3),
}

/// The V1 board's RX filter pair, U39 then U41 (`RXSEL`, `FE` bits 5:3). Only the codes
/// libusdr selects are named.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
#[repr(u8)]
pub(super) enum V1RxSelect {
    /// 2.1–3 GHz band-pass through both (`BPF_2100_3000_BPF_2100_3000`).
    Bpf2100To3000 = 0b001,

    /// 2.1 GHz low-pass through both (`LPF_2100_LPF_2100`).
    Lpf2100 = 0b011,

    /// 1.2 GHz low-pass through both (`LPF_1200_LPF_1200`).
    Lpf1200 = 0b100,

    /// 3–4.2 GHz band-pass through both (`BPF_3000_4200_BPF_3000_4200`).
    Bpf3000To4200 = 0b110,

    /// Mixed pairs (codes 0, 2, 5 and 7), which libusdr never selects.
    #[catch_all]
    Other(u3),
}

/// The V1 board's TX filter pair, U38 then U40 (`TXSEL`, `FE` bits 2:0). Only the codes
/// libusdr selects are named.
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u3)]
#[repr(u8)]
pub(super) enum V1TxSelect {
    /// 400 MHz low-pass through both (`LPF_400_LPF_400`).
    Lpf400 = 0b001,

    /// 2.1 GHz low-pass through both (`LPF_2100_LPF_2100`).
    Lpf2100 = 0b011,

    /// 1.2 GHz low-pass through both (`LPF_1200_LPF_1200`).
    Lpf1200 = 0b100,

    /// 4 GHz low-pass through both (`LPF_4000_LPF_4000`), libusdr's TX bypass.
    Lpf4000 = 0b110,

    /// Mixed pairs (codes 0, 2, 5 and 7), which libusdr never selects.
    #[catch_all]
    Other(u3),
}

/// The V1 board's RF switches and amplifiers: the front-end expander's two ports, port 0
/// in the low byte (`V1 FE`, 0x20).
#[bitfield(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct V1FrontEnd {
    /// RX attenuator. `ATTN`, bits 15:14.
    #[bits(14..=15)]
    attenuator: AttenuatorCode,

    /// Take the reference clock from connector J9 rather than the board's 25 MHz
    /// oscillator. `REFCLK_PATH`, bit 13; libusdr sets it while the oscillator is off.
    #[bits(13..=13)]
    external_reference: bool,

    /// TX power amplifier 1 off. `EN_PA1`, bit 12.
    #[bits(12..=12)]
    pa1_off: bool,

    /// RX LNA 2 off. `EN_LNA2`, bit 11.
    #[bits(11..=11)]
    lna2_off: bool,

    /// RX LNA 1 off. `EN_LNA1`, bit 10.
    #[bits(10..=10)]
    lna1_off: bool,

    /// Duplexer switch. `DUPL_PATH`, bits 9:7.
    #[bits(7..=9)]
    duplexer: DuplexerCode,

    /// TX power amplifier 2 off. `EN_PA2`, bit 6.
    #[bits(6..=6)]
    pa2_off: bool,

    /// RX filter pair. `RXSEL`, bits 5:3.
    #[bits(3..=5)]
    rx: V1RxSelect,

    /// TX filter pair. `TXSEL`, bits 2:0.
    #[bits(0..=2)]
    tx: V1TxSelect,
}

/// The V1 board's level-translator directions, LEDs and enables: the GPIO expander's
/// port 0 (`V1 GPIO0`, 0x22). A direction bit set drives the pin from the uSDR towards
/// the board's logic (libusdr's `DIRxx 1: USDR -> PLD`).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct V1Gpio0 {
    /// Pins C and D driven from the uSDR. `DIR_CD`, bit 7.
    #[bits(7..=7)]
    cd_out: bool,

    /// Pins 6 and 7 driven from the uSDR. `DIR_67`, bit 6.
    #[bits(6..=6)]
    pins67_out: bool,

    /// Pins 8 and 9 driven from the uSDR. `DIR_89`, bit 5.
    #[bits(5..=5)]
    pins89_out: bool,

    /// Pins A and B driven from the uSDR. `DIR_AB`, bit 4.
    #[bits(4..=4)]
    ab_out: bool,

    /// LED 2's control, which the YAML calls inverted. `LED2`, bit 3.
    #[bits(3..=3)]
    led2: bool,

    /// LED 1's control, which the YAML calls inverted. `LED1`, bit 2.
    #[bits(2..=2)]
    led1: bool,

    /// GPS module on. `EN_GPS`, bit 1.
    #[bits(1..=1)]
    gps_on: bool,

    /// Reference oscillator on. `EN_OSC`, bit 0.
    #[bits(0..=0)]
    oscillator_on: bool,
}

/// The V1 board's remaining LEDs and level-shifter outputs: the GPIO expander's port 1
/// (`V1 GPIO1`, 0x23). libusdr leaves the `LS_*` outputs (bits 5:0) low, so they are not
/// named.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct V1Gpio1 {
    /// LED 4's control, which the YAML calls inverted. `LED4`, bit 7.
    #[bits(7..=7)]
    led4: bool,

    /// LED 3's control, which the YAML calls inverted. `LED3`, bit 6.
    #[bits(6..=6)]
    led3: bool,
}

/// TX selector U28, the lower of the V0 board's pair (`TXSEL2`, `V0 FE0` bits 7:6).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum V0TxSelect2 {
    /// RF4: 1.2 GHz low-pass (`RF4_LPF_1200`).
    Lpf1200 = 0b00,

    /// RF1: bypass (`RF1_BYPASS`).
    Bypass = 0b01,

    /// RF2: 400 MHz low-pass (`RF2_LPF_400`).
    Lpf400 = 0b10,

    /// RF3: 2.1 GHz low-pass (`RF3_LPF_2100`).
    Lpf2100 = 0b11,
}

/// RX selector U29, the lower of the V0 board's pair (`RXSEL2`, `V0 FE0` bits 5:4).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum V0RxSelect2 {
    /// RF4: 3–4.2 GHz band-pass (`RF4_BPF_3000_4200`).
    Bpf3000To4200 = 0b00,

    /// RF1: 2.1 GHz low-pass (`RF1_LPF_2100`).
    Lpf2100 = 0b01,

    /// RF2: 1.2 GHz low-pass (`RF2_LPF_1200`).
    Lpf1200 = 0b10,

    /// RF3: 2.1–3 GHz band-pass (`RF3_BPF_2100_3000`).
    Bpf2100To3000 = 0b11,
}

/// RX selector U27, the upper of the V0 board's pair (`RXSEL1`, `V0 FE0` bits 3:2).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum V0RxSelect1 {
    /// RF4: 1.2 GHz low-pass (`RF4_LPF_1200`).
    Lpf1200 = 0b00,

    /// RF2: 3–4.2 GHz band-pass (`RF2_BPF_3000_4200`).
    Bpf3000To4200 = 0b01,

    /// RF1: 2.1–3 GHz band-pass (`RF1_BPF_2100_3000`).
    Bpf2100To3000 = 0b10,

    /// RF3: 2.1 GHz low-pass (`RF3_LPF_2100`).
    Lpf2100 = 0b11,
}

/// TX selector U26, the upper of the V0 board's pair (`TXSEL1`, `V0 FE0` bits 1:0).
#[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq)]
#[bit_enum(u2)]
pub(super) enum V0TxSelect1 {
    /// RF4: 400 MHz low-pass (`RF4_LPF_400`).
    Lpf400 = 0b00,

    /// RF1: 2.1 GHz low-pass (`RF1_LPF_2100`).
    Lpf2100 = 0b01,

    /// RF2: 1.2 GHz low-pass (`RF2_LPF_1200`).
    Lpf1200 = 0b10,

    /// RF3: bypass (`RF3_BYPASS`).
    Bypass = 0b11,
}

/// The V0 board's filter selectors: the front-end expander's port 0 (`V0 FE0`, 0x00).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct V0FrontEnd0 {
    /// TX selector U28. `TXSEL2`, bits 7:6.
    #[bits(6..=7)]
    tx2: V0TxSelect2,

    /// RX selector U29. `RXSEL2`, bits 5:4.
    #[bits(4..=5)]
    rx2: V0RxSelect2,

    /// RX selector U27. `RXSEL1`, bits 3:2.
    #[bits(2..=3)]
    rx1: V0RxSelect1,

    /// TX selector U26. `TXSEL1`, bits 1:0.
    #[bits(0..=1)]
    tx1: V0TxSelect1,
}

/// The V0 board's amplifiers, attenuator, loopback and duplexer: the front-end
/// expander's port 1 (`V0 FE1`, 0x01).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct V0FrontEnd1 {
    /// RX LNA on. `EN_LNA`, bit 7.
    #[bits(7..=7)]
    lna_on: bool,

    /// TX power amplifier on. `EN_PA`, bit 6.
    #[bits(6..=6)]
    pa_on: bool,

    /// RX attenuator. `ATTN`, bits 5:4.
    #[bits(4..=5)]
    attenuator: AttenuatorCode,

    /// TX-to-RX loopback off; the line is inverted. `NLOOPBACK`, bit 3.
    #[bits(3..=3)]
    loopback_off: bool,

    /// Duplexer switch. `DUPL_PATH`, bits 2:0.
    #[bits(0..=2)]
    duplexer: DuplexerCode,
}

/// The V0 board's level-translator directions and enables: the GPIO expander's port 0
/// (`V0 GPIO0`, 0x02). libusdr leaves the `LS_*` outputs (bits 5:2) low, so they are not
/// named.
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct V0Gpio0 {
    /// Pins 4 and 5 driven from the uSDR. `DIR_45`, bit 7.
    #[bits(7..=7)]
    pins45_out: bool,

    /// Pins 2 and 3 driven from the uSDR. `DIR_23`, bit 6.
    #[bits(6..=6)]
    pins23_out: bool,

    /// GPS module on. `EN_GPS`, bit 1.
    #[bits(1..=1)]
    gps_on: bool,

    /// Reference oscillator on. `EN_OSC`, bit 0.
    #[bits(0..=0)]
    oscillator_on: bool,
}

/// The V0 board's LEDs and level-translator directions: the GPIO expander's port 1
/// (`V0 GPIO1`, 0x03).
#[bitfield(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct V0Gpio1 {
    /// LED 4's control, which the YAML calls inverted. `LED4`, bit 7.
    #[bits(7..=7)]
    led4: bool,

    /// LED 3's control, which the YAML calls inverted. `LED3`, bit 6.
    #[bits(6..=6)]
    led3: bool,

    /// LED 2's control, which the YAML calls inverted. `LED2`, bit 5.
    #[bits(5..=5)]
    led2: bool,

    /// LED 1's control, which the YAML calls inverted. `LED1`, bit 4.
    #[bits(4..=4)]
    led1: bool,

    /// Pins A and B driven from the uSDR. `DIR_AB`, bit 3.
    #[bits(3..=3)]
    ab_out: bool,

    /// Pins 8 and 9 driven from the uSDR. `DIR_89`, bit 2.
    #[bits(2..=2)]
    pins89_out: bool,

    /// Pins 0 and 1 driven from the uSDR. `DIR_01`, bit 1.
    #[bits(1..=1)]
    pins01_out: bool,

    /// Pins 6 and 7 driven from the uSDR. `DIR_67`, bit 0.
    #[bits(0..=0)]
    pins67_out: bool,
}
