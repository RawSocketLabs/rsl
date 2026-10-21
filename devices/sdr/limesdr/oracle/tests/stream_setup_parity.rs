//! limesdr's stream setup against libusdr's `create_limesdr_stream`, on fresh simulated
//! LimeSDR-minis: for each direction, `host@wire` format and packet size, the outcome, the
//! transport stream libusdr asks for (block size, buffers, stream number, flags), every
//! operation between the open and the close, and the burst's host size from libusdr's own
//! `get_transform_fn` size function.
//!
//! libusdr's calls: `usdr_dms_create` of `/ll/srx/0` or `/ll/stx/0` in `host@wire`; the shim
//! records the transport's stream setup, then refuses it (the FT601 stream pipes are not
//! vendored), so libusdr's success is `-EOPNOTSUPP` after its stream initialisation
//! prepared the LMS7002M. limesdr's: `Device::setup_stream`.
//!
//! Whitelisted divergence: a host format libusdr has no converter for, which it
//! byte-copies, is refused before any exchange
//! (`a_host_libusdr_byte_copies_is_refused_before_any_exchange`).

#[path = "../../tests/common/sim_port.rs"]
mod sim_port;

use std::ffi::CString;

use limesdr::{Device, Direction, Error, SampleFormat, StreamSetup, WireFormat};
use limesdr_oracle::{Oracle, StreamParams, transform_size};
use limesdr_sim::{Op, SimBoard};
use sim_port::SimPort;

/// `EINVAL`.
const EINVAL: i32 = 22;
/// `EOPNOTSUPP`: the shim's refusal of the transport stream.
const EOPNOTSUPP: i32 = 95;
/// A burst's payload: 4096 bytes less the 16-byte header.
const PAYLOAD: u32 = 4080;

/// libusdr's name for a host format.
fn host_name(host: SampleFormat) -> &'static str {
    match host {
        SampleFormat::Ci16 => "ci16",
        SampleFormat::Cf32 => "cf32",
        SampleFormat::Ci12 => "ci12",
        SampleFormat::I16 => "i16",
        SampleFormat::F32 => "f32",
        SampleFormat::I12 => "i12",
        other => panic!("no libusdr name for {other:?}"),
    }
}

/// libusdr's name for a wire format.
fn wire_name(wire: WireFormat) -> &'static str {
    match wire {
        WireFormat::Ci16 => "ci16",
        WireFormat::Ci12 => "ci12",
        other => panic!("no libusdr name for {other:?}"),
    }
}

/// libusdr's outcome, its transport stream and its operations after the open.
fn libusdr(
    direction: Direction,
    wire: WireFormat,
    host: SampleFormat,
    samples: u32,
) -> (i32, Option<StreamParams>, Vec<Op>) {
    let mut oracle = Oracle::open(SimBoard::new()).expect("libusdr opens the board");
    let opened = oracle.trace_len();
    let format = CString::new(format!("{}@{}", host_name(host), wire_name(wire))).expect("no NUL");
    let (errno, params) = oracle.create_stream(direction == Direction::Rx, &format, samples);
    let used = oracle.trace_len();
    let board = oracle.close();
    (errno, params, board.trace()[opened..used].to_vec())
}

/// limesdr's setup and its operations after the open.
fn rsl(
    direction: Direction,
    wire: WireFormat,
    host: SampleFormat,
    samples: u32,
) -> (Result<StreamSetup, Error>, Vec<Op>) {
    let sim = SimPort::new(SimBoard::new());
    let mut device = Device::open(sim.clone()).expect("limesdr opens the board");
    let opened = sim.board().trace().len();
    let setup = device.setup_stream(direction, wire, host, samples);
    let trace = sim.board().trace()[opened..].to_vec();
    (setup, trace)
}

/// Compares one stream setup through both, and the burst's host size with libusdr's size
/// function.
fn compare(direction: Direction, wire: WireFormat, host: SampleFormat, samples: u32) {
    let scenario = format!("{direction:?} {host:?}@{wire:?}, {samples} samples");
    let (errno, params, reference) = libusdr(direction, wire, host, samples);
    let (setup, trace) = rsl(direction, wire, host, samples);
    match setup {
        Ok(setup) => {
            assert_eq!(
                errno, -EOPNOTSUPP,
                "{scenario}: libusdr reached the transport"
            );
            let stream_number = u32::from(direction == Direction::Tx);
            let expected = StreamParams {
                stream_number,
                block_bytes: setup.block_bytes(),
                buffers: setup.buffers(),
                flags: 0,
            };
            assert_eq!(params, Some(expected), "{scenario}: the transport stream");
            let (from, to) = match direction {
                Direction::Rx => (wire_name(wire), host_name(host)),
                Direction::Tx => (host_name(host), wire_name(wire)),
            };
            let from = CString::new(from).expect("no NUL");
            let to = CString::new(to).expect("no NUL");
            let size = transform_size(&from, &to, PAYLOAD, direction == Direction::Tx);
            assert_eq!(
                size,
                Some(setup.host_bytes_per_burst()),
                "{scenario}: the burst's host bytes"
            );
        }
        Err(Error::UnsupportedPacketSize(_)) => {
            assert_eq!(
                (errno, params),
                (-EINVAL, None),
                "{scenario}: refused alike"
            );
        }
        Err(other) => panic!("{scenario}: {other:?}"),
    }
    for (index, (ours, theirs)) in trace.iter().zip(&reference).enumerate() {
        assert_eq!(ours, theirs, "{scenario}: operation {index}");
    }
    assert_eq!(
        trace.len(),
        reference.len(),
        "{scenario}: as many operations"
    );
}

/// The host formats with a converter to and from each wire.
const HOSTS: [SampleFormat; 3] = [SampleFormat::Ci16, SampleFormat::Cf32, SampleFormat::Ci12];

#[test]
fn every_rx_format_and_packet_shape_matches_libusdr() {
    for wire in [WireFormat::Ci16, WireFormat::Ci12] {
        for host in HOSTS {
            // Whole bursts, part bursts below and above one, and none.
            for samples in [0, 1000, 1020, 1360, 4080, 5000, 65_536] {
                compare(Direction::Rx, wire, host, samples);
            }
        }
    }
}

#[test]
fn every_tx_format_default_minimum_and_multiple_matches_libusdr() {
    for wire in [WireFormat::Ci16, WireFormat::Ci12] {
        for host in HOSTS {
            // The default, the minimum, multiples of 4 and 16, and neither.
            for samples in [0, 100, 4096, 4100, 4104, 5440, 32_641, 65_536] {
                compare(Direction::Tx, wire, host, samples);
            }
        }
    }
}

#[test]
fn a_host_libusdr_byte_copies_is_refused_before_any_exchange() {
    for (direction, host) in [
        (Direction::Rx, SampleFormat::F32),
        (Direction::Rx, SampleFormat::I16),
        (Direction::Tx, SampleFormat::I16),
    ] {
        let (errno, params, reference) = libusdr(direction, WireFormat::Ci16, host, 4096);
        assert_eq!(
            errno, -EOPNOTSUPP,
            "libusdr streams {host:?} as copied bytes"
        );
        assert!(
            params.is_some() && !reference.is_empty(),
            "and prepares streaming"
        );
        let (setup, trace) = rsl(direction, WireFormat::Ci16, host, 4096);
        assert!(
            matches!(setup, Err(Error::UnsupportedFormat { .. })),
            "{setup:?}"
        );
        assert!(trace.is_empty(), "refused before any exchange: {trace:?}");
    }
}
