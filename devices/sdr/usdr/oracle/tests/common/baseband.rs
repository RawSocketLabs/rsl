//! The second intentional divergence, shared by the stream parity tests: libusdr's stream
//! create reapplies each channel's LO as a baseband offset; rsl-usdr does not.

use rsl_usdr_sim::Op;

use crate::crossing::{CLOCK, mixer_lo_block};

/// FPGA register latching GPO banks.
const REG_GPO: u32 = 0;
/// The RX DSP chain's configuration port.
const REG_PHY_RX: u32 = 56;
/// The RX DMA status register, read first by libusdr's DMA reset.
const REG_RX_DMA_STATUS: u32 = 6;
/// The SPI write of RXVGA2's gain register (0x68), whose low byte is the stage steps;
/// libusdr's DC calibration ends by restoring them.
const VGA2_GAIN_WRITE: u32 = 0xe800;

/// libusdr's create window without the per-channel baseband loop.
///
/// Intentional divergence, the second: after its DC calibration, libusdr's stream create
/// reapplies each channel's LO as a baseband offset (`usdr_rfic_fe_set_freq(FE_FREQ_BB_RX,
/// .., rx_raw.lo[i])` in `usdr_device_m2_lm6_1_create_stream`), a bug that reselects the
/// band, clamps the NCOs and bypasses the filter. rsl-usdr leaves it out. The cut runs from
/// the calibration's last write to the DMA engine's status read; it must hold only what
/// that loop does (LMS6002D SPI, GPO and PHY writes, Si5332 writes, sleeps), so nothing
/// else can hide in it, and be non-empty unless `idle_loop`. The window between also holds
/// libusdr's TX PLL retune when a TX LO is set; TX is not ported, so it never is, and the
/// cut never hides one.
///
/// With `idle_loop` the cut must be empty: untuned, with a wide or high path fixed, the
/// loop selects no band and retunes to 0 Hz, which fails before any operation.
///
/// Returns the trimmed window and whether the cut turned the board mixer on: with no
/// frequency set, libusdr's loop selects the mixer band, and its next tune out of that band
/// writes a mixer-off block rsl-usdr has no reason to (`stream_parity.rs` cuts it).
pub(crate) fn without_baseband_loop(window: &[Op], idle_loop: bool) -> (Vec<Op>, bool) {
    let dma_reset = window
        .iter()
        .position(|op| matches!(op, Op::RegRead { addr, .. } if *addr == REG_RX_DMA_STATUS))
        .expect("create resets the DMA engine");
    let calibrated = window[..dma_reset]
        .iter()
        .rposition(
            |op| matches!(op, Op::Spi { bus: 0, out, .. } if *out & !0xff == VGA2_GAIN_WRITE),
        )
        .expect("create calibrates first")
        + 1;
    let cut = &window[calibrated..dma_reset];
    assert_eq!(
        cut.is_empty(),
        idle_loop,
        "libusdr's baseband loop operates unless the scenario says it idles"
    );
    assert!(
        cut.iter().all(|op| matches!(
            op,
            Op::Spi { .. }
                | Op::Sleep { .. }
                | Op::RegWrite {
                    addr: REG_GPO | REG_PHY_RX,
                    ..
                }
        ) || matches!(op, Op::I2c { addr, .. } if *addr == CLOCK)),
        "the cut holds only the baseband loop's operations: {cut:x?}"
    );
    let mixer_on = cut.windows(6).any(|ops| ops == mixer_lo_block(true));
    (
        [&window[..calibrated], &window[dma_reset..]].concat(),
        mixer_on,
    )
}
