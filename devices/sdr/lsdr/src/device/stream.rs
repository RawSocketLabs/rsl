//! The RX stream's create: `usdr_device_m2_d09_4_ad45_2_create_stream` in
//! `device/m2_lsdr/m2_da09_4_ad45_2.c` for `/ll/srx/0` in `ci16`, with the stream engine's
//! part (`ipblks/streams/stream_sfetrx4_dma32.c`, `CORE_SFERX_DMA32_R0`) from
//! `sdr-stream`, the uSDR's and LMS7 family's engine at the same registers.
//!
//! At the pinned libusdr the create cannot succeed: after configuring the RX front end and
//! DMA, `initialize_stream_rx_32` holds the sync unit stopped (`sfetrx4_stream_sync`), whose
//! register it looks up as `/ll/sync/0/base`, which the `LSDr` factory does not declare;
//! the lookup fails with `-ENOENT` and so does the create, before the transport is asked
//! for its buffers. [`Device::create_rx_stream`] does the same writes and fails the same
//! way, so nothing past it (start, receive, stop) is reachable in libusdr to port.

use std::time::Duration;

use hal::SequenceAction;
use sdr_stream::{BurstPlan, EngineSequence, RxLanes, RxSampleFormat};

use super::{Device, run};
use crate::board::LSDR_WIRING;
use crate::error::Error;
use crate::portable::adc_power::AdcPower;
use crate::portable::fpga::{self, Gpo};
use crate::portable::single::Single;

/// Bits of one `ci16` sample on one channel: I then Q, 16 bits each.
const SAMPLE_BITS: u32 = 32;

/// The DSP chain's reset pulse at create (`usleep(1000)`).
const DSP_RESET_PULSE: Duration = Duration::from_millis(1);

/// The receive channels a stream carries: the first ADC's two (libusdr's `s_lsdr_chmap`:
/// `0a`/`a` is channel 0, `0b`/`b` channel 1; the second ADC's are commented out there,
/// "disable now").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RxChannels {
    /// Channel A (libusdr's channel mask 1): the front end's lanes 1 and 0.
    A,

    /// Channel B (mask 2): lanes 3 and 2.
    B,

    /// Both, A then B in each sample (mask 3): all four lanes.
    Both,
}

impl RxChannels {
    /// How many channels the stream carries.
    const fn count(self) -> u32 {
        match self {
            Self::A | Self::B => 1,
            Self::Both => 2,
        }
    }

    /// The lanes `sfe_rx4_configure` selects for the channels' complex samples.
    const fn lanes(self) -> RxLanes {
        match self {
            Self::A => RxLanes::Lanes10,
            Self::B => RxLanes::Lanes32,
            Self::Both => RxLanes::Lanes3210,
        }
    }
}

impl Device {
    /// Creates the RX stream on `channels` with `samples_per_packet` samples per packet,
    /// in `ci16` (libusdr's `usdr_dms_create` of `/ll/srx/0`), as far as libusdr does at
    /// the pinned revision, and fails where it fails ([`Error::NoSyncUnit`]).
    ///
    /// As libusdr: both ADCs are powered up as by [`Self::set_power`], the DSP chain's
    /// reset pulsed, the RX DMA reset, and the RX front end and DMA configured for the
    /// packet's bursts; then the sync unit, which the `LSDr` does not declare, cannot be
    /// held stopped. The board is left with the ADCs powered and the engine configured.
    ///
    /// # Errors
    ///
    /// [`Error::NoSyncUnit`] once everything else succeeded, always at the pin.
    /// Before it: [`Error::Bus`] when an access fails (libusdr ignores a failure while
    /// powering the ADCs; a failed reset raise still takes its 1 ms wait);
    /// [`Error::ChipMissing`] for an RX DMA engine that reads dead;
    /// [`Error::UnsupportedPacketSize`] after the DMA reset, as libusdr checks it there.
    pub fn create_rx_stream(
        &mut self,
        channels: RxChannels,
        samples_per_packet: u32,
    ) -> Result<(), Error> {
        let bus = &mut *self.bus;
        run(bus, &mut AdcPower::new(LSDR_WIRING, true))?;
        let dsp_reset =
            |level| Single::new(SequenceAction::Fpga(Gpo::DspReset.write(level)), fpga::NAME);
        let raised = run(bus, &mut dsp_reset(1));
        run(
            bus,
            &mut Single::new(SequenceAction::Sleep(DSP_RESET_PULSE), fpga::NAME),
        )?;
        raised?;
        run(bus, &mut dsp_reset(0))?;
        run(bus, &mut EngineSequence::rx_reset())?;
        let plan = BurstPlan::for_stream(samples_per_packet, SAMPLE_BITS * channels.count())
            .ok_or(Error::UnsupportedPacketSize(samples_per_packet))?;
        let format = plan.rx_format(channels.lanes(), RxSampleFormat::Bits16);
        run(bus, &mut EngineSequence::new().configure_rx(&format))?;
        Err(Error::NoSyncUnit)
    }
}
