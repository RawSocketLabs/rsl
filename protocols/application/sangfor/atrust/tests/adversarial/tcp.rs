//! Nodes that answer with something else than the protocol, or stop answering.

use std::io::{self, Read};

use sangfor_atrust::policy::Policy;
use sangfor_atrust::tcp::{
    Destination, Error, FrameReader, Handshake, Identity, Progress, Stream, frame,
};
use sangfor_atrust_sig::SignKey;

use crate::support::Node;
use crate::support::fixtures::DEVICE_ID;

fn identity() -> Identity {
    Identity::with_sign_key(
        "S1D-example",
        "user01",
        DEVICE_ID,
        0,
        SignKey::from_bytes([7; 32]),
    )
    .unwrap()
}

fn policy() -> Policy {
    Policy::parse(
        br#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":"app-0001","accessModel":"L3VPN",
            "addressList":[{"protocol":"tcp","port":"1-65535","host":"*.example.net"},
                           {"protocol":"tcp","port":"1-65535","host":"192.0.2.0/24"}]}]}]}}}}"#,
        "sdp.example.org",
    )
    .unwrap()
}

fn destination() -> Destination {
    Destination::route(&policy(), "192.0.2.80", 8080, false, |_| None).unwrap()
}

fn handshake() -> Handshake {
    Handshake::new(&identity(), &destination(), true).unwrap().0
}

/// The answer up to and including an accepting verdict.
const ACCEPTED: &[u8] = b"\x05\x81\x53\x00\x00\x02{}";

#[test]
fn a_verdict_that_is_not_an_object_with_an_integer_code_fails() {
    for json in [
        "null",
        "[]",
        "7",
        r#""ok""#,
        r#"{"code":null}"#,
        r#"{"code":"0"}"#,
        r#"{"code":0.5}"#,
        r#"{"code":[0]}"#,
        r#"{"code":0"#,
        "\u{feff}{}",
    ] {
        let mut answer = vec![0x05, 0x81, 0x53, 0x00];
        answer.extend(u16::try_from(json.len()).unwrap().to_be_bytes());
        answer.extend(json.as_bytes());
        assert!(
            matches!(handshake().receive(&answer), Err(Error::Protocol(_))),
            "{json}"
        );
    }
}

#[test]
fn a_connect_reply_with_a_name_or_an_unknown_address_type_fails() {
    for bound in [
        &[0x05, 0x00, 0x01, 0x03, 0x01, b'a', 0x00, 0x50][..],
        &[0x05, 0x00, 0x01, 0x05, 0, 0, 0, 0, 0, 0],
    ] {
        let answer = [ACCEPTED, bound].concat();
        assert!(
            matches!(handshake().receive(&answer), Err(Error::Protocol(_))),
            "{bound:?}"
        );
    }
}

#[test]
fn a_failed_handshake_stays_failed() {
    let mut handshake = handshake();
    assert!(matches!(
        handshake.receive(&[0x05, 0x00]),
        Err(Error::Protocol(_))
    ));
    // Even a perfect answer does not revive it.
    let good = [
        ACCEPTED,
        &[0x05, 0x00, 0x00, 0x01, 192, 0, 2, 80, 0x1f, 0x90],
    ]
    .concat();
    assert!(matches!(handshake.receive(&good), Err(Error::Protocol(_))));

    // And an opened one takes nothing more.
    let mut opened = self::handshake();
    assert!(matches!(
        opened.receive(&good).unwrap(),
        Progress::Open { .. }
    ));
    assert!(matches!(opened.receive(b"data"), Err(Error::Protocol(_))));
}

#[test]
fn a_verdict_that_never_arrives_is_waited_for_not_guessed() {
    let mut handshake = handshake();
    // Claims 65535 bytes of JSON and sends a few.
    let start = b"\x05\x81\x53\x00\xff\xff{\"code\":0}";
    assert_eq!(handshake.receive(start).unwrap(), Progress::NeedMore);
    assert_eq!(
        handshake.receive(&[b' '; 1000]).unwrap(),
        Progress::NeedMore
    );
}

#[test]
fn a_request_too_long_for_its_length_fields_is_refused_before_anything_is_sent() {
    let long = "s".repeat(70_000);
    let identity =
        Identity::with_sign_key(long, "user01", DEVICE_ID, 0, SignKey::from_bytes([7; 32]))
            .unwrap();
    assert!(matches!(
        Handshake::new(&identity, &destination(), false),
        Err(Error::TooLong)
    ));
    let node = Node::default();
    assert!(matches!(
        Stream::open(node, &identity, &destination(), false),
        Err(Error::TooLong)
    ));

    // A name its length byte cannot hold is no destination at all.
    let name = format!("{}.example.net", "a".repeat(250));
    assert_eq!(
        Destination::route(&policy(), &name, 443, false, |_| None),
        None
    );
    let name = format!("{}.example.net", "a".repeat(200));
    assert!(Destination::route(&policy(), &name, 443, false, |_| None).is_some());
}

