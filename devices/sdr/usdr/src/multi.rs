//! Several uSDRs driven as one: libusdr's multi-device aggregate (`device/mdev.c`, opened
//! with a `bus=a:b:...` connection string).
//!
//! The aggregate is a list of [`Device`]s, the first its master. libusdr runs every
//! operation on each device in turn and stops at the first failure; so does
//! [`MultiDevice`], but it reports what each device did ([`MultiError`], [`Outcome`])
//! where libusdr returns one error code and, on success, the master's results alone.
//! Nothing here is atomic: a failure leaves the devices before it changed.
//!
//! Only the uSDR has a full driver, so the aggregate lives here; once a second SDR driver
//! needs one, the device-independent part (ordering, outcomes) moves to `devices/sdr/`.

use std::error::Error as StdError;
use std::fmt;
use std::mem;
use std::time::Duration;

use deps::tracing::debug;

use crate::board::{Board, Direction};
use crate::device::{Device, DeviceBuilder};
use crate::error::Error;
use crate::frontend::FrontEndKind;
use crate::portable::stream::SyncTrigger;
use crate::reference::ReferenceClock;
use crate::stream::{HostSample, RxMetadata, StreamTime, StreamsConfig, SyncMode, TxTelemetry};
use sdr_stream::StartPart;

/// The reference every device takes when the master's front end is `SimpleSync`, in Hz:
/// libusdr's `_debug_ll_mdev_set` (`device/device_fe.c`) sets `/dm/sdr/refclk/path` to
/// `"external"`, then `/dm/sdr/refclk/frequency` to 50 MHz, on the aggregate.
const SIMPLESYNC_REFERENCE_HZ: u32 = 50_000_000;

/// One step of a device's stop, which [`MultiDevice::stop_streams`] runs on every device.
type StopStep = fn(&mut Board) -> Result<(), Error>;

/// Several powered uSDRs driven as one (libusdr's multi-device aggregate): settings go to
/// every device in turn ([`MultiDevice::broadcast`]), and one stream group spans them,
/// device `i` carrying the aggregate's channels `i * k .. (i + 1) * k` for its `k`
/// channels. The first device is the master: it reads for the aggregate where libusdr
/// reads from it, and with [`SyncMode::GeneratedSysref`] it generates the SYSREF every
/// device starts on.
///
/// libusdr's aggregate takes devices of one type; every [`Device`] is a uSDR, of any
/// revision, as libusdr allows.
///
/// A device's own methods stay available through [`MultiDevice::device_mut`], for settings
/// that differ between devices. Each device's stream group is its share of the
/// aggregate's, held by the device itself: one stopped, started or created there makes
/// the aggregate's stream calls refuse or fail on that device, and the outcomes say so.
#[derive(Debug)]
pub struct MultiDevice {
    /// The devices, the master first; never empty.
    devices: Vec<Device>,

    /// The master generates SYSREF once every device's group waits for 1PPS: the
    /// aggregate's group was created with [`SyncMode::GeneratedSysref`].
    master_generates_sysref: bool,
}

/// An aggregate call that did not complete on every device.
#[derive(Debug)]
#[non_exhaustive]
pub enum MultiError<T> {
    /// Refused before any bus access on any device: no device has the streams the call
    /// needs, or the buffers do not match the aggregate's channels.
    Refused(Error),

    /// The call reached the devices and failed on at least one.
    Devices {
        /// The step that failed. Earlier steps of the call completed on every device;
        /// later ones ran on none.
        step: MultiStep,

        /// What the step did on each device, by device index.
        outcomes: Vec<Outcome<T>>,
    },
}

