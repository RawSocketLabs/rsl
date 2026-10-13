//! The gateware registers the `LimeSDR`'s open uses: the reference-clock frequency detector
//! and stream control. Facts from the pinned libusdr's `device/u3_limesdr/limesdr_ctrl.c`,
//! which names most registers only by address (`LIMESDR_REG_*`, "TODO: Rename these magic
//! constants").

/// The chip name failed gateware accesses carry.
pub(crate) const NAME: &str = "FPGA";

/// Frequency-detector control (`LIMESDR_REG_0061`); see [`DETECTOR_START`].
pub(crate) const DETECTOR_CONTROL: u16 = 0x0061;

/// [`DETECTOR_CONTROL`] bit 2, written to start a measurement (`limesdr_detect_refclk`
/// writes 0, then 4).
pub(crate) const DETECTOR_START: u16 = 1 << 2;

/// `LIMESDR_REG_0063`, which `limesdr_detect_refclk` clears before a measurement; the
/// source does not say what it holds.
pub(crate) const DETECTOR_0063: u16 = 0x0063;

/// Frequency-detector status (`LIMESDR_REG_0065`); see [`DETECTOR_DONE`].
pub(crate) const DETECTOR_STATUS: u16 = 0x0065;

/// [`DETECTOR_STATUS`] bit 2: the measurement is complete (the C tests `& 0x4`).
pub(crate) const DETECTOR_DONE: u16 = 1 << 2;

/// The measured count, low half (`LIMESDR_REG_0072`); the high half is the next register,
/// `LIMESDR_REG_0073`. libusdr reads them as one 32-bit value.
pub(crate) const DETECTOR_COUNT: u16 = 0x0072;

/// Stream control (`LIMESDR_REG_000A`); see [`RX_ENABLE`] and [`TX_ENABLE`].
pub(crate) const STREAM_CONTROL: u16 = 0x000a;

/// [`STREAM_CONTROL`] bit 0, "controls both receiver and transmitter" (`RX_EN`).
pub(crate) const RX_ENABLE: u16 = 1 << 0;

/// [`STREAM_CONTROL`] bit 1, "used for wfm playback from fpga" (`TX_EN`).
pub(crate) const TX_ENABLE: u16 = 1 << 1;

/// Direct clocking (`LIMESDR_REG_0005`): bit `n` makes `LimeLight` clock `n` direct, the
/// PLL not in the path (`limesdr_set_direct_clocking`; the source names the bits no
/// further). The sample rate sets bits 0 and 1 ("TODO set direct mode or do PLL with phase
/// search").
pub(crate) const DIRECT_CLOCKING: u16 = 0x0005;

/// The RF switches (`LIMESDR_REG_0017`): the receive antenna path in bits 9:8 and the
/// transmit path in bits 13:12, each a band's switch position (`_limesdr_antenna_port_switch`).
pub(crate) const RF_SWITCHES: u16 = 0x0017;

/// Where [`RF_SWITCHES`] holds the receive path's position.
pub(crate) const RX_SWITCH_SHIFT: u16 = 8;

/// Where [`RF_SWITCHES`] holds the transmit path's position.
pub(crate) const TX_SWITCH_SHIFT: u16 = 12;
