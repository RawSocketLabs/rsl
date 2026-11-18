//! A TCP tunnel (spec §9) opened to a node that sends whatever it likes: opening, reading and closing
//! must never panic, and how the node's bytes are cut into reads must change nothing: not
//! whether the tunnel opens, not its mode, not the data delivered, not how it ends. What is
//! written goes out as itself in raw mode, and in framed mode as frames that read back as it.
//!
//! The first byte picks the mode asked for, the read sizes, and how much of a valid answer
//! (vector V16) precedes the input, so the data phase is reached without rediscovering it.
//!
//! Run: `cargo +nightly fuzz run tunnel --fuzz-dir protocols/application/sangfor/fuzz`.
#![no_main]

use std::io::{self, Read, Write};
use std::mem::{Discriminant, discriminant};
use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use sangfor_atrust::policy::Policy;
use sangfor_atrust::tcp::{
    Destination, Error, FrameReader, Handshake, Identity, Mode, Stream, Transport,
};
use sangfor_atrust_sig::SignKey;

/// The node's answer of vector V16: method, authentication and connect replies.
const V16: &[u8] = b"\x05\x81\x53\x00\x00\x0a{\"code\":0}\x05\x00\x01\x01\xc0\x00\x02\x50\x1f\x90";
/// The start of [`V16`] and where each of its four parts ends: the method reply, the
/// authentication reply, the head of the connect reply, and its bound address.
const PREFIXES: [usize; 5] = [0, 2, 16, 18, V16.len()];

/// The identity of the specification's vectors, and the destination of vector V15.
static CLIENT: LazyLock<(Identity, Destination)> = LazyLock::new(|| {
    let key: [u8; 32] = std::array::from_fn(|index| u8::try_from(index).unwrap());
    let identity = Identity::with_sign_key(
        "S1D-example",
        "user01",
        "00112233445566778899aabbccddeeff",
        1_794_787_200_000_000,
        SignKey::from_bytes(key),
    )
    .unwrap();
    let policy = Policy::parse(
        br#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":"app-0001","nodeGroupId":"ng-1",
            "accessModel":"L3VPN","addrPretend":true,
            "addressList":[{"protocol":"tcp","port":"443","host":"app.example.net"}]}]}]}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    let destination = Destination::route(&policy, "app.example.net", 443, false, |_| None).unwrap();
    (identity, destination)
});

/// A node that sends `input`, at most `chunk` bytes a read, and keeps what it is sent.
struct Node<'a> {
    input: &'a [u8],
    chunk: usize,
    written: Vec<u8>,
}

impl Read for Node<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = buffer.len().min(self.chunk).min(self.input.len());
        let (now, later) = self.input.split_at(count);
        buffer[..count].copy_from_slice(now);
        self.input = later;
        Ok(count)
    }
}

impl Write for Node<'_> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.written.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Transport for Node<'_> {
    fn close_write(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Everything a caller can observe of one tunnel.
#[derive(Debug, PartialEq)]
struct Outcome {
    opened: Result<Mode, Discriminant<Error>>,
    delivered: Vec<u8>,
    ended: Option<io::ErrorKind>,
    sent: Vec<u8>,
}

/// Open a tunnel to a node that sends `input` in reads of `chunk`, write `payload`, read to
/// the end `room` bytes at a time, and close.
fn run(input: &[u8], chunk: usize, room: usize, zero_rtt: bool, payload: &[u8]) -> Outcome {
    let (identity, destination) = &*CLIENT;
    let (_, first) = Handshake::new(identity, destination, zero_rtt).unwrap();

    let node = Node {
        input,
        chunk,
        written: Vec::new(),
    };
    let mut stream = match Stream::open(node, identity, destination, zero_rtt) {
        Ok(stream) => stream,
        Err(error) => {
            return Outcome {
                opened: Err(discriminant(&error)),
                delivered: Vec::new(),
                ended: None,
                sent: Vec::new(),
            };
        }
    };
    let mode = stream.mode();

    stream.write_all(payload).unwrap();
    stream.flush().unwrap();
    let sent = stream
        .get_ref()
        .written
        .strip_prefix(first.as_slice())
        .expect("the first flight goes out first, unchanged");
    if mode.framed {
        let (mut reader, mut carried) = (FrameReader::new(), vec![0; payload.len()]);
        let (used, filled) = reader.read(sent, &mut carried).unwrap();
        assert_eq!(
            (used, &carried[..filled], reader.is_between_frames()),
            (sent.len(), payload, true),
            "what was written must go out as whole frames that carry it"
        );
    } else {
        assert_eq!(sent, payload, "what was written must go out as it is");
    }

    let (mut delivered, mut buffer) = (Vec::new(), vec![0; room]);
    let ended = loop {
        match stream.read(&mut buffer) {
            Ok(0) => break None,
            Ok(count) => delivered.extend_from_slice(&buffer[..count]),
            Err(error) => break Some(error.kind()),
        }
    };
    stream.close_write().unwrap();
    Outcome {
        opened: Ok(mode),
        delivered,
        ended,
        sent: stream.get_ref().written.clone(),
    }
}

fuzz_target!(|data: &[u8]| {
    let Some((&selector, rest)) = data.split_first() else {
        return;
    };
    let zero_rtt = selector & 1 != 0;
    let chunk = [1, 2, 7, 513][usize::from(selector >> 1 & 3)];
    let room = [1, 3, 64, 4096][usize::from(selector >> 3 & 3)];
    let prefix = &V16[..PREFIXES[usize::from(selector >> 5) % PREFIXES.len()]];
    let input = [prefix, rest].concat();

    let pieces = run(&input, chunk, room, zero_rtt, rest);
    let whole = run(&input, usize::MAX, 4096, zero_rtt, rest);
    assert_eq!(
        pieces, whole,
        "how the node's bytes are cut into reads changed the tunnel"
    );
    if prefix.len() == V16.len() {
        assert!(whole.opened.is_ok(), "the answer of V16 opens the tunnel");
    }
});