/// One step of an aggregate call, which runs on each device in turn before the next step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MultiStep {
    /// Opening the devices ([`MultiDevice::open`]). A failure closes the devices opened
    /// before it again.
    Open,

    /// Moving every device to the reference a `SimpleSync` master distributes
    /// ([`MultiDevice::open`]). A failure closes every device.
    SharedReference,

    /// The refusals each device makes before any bus access. Every device is checked:
    /// those that refused are `Failed`, those that passed `NotReached`, since the call
    /// itself ran on none of them.
    Check,

    /// The call itself on each device: a [`MultiDevice::broadcast`],
    /// [`MultiDevice::receive_into`] or [`MultiDevice::send`].
    Call,

    /// Creating the RX streams (libusdr's `usdr_dms_create` of `/ll/srx/0`).
    CreateRx,

    /// Creating the TX streams (`/ll/stx/0`).
    CreateTx,

    /// Running the RX streams (`USDR_DMS_START` of the RX stream).
    StartRx,

    /// Starting the TX streams (`USDR_DMS_START` of the TX stream).
    StartTx,

    /// Setting each device's sync trigger (`usdr_dms_sync`).
    Sync,

    /// The master's SYSREF generator, after every device waits for 1PPS.
    MasterSysref,

    /// Stopping and destroying the stream groups ([`MultiDevice::stop_streams`]); every
    /// device and step is attempted, and each device's first failure reported.
    Stop,

    /// Closing the devices ([`MultiDevice::close`]); every device is attempted.
    Close,
}

/// What one step did on one device.
#[derive(Debug)]
pub enum Outcome<T> {
    /// It completed, with this result.
    Done(T),

    /// It failed.
    Failed(Error),

    /// It never ran there: the step stopped at an earlier device's failure, as libusdr's
    /// aggregate stops.
    NotReached,
}