#[test]
fn a_node_that_hangs_up_during_the_handshake_is_an_error() {
    for pieces in [
        &[][..],
        &[&ACCEPTED[..1]],
        &[ACCEPTED],
        &[ACCEPTED, &[0x05, 0x00, 0x01]],
    ] {
        let found =
            Stream::open(Node::sending(pieces), &identity(), &destination(), true).unwrap_err();
        assert!(
            matches!(&found, Error::Io(error) if error.kind() == io::ErrorKind::UnexpectedEof),
            "{found:?}"
        );
    }
    // A refusal is reported as what it is, not as the hang-up that follows it.
    let refused = [&ACCEPTED[..2], &[0x53, 0x01]].concat();
    let found = Stream::open(
        Node::sending(&[&refused]),
        &identity(),
        &destination(),
        true,
    )
    .unwrap_err();
    assert!(matches!(found, Error::Protocol(_)));
}

#[test]
fn a_read_into_no_room_takes_nothing() {
    let good = [
        ACCEPTED,
        &[0x05, 0x00, 0x01, 0x01, 192, 0, 2, 80, 0x1f, 0x90],
        &frame(b"hi"),
    ]
    .concat();
    let mut stream =
        Stream::open(Node::sending(&[&good]), &identity(), &destination(), true).unwrap();
    assert_eq!(stream.read(&mut []).unwrap(), 0);
    let mut data = Vec::new();
    stream.read_to_end(&mut data).unwrap();
    assert_eq!(data, b"hi");
}

#[test]
fn generated_input_never_panics_and_frames_read_the_same_however_cut() {
    // A fixed linear congruential sequence: the test is deterministic.
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        state.to_be_bytes()[0]
    };
    for _ in 0..500 {
        // Arbitrary bytes after a valid start: any outcome but a panic.
        let noise: Vec<u8> = (0..48).map(|_| next() % 8).collect();
        let _ = handshake().receive(&[&ACCEPTED[..usize::from(next() % 9)], &noise].concat());

        // A stream of frames with arbitrary data, some of them empty, then a close frame.
        let mut stream = Vec::new();
        let mut wire = Vec::new();
        for _ in 0..(next() % 5) {
            let data: Vec<u8> = (0..(next() % 40)).map(|_| next()).collect();
            stream.extend_from_slice(&data);
            // An empty write makes no frame, so an empty frame is put there by hand.
            wire.extend(if data.is_empty() {
                vec![0x01, 0x00, 0x00, 0x00]
            } else {
                frame(&data)
            });
        }
        wire.extend([0x01, 0x01, 0x00, 0x00]);

        let piece = usize::from(next() % 7) + 1;
        let room = usize::from(next() % 5) + 1;
        let (mut reader, mut read) = (FrameReader::new(), Vec::new());
        for chunk in wire.chunks(piece) {
            let mut chunk = chunk;
            while !chunk.is_empty() && !reader.is_closed() {
                let mut output = vec![0; room];
                let (used, filled) = reader.read(chunk, &mut output).unwrap();
                assert!(used > 0, "no progress with input and room to spare");
                read.extend_from_slice(&output[..filled]);
                chunk = &chunk[used..];
            }
        }
        assert_eq!(read, stream, "pieces of {piece}, room for {room}");
        assert!(reader.is_closed());
    }
}

#[test]
fn an_identity_does_not_print_what_authorises_a_stream() {
    let shown = format!("{:?}", identity());
    assert!(!shown.contains("S1D-example"), "{shown}");
    assert!(shown.contains("user01"));
}

#[test]
fn a_bound_name_is_refused_at_its_type_byte() {
    // Not waited for: the name's length byte could promise 255 bytes that never come.
    let answer = [ACCEPTED, &[0x05, 0x00, 0x01, 0x03]].concat();
    assert!(matches!(
        handshake().receive(&answer),
        Err(Error::Protocol(_))
    ));
}

#[test]
fn a_handshake_that_is_over_keeps_nothing_it_is_given() {
    let mut handshake = handshake();
    assert!(handshake.receive(&[0x05, 0x00]).is_err());
    for _ in 0..100 {
        assert!(handshake.receive(&[0u8; 10_000]).is_err());
    }
    assert!(
        format!("{handshake:?}").len() < 200,
        "nothing fed after the end is held"
    );
}
