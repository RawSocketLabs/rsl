//! The second intentional divergence, shared by the stream parity tests: libusdr's stream
//! create reapplies each channel's LO as a baseband offset; rsl-usdr does not.

use std::ops::RangeInclusive;

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
/// The SPI write that ends a TX PLL tune: the comparator powered down (0x1B = 0x7E).
const TX_PLL_TUNED: u32 = 0x9b7e;
/// The LMS6002D's TX PLL registers, which the baseband loop never touches.
const TX_PLL: RangeInclusive<u32> = 0x10..=0x1f;

/// libusdr's create window without the per-channel baseband loop.
///
/// Intentional divergence, the second: after its DC calibration, libusdr's stream create
/// reapplies each channel's LO as a baseband offset (`usdr_rfic_fe_set_freq(FE_FREQ_BB_RX,
/// .., rx_raw.lo[i])` in `usdr_device_m2_lm6_1_create_stream`), a bug that reselects the
/// band, clamps the NCOs and bypasses the filter. rsl-usdr leaves it out. The cut runs from
/// the calibration's last write, or the TX PLL retune that follows it when a TX LO is set
/// (which ends with [`TX_PLL_TUNED`]), to the DMA engine's status read; it must hold only
/// what that loop does (LMS6002D SPI off the TX PLL, GPO and PHY writes, Si5332 writes,
/// sleeps), so nothing else can hide in it, and be non-empty unless `idle_loop`. The
/// retune stays compared: the loop tunes only the RX PLL, so it never writes
/// [`TX_PLL_TUNED`], and a retune cut short before that write would fail the TX PLL check.
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
    let calibrated = window[calibrated..dma_reset]
        .iter()
        .rposition(|op| {
            matches!(
                op,
                Op::Spi {
                    bus: 0,
                    out: TX_PLL_TUNED,
                    ..
                }
            )
        })
        .map_or(calibrated, |retuned| calibrated + retuned + 1);
    let cut = &window[calibrated..dma_reset];
    assert_eq!(
        cut.is_empty(),
        idle_loop,
        "libusdr's baseband loop operates unless the scenario says it idles"
    );
    assert!(
        cut.iter().all(|op| matches!(
            op,
            Op::Sleep { .. }
                | Op::RegWrite {
                    addr: REG_GPO | REG_PHY_RX,
                    ..
                }
        ) || matches!(op, Op::Spi { out, .. } if !TX_PLL.contains(&(out >> 8 & 0x7f)))
            || matches!(op, Op::I2c { addr, .. } if *addr == CLOCK)),
        "the cut holds only the baseband loop's operations, none on the TX PLL: {cut:x?}"
    );
    let mixer_on = cut.windows(6).any(|ops| ops == mixer_lo_block(true));
    (
        [&window[..calibrated], &window[dma_reset..]].concat(),
        mixer_on,
    )
}