impl MultiDevice {
    /// Opens the master, then each member in turn, as libusdr's `mdev_create` opens the
    /// devices of its `bus=` list, then, if the master's front end is `SimpleSync`, moves
    /// every device to the 50 MHz reference it distributes
    /// ([`ReferenceClock::External`], libusdr's `/ll/mdev` hook).
    ///
    /// # Errors
    ///
    /// [`MultiError::Devices`] at [`MultiStep::Open`] with the first device that failed to
    /// open, or at [`MultiStep::SharedReference`]. Every device opened is closed again
    /// (dropped, powering it down), as libusdr destroys them.
    ///
    /// Divergence: libusdr fails the whole open (`-ENOENT`) when the master has no front
    /// end (`/ll/mdev` exists only with one), say with `fe=none`; this opens.
    pub fn open<'a>(
        master: DeviceBuilder<'a>,
        members: impl IntoIterator<Item = DeviceBuilder<'a>>,
    ) -> Result<Self, MultiError<()>> {
        let builders: Vec<DeviceBuilder<'a>> = [master].into_iter().chain(members).collect();
        let count = builders.len();
        let mut devices = Vec::with_capacity(count);
        for builder in builders {
            match builder.open() {
                Ok(device) => devices.push(device),
                Err(error) => {
                    let opened = devices.len();
                    // Dropping powers each opened device down, as libusdr destroys them.
                    drop(devices);
                    return Err(MultiError::Devices {
                        step: MultiStep::Open,
                        outcomes: outcomes(vec![(); opened], error, count),
                    });
                }
            }
        }
        let mut multi = Self {
            devices,
            master_generates_sysref: false,
        };
        if multi.master().front_end() == Some(FrontEndKind::SimpleSync) {
            let shared = ReferenceClock::External {
                hz: SIMPLESYNC_REFERENCE_HZ,
            };
            multi.each(MultiStep::SharedReference, |_, device| {
                device.set_reference_clock(shared)
            })?;
        }
        debug!(devices = count, "multi-device aggregate opened");
        Ok(multi)
    }

    /// How many devices the aggregate holds (libusdr's `/ll/devices`): at least one.
    #[must_use]
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// The master, device 0.
    #[must_use]
    pub fn master(&self) -> &Device {
        &self.devices[0]
    }

    /// The master, device 0, for reads libusdr's aggregate takes from it.
    pub fn master_mut(&mut self) -> &mut Device {
        &mut self.devices[0]
    }

    /// Device `index`, if the aggregate has it.
    #[must_use]
    pub fn device(&self, index: usize) -> Option<&Device> {
        self.devices.get(index)
    }

    /// Device `index`, if the aggregate has it, for a setting of its own.
    pub fn device_mut(&mut self, index: usize) -> Option<&mut Device> {
        self.devices.get_mut(index)
    }

    /// Runs `set` on each device in turn, the master first, stopping at the first failure:
    /// libusdr's aggregate setting a value (`_mdev_obj_set_i64`), such as
    /// `multi.broadcast(|device| device.set_rx_frequency(hz))`. Returns each device's
    /// result.
    ///
    /// # Errors
    ///
    /// [`MultiError::Devices`] at [`MultiStep::Call`]: the devices before the failed one
    /// have the setting, those after it are untouched.
    pub fn broadcast<T>(
        &mut self,
        mut set: impl FnMut(&mut Device) -> Result<T, Error>,
    ) -> Result<Vec<T>, MultiError<T>> {
        self.each(MultiStep::Call, |_, device| set(device))
    }

    /// Creates `config`'s streams on every device, stopped: each device's checks, then
    /// the RX stream on each device in turn, then the TX stream on each, as libusdr's
    /// aggregate creates them (`_mdev_create_stream` for each stream). Each device's
    /// stream is created as [`Device::create_streams`] creates it; the aggregate's
    /// channels are each device's in device order.
    ///
    /// The sync mode is the aggregate's ([`MultiDevice::start_streams`]). With
    /// [`SyncMode::GeneratedSysref`] each device's group waits for 1PPS
    /// ([`SyncMode::OnePps`]) and the master generates SYSREF after them (libusdr's
    /// aggregate `"sysref"`); libusdr refuses its direct `"sysref+gen"`, every device
    /// generating, which this cannot express. [`SyncMode::OnePps`] waits for a pulse from
    /// outside (`"1pps"`). The activity and free-running modes start each device on its
    /// own, unaligned with the others.
    ///
    /// # Errors
    ///
    /// - [`MultiError::Devices`] at [`MultiStep::Check`] for the refusals of
    ///   [`Device::create_streams`] before any bus access, on any device; no device is
    ///   touched.
    /// - [`MultiError::Devices`] at [`MultiStep::CreateRx`] or [`MultiStep::CreateTx`]:
    ///   as with [`Device::create_streams`], each device keeps the streams it created
    ///   (with the RX step failed, TX was created nowhere). Call
    ///   [`MultiDevice::stop_streams`] before creating again. libusdr leaks those streams
    ///   (it returns no handle to destroy them, and a retry fails `-EBUSY`).
    pub fn create_streams(&mut self, config: StreamsConfig) -> Result<(), MultiError<()>> {
        let member = match config.sync {
            SyncMode::GeneratedSysref => config.with_sync(SyncMode::OnePps),
            _ => config,
        };
        self.check(|device| device.board().check_streams(member))?;
        self.master_generates_sysref = config.sync == SyncMode::GeneratedSysref;
        if config.rx.is_some() {
            self.each(MultiStep::CreateRx, |_, device| {
                device.board().create_stream(member, Direction::Rx)
            })?;
        }
        if config.tx.is_some() {
            self.each(MultiStep::CreateTx, |_, device| {
                device.board().create_stream(member, Direction::Tx)
            })?;
        }
        Ok(())
    }

    /// Synchronises and starts the stream group on every device: each part of
    /// [`Device::start_streams`] on each device in turn before the next part, as libusdr's
    /// aggregate runs `usdr_dms_sync` and `USDR_DMS_START` (an RX-only group syncs, then
    /// runs; with TX, RX runs, then TX starts, then the devices sync). With
    /// [`SyncMode::GeneratedSysref`], the master then generates SYSREF, once every device
    /// waits for it (`_mdev_stream_sync`). On gateware whose sync unit ignores the
    /// generator bit, the devices wait for a 1PPS pulse from outside instead.
    ///
    /// # Errors
    ///
    /// - [`MultiError::Devices`] at [`MultiStep::Check`] before any bus access: a device
    ///   with no group ([`Error::NotStreaming`]), one already started
    ///   ([`Error::AlreadyStreaming`]), or one whose streams differ from the master's
    ///   ([`Error::UnmatchedStreams`], as a failed create leaves them).
    /// - [`MultiError::Devices`] at [`MultiStep::StartRx`], [`MultiStep::StartTx`],
    ///   [`MultiStep::Sync`] or [`MultiStep::MasterSysref`]: the devices before the failure
    ///   ran the step. Stop the group and create it again.
    ///
    /// The master generates SYSREF only while every device's group still waits for 1PPS,
    /// as the aggregate created them: a group a device recreated on its own, in another
    /// mode, leaves the generator off.
    pub fn start_streams(&mut self) -> Result<(), MultiError<()>> {
        let mut parts = Vec::with_capacity(self.devices.len());
        self.check(|device| {
            parts.push(device.board().start_parts()?);
            Ok(())
        })?;
        let waits_for_pps = |parts: &[Option<StartPart>; 3]| {
            parts.contains(&Some(StartPart::Sync(SyncTrigger::OnePps)))
        };
        let generate_sysref = self.master_generates_sysref && parts.iter().all(waits_for_pps);
        // Each device's parts as the aggregate's steps; the devices must agree.
        let steps = |parts: &[Option<StartPart>; 3]| parts.map(|part| part.map(MultiStep::of));
        let master_steps = steps(&parts[0]);
        self.check_each(|index, _| {
            if steps(&parts[index]) == master_steps {
                Ok(())
            } else {
                Err(Error::UnmatchedStreams)
            }
        })?;
        let last = master_steps.iter().rposition(Option::is_some);
        for (position, step) in master_steps.into_iter().enumerate() {
            let Some(step) = step else { continue };
            self.each(step, |index, device| {
                // Checked above: every device has the master's steps.
                let Some(part) = parts[index][position] else {
                    return Err(Error::UnmatchedStreams);
                };
                device.board().start_part(part, Some(position) == last)
            })?;
            if step == MultiStep::Sync && generate_sysref {
                let count = self.devices.len();
                self.master_mut()
                    .board()
                    .generate_sysref()
                    .map_err(|error| MultiError::Devices {
                        step: MultiStep::MasterSysref,
                        outcomes: outcomes(Vec::new(), error, count),
                    })?;
            }
        }
        Ok(())
    }

    /// Receives one RX packet on each device in turn, the master first, into the front of
    /// its channels' buffers (`channels[i * k .. (i + 1) * k]` for device `i`'s `k`), as
    /// libusdr's aggregate does (`_mstr_stream_recv`), stopping at the first failure.
    /// Returns each device's [`RxMetadata`], where libusdr reports the master's alone.
    ///
    /// Each device waits up to `timeout` in turn, so a call can take the device count
    /// times as long. The devices' packets are not aligned by this call: a device that
    /// failed leaves the ones before it a packet ahead, which their [`RxMetadata::time`]s
    /// show.
    ///
    /// # Errors
    ///
    /// - [`MultiError::Refused`] with [`Error::ChannelBuffers`] for buffers that do not
    ///   match the aggregate's channels.
    /// - [`MultiError::Devices`] at [`MultiStep::Check`] for the refusals of
    ///   [`Device::receive_into`] before any bus access, on any device (no RX stream, the
    ///   wrong host format, a buffer shorter than a packet); no device is touched.
    /// - [`MultiError::Devices`] at [`MultiStep::Call`] with each device's result: as
    ///   [`Device::receive_into`] for the device that failed; the devices before it
    ///   delivered their packets into their buffers.
    pub fn receive_into<S: HostSample>(
        &mut self,
        channels: &mut [&mut [S]],
        timeout: Duration,
    ) -> Result<Vec<RxMetadata>, MultiError<RxMetadata>> {
        let counts = self.channel_counts(|device| device.board().rx_channel_count())?;
        check_buffers(&counts, channels.len())?;
        let mut offset = 0;
        self.check_each(|index, device| {
            let mine = &channels[offset..offset + counts[index]];
            offset += counts[index];
            device.board().check_receive(mine)
        })?;
        let mut rest = channels;
        self.each(MultiStep::Call, |index, device| {
            let (mine, after) = mem::take(&mut rest).split_at_mut(counts[index]);
            rest = after;
            device.receive_into(mine, timeout)
        })
    }

    /// Sends one packet on each device in turn, the master first, from its channels'
    /// buffers (`channels[i * k .. (i + 1) * k]` for device `i`'s `k`), each at `time`
    /// ([`Device::send`]), stopping at the first failure, as libusdr's aggregate does
    /// (`_mstr_stream_send`).
    ///
    /// # Errors
    ///
    /// As [`MultiDevice::send_with_stat`].
    pub fn send<S: HostSample>(
        &mut self,
        channels: &[&[S]],
        time: Option<StreamTime>,
        timeout: Duration,
    ) -> Result<(), MultiError<()>> {
        self.send_with_stat(channels, time, timeout)
            .map(|_| ())
            .map_err(MultiError::into_unit)
    }

    /// [`MultiDevice::send`], returning each device's [`Device::send_with_stat`] report,
    /// where libusdr reports the master's alone.
    ///
    /// # Errors
    ///
    /// - [`MultiError::Refused`] with [`Error::ChannelBuffers`] for buffers that do not
    ///   match the aggregate's channels, and with [`Error::SendSize`] (the first buffer's
    ///   length as `max`) for buffers of different lengths: libusdr's aggregate sends one
    ///   sample count on every device.
    /// - [`MultiError::Devices`] at [`MultiStep::Check`] for the refusals of
    ///   [`Device::send`] before any bus access, on any device (no TX stream, the wrong
    ///   host format, a size or time the stream cannot take); no device is touched.
    /// - [`MultiError::Devices`] at [`MultiStep::Call`] with each device's result: as
    ///   [`Device::send`] for the device that failed; the devices before it took the
    ///   packet, those after it did not.
    pub fn send_with_stat<S: HostSample>(
        &mut self,
        channels: &[&[S]],
        time: Option<StreamTime>,
        timeout: Duration,
    ) -> Result<Vec<Option<TxTelemetry>>, MultiError<Option<TxTelemetry>>> {
        let counts = self.channel_counts(|device| device.board().tx_channel_count())?;
        check_buffers(&counts, channels.len())?;
        // libusdr's `usdr_dms_send` takes one sample count for every channel.
        let first = channels.first().map_or(0, |first| first.len());
        if let Some(other) = channels.iter().find(|other| other.len() != first) {
            return Err(MultiError::Refused(Error::SendSize {
                max: first,
                got: other.len(),
            }));
        }
        let mut offset = 0;
        self.check_each(|index, device| {
            let mine = &channels[offset..offset + counts[index]];
            offset += counts[index];
            device.board().check_send(mine, time)
        })?;
        let mut rest = channels;
        self.each(MultiStep::Call, |index, device| {
            let (mine, after) = rest.split_at(counts[index]);
            rest = after;
            device.send_with_stat(mine, time, timeout)
        })
    }

    /// Stops and destroys the stream group on every device that has one, in libusdr's
    /// aggregate order: the RX stream stopped on each device (`USDR_DMS_STOP`), then the
    /// TX stream on each, then each device's RX stream destroyed (powering its receiver
    /// down), then its TX stream; each device's part as [`Device::stop_streams`] does it.
    /// Every step is attempted on every device, where libusdr stops its streams only up
    /// to the first failure (then destroys them all).
    ///
    /// # Errors
    ///
    /// [`MultiError::Refused`] with [`Error::NotStreaming`] when no device has a group;
    /// [`MultiError::Devices`] at [`MultiStep::Stop`] with each device's first failure.
    pub fn stop_streams(&mut self) -> Result<(), MultiError<()>> {
        if !self
            .devices
            .iter_mut()
            .any(|device| device.board().has_streams())
        {
            return Err(MultiError::Refused(Error::NotStreaming));
        }
        self.master_generates_sysref = false;
        let steps: [StopStep; 4] = [
            |board| board.halt_stream(Direction::Rx),
            |board| board.halt_stream(Direction::Tx),
            |board| board.destroy_stream(Direction::Rx),
            |board| board.destroy_stream(Direction::Tx),
        ];
        let mut results: Vec<Result<(), Error>> = self.devices.iter().map(|_| Ok(())).collect();
        for step in steps {
            for (device, result) in self.devices.iter_mut().zip(&mut results) {
                let stepped = step(device.board());
                if result.is_ok() {
                    *result = stepped;
                }
            }
        }
        let outcomes = results.into_iter().map(Outcome::from).collect();
        MultiError::from_outcomes(MultiStep::Stop, outcomes).map(|_| ())
    }

    /// Closes every device ([`Device::close`]: any stream group stopped, the board powered
    /// down), attempting each, as libusdr destroys the aggregate's devices.
    ///
    /// # Errors
    ///
    /// [`MultiError::Devices`] at [`MultiStep::Close`] with each device's result.
    pub fn close(self) -> Result<(), MultiError<()>> {
        let outcomes: Vec<Outcome<()>> = self
            .devices
            .into_iter()
            .map(|device| Outcome::from(device.close()))
            .collect();
        MultiError::from_outcomes(MultiStep::Close, outcomes).map(|_| ())
    }

    /// Runs `step` on each device in turn, stopping at the first failure.
    fn each<T>(
        &mut self,
        step: MultiStep,
        mut run: impl FnMut(usize, &mut Device) -> Result<T, Error>,
    ) -> Result<Vec<T>, MultiError<T>> {
        let count = self.devices.len();
        let mut done = Vec::with_capacity(count);
        for (index, device) in self.devices.iter_mut().enumerate() {
            match run(index, device) {
                Ok(result) => done.push(result),
                Err(error) => {
                    debug!(%step, device = index, %error, "aggregate step failed");
                    let outcomes = outcomes(done, error, count);
                    return Err(MultiError::Devices { step, outcomes });
                }
            }
        }
        Ok(done)
    }

    /// Runs the pre-I/O `check` on every device ([`MultiStep::Check`]).
    fn check<T>(
        &mut self,
        mut check: impl FnMut(&mut Device) -> Result<(), Error>,
    ) -> Result<(), MultiError<T>> {
        self.check_each(|_, device| check(device))
    }

    /// Runs the pre-I/O `check` on every device, by index ([`MultiStep::Check`]).
    fn check_each<T>(
        &mut self,
        mut check: impl FnMut(usize, &mut Device) -> Result<(), Error>,
    ) -> Result<(), MultiError<T>> {
        let outcomes: Vec<Outcome<T>> = self
            .devices
            .iter_mut()
            .enumerate()
            .map(|(index, device)| match check(index, device) {
                Ok(()) => Outcome::NotReached,
                Err(error) => Outcome::Failed(error),
            })
            .collect();
        if outcomes
            .iter()
            .any(|outcome| matches!(outcome, Outcome::Failed(_)))
        {
            return Err(MultiError::Devices {
                step: MultiStep::Check,
                outcomes,
            });
        }
        Ok(())
    }

    /// Each device's channel count for a stream, by `count`; a device without the stream
    /// fails the check with [`Error::NotStreaming`].
    fn channel_counts<T>(
        &mut self,
        mut count: impl FnMut(&mut Device) -> Option<usize>,
    ) -> Result<Vec<usize>, MultiError<T>> {
        let mut counts = Vec::with_capacity(self.devices.len());
        self.check(|device| {
            counts.push(count(device).ok_or(Error::NotStreaming)?);
            Ok(())
        })?;
        Ok(counts)
    }
}

