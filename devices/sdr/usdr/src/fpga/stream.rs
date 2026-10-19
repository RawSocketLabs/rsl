//! The uSDR's side of the RX stream engine's arithmetic: which lanes and sample format a
//! stream's channels and wire take. The arithmetic itself is the engine's, shared with the
//! LMS7 family (`sdr-stream`'s [`BurstPlan`], re-exported here).
//!
//! Source: `ipblks/streams/sfe_rx_4.c` (`sfe_rx4_configure`).

pub(crate) use sdr_stream::BurstPlan;
use sdr_stream::{RxLanes, RxSampleFormat};

use crate::stream::{RxChannels, WireFormat};

impl From<WireFormat> for RxSampleFormat {
    /// `sfe_rx4_configure`'s `fe_format` for the wire format's bits.
    fn from(wire: WireFormat) -> Self {
        match wire {
            WireFormat::Ci16 | WireFormat::I16 => Self::Bits16,
            WireFormat::Ci12 | WireFormat::I12 => Self::Bits12,
        }
    }
}

/// The lanes `sfe_rx4_configure` selects for `channels` of `wire`: complex channel 0 is
/// lanes 1 and 0, channel 1 lanes 3 and 2, both all four; real channel 0 lane 0, real
/// channel 1 lane 1, and both (refused before any bus access: libusdr has no converter for
/// them) lanes 1 and 0.
pub(crate) fn rx_lanes(wire: WireFormat, channels: RxChannels) -> RxLanes {
    match (wire.is_complex(), channels) {
        (true, RxChannels::Channel0) | (false, RxChannels::Both) => RxLanes::Lanes10,
        (true, RxChannels::Channel1) => RxLanes::Lanes32,
        (true, RxChannels::Both) => RxLanes::Lanes3210,
        (false, RxChannels::Channel0) => RxLanes::Lane0,
        (false, RxChannels::Channel1) => RxLanes::Lane1,
    }
}

#[cfg(test)]
mod tests {
    use sdr_stream::RxFormat;

    use super::*;

    #[test]
    fn a_ci16_channel_0_packet_takes_libusdrs_front_end_words() {
        let wire = WireFormat::Ci16;
        let plan =
            BurstPlan::for_stream(1024, RxChannels::Channel0.sample_bits(wire)).expect("valid");
        assert_eq!(
            plan.rx_format(rx_lanes(wire, RxChannels::Channel0), wire.into()),
            // 16 bursts in the FIFO, 511 words, lanes 1 and 0, 16 bits.
            RxFormat::new(1024, 512, 1, 0x0040_3fe7).expect("valid")
        );
    }
}
