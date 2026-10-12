//! The band-crossing divergence, shared by the parity tests whose calls move the RX path
//! into or out of the mixer band.

use rsl_usdr_sim::{I2cAddress, Op};

/// The Si5332.
pub(crate) const CLOCK: I2cAddress = I2cAddress { bus: 0, addr: 0x6a };

/// Whether a scenario expects rsl-usdr's band-crossing fix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Crossing {
    /// The scenario never enters or leaves the mixer band.
    None,

    /// The scenario crosses into or out of the mixer band, so libusdr's
    /// `si5332_set_port3_en` block is expected to be replaced by rsl-usdr's one write.
    Fixed,
}

/// libusdr's window with its `si5332_set_port3_en` block replaced by the single `OUT3_OE`
/// write rsl-usdr makes instead, and whether the window had one.
///
/// Intentional divergence, scoped to the scenarios that ask for it: libusdr requests READY
/// to rewrite the power-down registers, which stops every Si5332 output; rsl-usdr flips
/// `OUT3_OE` (0xB6 bit 6) while ACTIVE. The block must be exactly libusdr's six writes for
/// an RX-only device, so nothing else can hide in the cut.
pub(crate) fn with_band_crossing_fix(window: &[Op]) -> (Vec<Op>, bool) {
    let Some(start) = window
        .iter()
        .position(|op| matches!(op, Op::I2c { addr, write, .. } if *addr == CLOCK && write.first() == Some(&0xb6)))
    else {
        return (window.to_vec(), false);
    };
    let on = matches!(&window[start], Op::I2c { write, .. } if write[..] == [0xb6, 0x43]);
    let libusdr_block = mixer_lo_block(on);
    let block = window
        .get(start..start + libusdr_block.len())
        .expect("libusdr's set_port3_en block is complete");
    assert_eq!(
        block, libusdr_block,
        "libusdr's set_port3_en block is as expected"
    );
    let fixed = clock_write([0xb6, if on { 0xff } else { 0xbf }]);
    let fixed_window = [
        &window[..start],
        &[fixed],
        &window[start + libusdr_block.len()..],
    ]
    .concat();
    (fixed_window, true)
}

/// libusdr's `si5332_set_port3_en` writes for an RX-only device: OUT3 (with OUT0/1, TX not
/// running) on or off, READY, the power-downs, ACTIVE.
pub(crate) fn mixer_lo_block(on: bool) -> [Op; 6] {
    if on {
        [
            [0xb6, 0x43],
            [0x06, 0x01],
            [0xba, 0x76],
            [0xbb, 0x34],
            [0xbc, 0x08],
            [0x06, 0x02],
        ]
    } else {
        [
            [0xb6, 0x03],
            [0x06, 0x01],
            [0xba, 0x7e],
            [0xbb, 0x3c],
            [0xbc, 0x48],
            [0x06, 0x02],
        ]
    }
    .map(clock_write)
}

/// A write of `[register, value]` to the Si5332.
fn clock_write(bytes: [u8; 2]) -> Op {
    Op::I2c {
        addr: CLOCK,
        write: bytes.to_vec(),
        read: Vec::new(),
    }
}