/// Refuses buffers whose number is not the aggregate's channels, `counts` summed.
fn check_buffers<T>(counts: &[usize], buffers: usize) -> Result<(), MultiError<T>> {
    let expected = counts.iter().sum();
    if buffers == expected {
        Ok(())
    } else {
        Err(MultiError::Refused(Error::ChannelBuffers {
            expected,
            got: buffers,
        }))
    }
}

/// The outcomes of a step that completed on the first devices, with the results `done`,
/// failed on the next with `error`, and never reached the rest of `count`.
fn outcomes<T>(done: Vec<T>, error: Error, count: usize) -> Vec<Outcome<T>> {
    let mut outcomes: Vec<Outcome<T>> = done.into_iter().map(Outcome::Done).collect();
    outcomes.push(Outcome::Failed(error));
    outcomes.resize_with(count, || Outcome::NotReached);
    outcomes
}

impl fmt::Display for MultiStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Open => "open",
            Self::SharedReference => "shared reference",
            Self::Check => "check",
            Self::Call => "call",
            Self::CreateRx => "RX create",
            Self::CreateTx => "TX create",
            Self::StartRx => "RX start",
            Self::StartTx => "TX start",
            Self::Sync => "sync",
            Self::MasterSysref => "master SYSREF",
            Self::Stop => "stop",
            Self::Close => "close",
        })
    }
}

impl MultiStep {
    /// The step a part of a device's start is on the aggregate.
    const fn of(part: StartPart) -> Self {
        match part {
            StartPart::RunRx => Self::StartRx,
            StartPart::StartTx { .. } => Self::StartTx,
            StartPart::Sync(_) => Self::Sync,
        }
    }
}

impl<T> MultiError<T> {
    /// The devices that failed, by index, with their errors; for
    /// [`MultiError::Refused`], none.
    pub fn failures(&self) -> impl Iterator<Item = (usize, &Error)> {
        let outcomes = match self {
            Self::Refused(_) => &[][..],
            Self::Devices { outcomes, .. } => outcomes.as_slice(),
        };
        outcomes
            .iter()
            .enumerate()
            .filter_map(|(index, outcome)| match outcome {
                Outcome::Failed(error) => Some((index, error)),
                Outcome::Done(_) | Outcome::NotReached => None,
            })
    }

    /// `Ok` with each device's result if every one completed, else the step's failure.
    fn from_outcomes(step: MultiStep, outcomes: Vec<Outcome<T>>) -> Result<Vec<T>, Self> {
        if outcomes
            .iter()
            .any(|outcome| !matches!(outcome, Outcome::Done(_)))
        {
            return Err(Self::Devices { step, outcomes });
        }
        Ok(outcomes.into_iter().filter_map(Outcome::done).collect())
    }

    /// The same failure with each completed device's result dropped.
    fn into_unit(self) -> MultiError<()> {
        match self {
            Self::Refused(error) => MultiError::Refused(error),
            Self::Devices { step, outcomes } => MultiError::Devices {
                step,
                outcomes: outcomes.into_iter().map(Outcome::without_result).collect(),
            },
        }
    }
}

impl<T> Outcome<T> {
    /// The result, if the step completed.
    pub fn done(self) -> Option<T> {
        match self {
            Self::Done(result) => Some(result),
            Self::Failed(_) | Self::NotReached => None,
        }
    }

    /// The same outcome with a completed device's result dropped.
    fn without_result(self) -> Outcome<()> {
        match self {
            Self::Done(_) => Outcome::Done(()),
            Self::Failed(error) => Outcome::Failed(error),
            Self::NotReached => Outcome::NotReached,
        }
    }
}

impl<T> From<Result<T, Error>> for Outcome<T> {
    fn from(result: Result<T, Error>) -> Self {
        match result {
            Ok(value) => Self::Done(value),
            Err(error) => Self::Failed(error),
        }
    }
}

impl<T> fmt::Display for MultiError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(error) => write!(f, "refused before any device was reached: {error}"),
            Self::Devices { step, .. } => {
                let failures: Vec<String> = self
                    .failures()
                    .map(|(index, error)| format!("device {index} ({error})"))
                    .collect();
                write!(
                    f,
                    "the aggregate's {step} step failed on {}",
                    failures.join(", ")
                )
            }
        }
    }
}

impl<T: fmt::Debug> StdError for MultiError<T> {
    /// The error of the first device that failed.
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Refused(error) => Some(error),
            Self::Devices { .. } => self
                .failures()
                .next()
                .map(|(_, error)| error as &(dyn StdError + 'static)),
        }
    }
}
