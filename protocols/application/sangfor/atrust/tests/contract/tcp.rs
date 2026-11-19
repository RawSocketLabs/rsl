//! The TCP tunnel (spec §9, §6.6): the route, the request, the handshake and the data phase.

use std::io::{self, Read, Write};
use std::net::Ipv4Addr;

use sangfor_atrust::policy::Policy;
use sangfor_atrust::tcp::{
    Destination, Error, FrameReader, Handshake, Identity, MAX_FRAME_DATA, Mode, Progress, Stream,
    frame,
};
use sangfor_atrust_sig::SignKey;
use sangfor_atrust_wire::tcp::ReplyCode;
use sangfor_atrust_wire::{Endpoint, TunnelRequest};

use crate::support::Node;
use crate::support::fixtures::{DEVICE_ID, V21};

/// The server's answer of vector V16: hello, verdict, and a connect reply with the flag set.
const V16: &[u8] = b"\x05\x81\x53\x00\x00\x0a{\"code\":0}\x05\x00\x01\x01\xc0\x00\x02\x50\x1f\x90";
/// The unsigned request of vector V15.
const V15_UNSIGNED: &str = r#"{"sid":"S1D-example","appId":"app-0001","url":"tcp://app.example.net:443","deviceId":"00112233445566778899aabbccddeeff","connectionId":"B4377F7BABF2991B7D6983C4D3E19CD4-1794787200000000","procHash":"AB49E7CA5E2BED4B3A7D9981456BE57B62CCE2D42C43C881256346D45581AF19","userName":"user01","rcAppliedInfo":0,"lang":"en-US","destAddr":"app.example.net:443","env":{"application":{"runtime":{"process":{"name":"google-chrome-stable","digital_signature":"TrustAppClosed","platform":"Linux","fingerprint":"AB49E7CA5E2BED4B3A7D9981456BE57B62CCE2D42C43C881256346D45581AF19","description":"TrustAppClosed","path":"/usr/bin/google-chrome-stable","version":"TrustAppClosed","security_env":"normal"},"process_trusted":"TRUSTED"}}},"xRequestSig":""}"#;
const V15_SIGNATURE: &str = "ACFDC0F8B848BF3A2583D127E11ABFC8F3DEA0C4EBA11BD3BA3CC816743FACC8";

fn v21() -> Policy {
    Policy::parse(V21.as_bytes(), "sdp.example.org").unwrap()
}

fn ip(text: &str) -> Ipv4Addr {
    text.parse().unwrap()
}

/// A resolver for names the policy must settle by itself.
fn unasked(name: &str) -> Option<Ipv4Addr> {
    panic!("the resolver must not be asked for {name}")
}

/// The identity of the specification's vectors: the shared sid, user and device, set up at
/// the time of V8, signing with the bytes 00 to 1F.
pub(crate) fn identity() -> Identity {
    let key: [u8; 32] = std::array::from_fn(|index| u8::try_from(index).unwrap());
    Identity::with_sign_key(
        "S1D-example",
        "user01",
        DEVICE_ID,
        1_794_787_200_000_000,
        SignKey::from_bytes(key),
    )
    .unwrap()
}

/// The destination of V15: `app.example.net:443` under a name rule of `app-0001` that has
/// the gateway resolve the name.
pub(crate) fn v15_destination() -> Destination {
    let policy = Policy::parse(
        br#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":"app-0001","nodeGroupId":"ng-1",
            "accessModel":"L3VPN","addrPretend":true,
            "addressList":[{"protocol":"tcp","port":"443","host":"app.example.net"}]}]}]}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    Destination::route(&policy, "app.example.net", 443, false, unasked).unwrap()
}

/// The destination message a tunnel would send for `destination` (spec §9.1), raw mode.
fn destination_message(destination: &Destination) -> Vec<u8> {
    TunnelRequest::connect(destination.wire.clone(), false)
        .to_bytes()
        .unwrap()
}

// V26: from a destination to the request's values.
#[test]
fn routing_matches_v26() {
    let policy = v21();

    // A name rule that leaves resolving to the client; the pin supplies the address.
    for host in ["app.example.net", "APP.Example.NET."] {
        let found = Destination::route(&policy, host, 443, false, unasked).unwrap();
        assert_eq!(
            destination_message(&found),
            [0x05, 0x01, 0x00, 0x01, 0xcb, 0x00, 0x71, 0x05, 0x01, 0xbb]
        );
        assert_eq!(found.dest_addr, "app.example.net:443", "{host}");
        assert_eq!(found.dest_ip, Some(ip("203.0.113.5")));
        assert_eq!(
            (found.app_id.as_str(), found.node_group_id.as_str()),
            ("app-0002", "ng-2")
        );
    }

    // The address itself, by the rule the pin made.
    let found = Destination::route(&policy, " 203.0.113.5 ", 443, false, unasked).unwrap();
    assert_eq!(
        destination_message(&found),
        [0x05, 0x01, 0x00, 0x01, 0xcb, 0x00, 0x71, 0x05, 0x01, 0xbb]
    );
    assert_eq!(
        (found.dest_addr.as_str(), found.dest_ip),
        ("203.0.113.5:443", None)
    );
    assert_eq!(found.app_id, "app-0002");

    // A name beneath a plain rule name, with no pin: the caller's resolver answers.
    let resolver = |name: &str| (name == "www.app.example.net").then(|| ip("203.0.113.77"));
    let found = Destination::route(&policy, "www.app.example.net", 443, false, resolver).unwrap();
    assert_eq!(
        destination_message(&found),
        [0x05, 0x01, 0x00, 0x01, 0xcb, 0x00, 0x71, 0x4d, 0x01, 0xbb]
    );
    assert_eq!(found.dest_addr, "www.app.example.net:443");
    assert_eq!(found.dest_ip, Some(ip("203.0.113.77")));
    assert_eq!(found.app_id, "app-0002");

    // The only candidate is the udp rule, and no address rule covers what it resolves to.
    assert_eq!(
        Destination::route(&policy, "x.example.net", 443, false, |_| Some(ip(
            "192.0.2.99"
        ))),
        None
    );

    // The one matching rule prefers L3.
    assert_eq!(
        Destination::route(&policy, "198.51.100.9", 22, false, unasked),
        None
    );
    let found = Destination::route(&policy, "198.51.100.9", 22, true, unasked).unwrap();
    assert_eq!(
        destination_message(&found),
        [0x05, 0x01, 0x00, 0x01, 0xc6, 0x33, 0x64, 0x09, 0x00, 0x16]
    );
    assert_eq!(
        (found.dest_addr.as_str(), found.dest_ip),
        ("198.51.100.9:22", None)
    );
    assert_eq!(
        (found.app_id.as_str(), found.node_group_id.as_str()),
        ("app-0001", "ng-1")
    );

    // IPv6 is never covered, bracketed or not.
    for host in ["[2001:db8::5]", "2001:db8::5", " 2001:db8::5 "] {
        assert_eq!(
            Destination::route(&policy, host, 443, true, unasked),
            None,
            "{host}"
        );
    }
}

// §9.2, destination rules: under a rule that has the gateway resolve, the name goes on the
// wire and the client resolves nothing; a name that resolves nowhere is not covered.
#[test]
fn a_name_goes_on_the_wire_only_when_the_gateway_resolves_it() {
    let found = v15_destination();
    // V14.
    let mut expected = vec![0x05, 0x01, 0x00, 0x03, 0x0f];
    expected.extend(b"app.example.net");
    expected.extend([0x01, 0xbb]);
    assert_eq!(destination_message(&found), expected);
    assert_eq!(
        (found.dest_addr.as_str(), found.dest_ip),
        ("app.example.net:443", None)
    );

    // V21's rule leaves resolving to the client; with no pin and no answer there is no route.
    assert_eq!(
        Destination::route(&v21(), "www.app.example.net", 443, false, |_| None),
        None
    );
    // A name no rule covers that resolves to an address a rule covers goes by the address.
    let found = Destination::route(&v21(), "elsewhere.example.org", 443, false, |_| {
        Some(ip("203.0.113.5"))
    })
    .unwrap();
    assert_eq!(
        (found.dest_addr.as_str(), found.dest_ip),
        ("203.0.113.5:443", None)
    );
    assert!(matches!(found.wire, Endpoint::Ipv4 { .. }));
}

// V15, V14: the first bytes a client sends.
#[test]
fn the_first_flight_matches_v15() {
    let (_, first) = Handshake::new(&identity(), &v15_destination(), false).unwrap();

    assert_eq!(first[..7], [0x05, 0x01, 0x81, 0x53, 0x03, 0x03, 0x1c]);
    let document = std::str::from_utf8(&first[7..7 + 796]).unwrap();
    let signed = V15_UNSIGNED.replace(
        r#""xRequestSig":"""#,
        &format!(r#""xRequestSig":"{V15_SIGNATURE}""#),
    );
    assert_eq!(V15_UNSIGNED.len(), 732);
    assert_eq!(document, signed);

    // The destination message of V14 follows, and nothing after it.
    let mut destination = vec![0x05, 0x01, 0x00, 0x03, 0x0f];
    destination.extend(b"app.example.net");
    destination.extend([0x01, 0xbb]);
    assert_eq!(first[7 + 796..], destination);

    // Zero-RTT changes one byte of it.
    let (_, framed) = Handshake::new(&identity(), &v15_destination(), true).unwrap();
    assert_eq!(framed[7 + 796 + 2], 0x01);
    assert_eq!(framed.len(), first.len());
}

// §9.2: destIP sits between destAddr and env when the client resolved; the user name is
// always present; port 22 is reported as an SSH client (V8's fingerprint).
#[test]
fn the_request_carries_what_the_route_and_the_identity_say() {
    let document = |identity: &Identity, destination: &Destination| {
        let (_, first) = Handshake::new(identity, destination, false).unwrap();
        let length = usize::from(u16::from_be_bytes([first[5], first[6]]));
        String::from_utf8(first[7..7 + length].to_vec()).unwrap()
    };
    let resolved = Destination::route(&v21(), "app.example.net", 443, false, unasked).unwrap();
    let text = document(&identity(), &resolved);
    assert!(
        text.contains(r#""destAddr":"app.example.net:443","destIP":"203.0.113.5","env":{"#),
        "{text}"
    );
    assert!(text.contains(r#""appId":"app-0002","url":"tcp://app.example.net:443""#));

    // V28: an empty user name is sent as an empty string.
    let key = SignKey::from_bytes([7; 32]);
    let nameless = Identity::with_sign_key("S1D-example", "", DEVICE_ID, 0, key).unwrap();
    assert!(document(&nameless, &resolved).contains(r#""userName":"""#));

    let ssh = Destination::route(&v21(), "198.51.100.9", 22, true, unasked).unwrap();
    let text = document(&identity(), &ssh);
    let fingerprint = "D0195B967DFD056E32DA2FDA9C53251423FF5032D7A06AF76050F96433D0DCC3";
    assert!(text.contains(&format!(r#""procHash":"{fingerprint}""#)));
    assert!(text.contains(&format!(r#""name":"ssh","digital_signature":"TrustAppClosed","platform":"Linux","fingerprint":"{fingerprint}""#)));
    assert!(text.contains(r#""path":"/usr/bin/ssh""#));
    assert!(!text.contains("destIP"));

    // The signature is over the document with an empty signature in its place (§8.4).
    let (unsigned, signature) = {
        let at = text.find(r#""xRequestSig":""#).unwrap() + r#""xRequestSig":""#.len();
        (format!(r#"{}"}}"#, &text[..at]), &text[at..at + 64])
    };
    let key: [u8; 32] = std::array::from_fn(|index| u8::try_from(index).unwrap());
    assert!(
        SignKey::from_bytes(key)
            .verify(unsigned.as_bytes(), signature)
            .unwrap()
    );
}

/// A handshake that has sent its first flight, in raw or framed mode.
fn handshake(zero_rtt: bool) -> Handshake {
    Handshake::new(&identity(), &v15_destination(), zero_rtt)
        .unwrap()
        .0
}

// V16: the node's answer, however it is cut into reads.
#[test]
fn the_reply_of_v16_opens_the_tunnel_however_it_arrives() {
    let mut answer = V16.to_vec();
    answer.extend(b"early data");
    let open = |mode| Progress::Open {
        mode,
        early: b"early data".to_vec(),
    };

    // Zero-RTT asked for and the flag set: framed, and closed by a frame.
    let reusable = Mode {
        framed: true,
        reusable: true,
    };
    assert_eq!(handshake(true).receive(&answer).unwrap(), open(reusable));

    // Byte by byte: nothing is decided early and nothing is lost.
    let mut slow = handshake(true);
    for byte in &answer[..V16.len() - 1] {
        assert_eq!(slow.receive(&[*byte]).unwrap(), Progress::NeedMore);
    }
    assert_eq!(
        slow.receive(&answer[V16.len() - 1..]).unwrap(),
        open(reusable)
    );

    // Without zero-RTT the flag means nothing.
    let raw = Mode {
        framed: false,
        reusable: false,
    };
    assert_eq!(handshake(false).receive(&answer).unwrap(), open(raw));

    // Framed without the flag: this end closes by the transport's notice.
    let mut unflagged = V16.to_vec();
    unflagged[V16.len() - 8] = 0x00;
    let framed_only = Mode {
        framed: true,
        reusable: false,
    };
    assert_eq!(
        handshake(true).receive(&unflagged).unwrap(),
        Progress::Open {
            mode: framed_only,
            early: Vec::new()
        }
    );

    // An IPv6 bound address is as good as an IPv4 one.
    let mut v6 = V16[..V16.len() - 7].to_vec();
    v6.push(0x04);
    v6.extend([0; 18]);
    assert!(matches!(
        handshake(false).receive(&v6).unwrap(),
        Progress::Open { .. }
    ));
}

// V27, V16, §9.1: what fails the handshake, and when.
#[test]
fn the_handshake_fails_as_v27_says() {
    let after_hello = |rest: &[u8]| {
        let mut handshake = handshake(false);
        assert_eq!(
            handshake.receive(&[0x05, 0x81]).unwrap(),
            Progress::NeedMore
        );
        handshake.receive(rest)
    };
    assert!(
        matches!(
            after_hello(&[0x53, 0x00, 0x00, 0x00]),
            Err(Error::Protocol(_))
        ),
        "no JSON"
    );
    assert_eq!(
        after_hello(&[0x53, 0x00, 0x00, 0x02, 0x7b, 0x7d]).unwrap(),
        Progress::NeedMore
    );
    let mut ok = vec![0x53, 0x00, 0x00, 0x10];
    ok.extend(br#"{"message":"ok"}"#);
    assert_eq!(after_hello(&ok).unwrap(), Progress::NeedMore);
    assert!(matches!(
        after_hello(&[0x53, 0x00, 0x00, 0x01, 0x78]),
        Err(Error::Protocol(_))
    ));
    // A refusal is two bytes, and nothing more is waited for.
    assert!(matches!(
        after_hello(&[0x53, 0x01]),
        Err(Error::Protocol(_))
    ));
    assert!(matches!(
        after_hello(&[0x54, 0x00]),
        Err(Error::Protocol(_))
    ));

    // V16: a hello that is not `05 81`.
    for hello in [[0x05, 0xd0], [0x05, 0xff], [0x04, 0x81]] {
        assert!(
            matches!(handshake(false).receive(&hello), Err(Error::Protocol(_))),
            "{hello:?}"
        );
    }
}

// §9.1, §4.4: the verdict's code, and the connect result.
#[test]
fn the_verdict_and_the_connect_result_decide() {
    let verdict = |json: &str| {
        let mut answer = vec![0x05, 0x81, 0x53, 0x00];
        answer.extend(u16::try_from(json.len()).unwrap().to_be_bytes());
        answer.extend(json.as_bytes());
        handshake(false).receive(&answer)
    };
    assert_eq!(verdict(r#"{"code":0}"#).unwrap(), Progress::NeedMore);
    for code in [10_000_004, 75_500_002] {
        let found = verdict(&format!(r#"{{"code":{code},"message":"gone"}}"#));
        assert!(matches!(found, Err(Error::SessionInvalid { code: found }) if found == code));
    }
    assert!(matches!(
        verdict(r#"{"code":73600007,"message":"no"}"#),
        Err(Error::Refused { code: 73_600_007, message }) if message == "no"
    ));
    // The message is only for the error text: whatever it is, the code decides.
    assert!(
        matches!(verdict(r#"{"code":1,"message":7}"#), Err(Error::Refused { code: 1, message }) if message.is_empty())
    );
    assert_eq!(
        verdict(r#"{"code":0,"message":"success"}"#).unwrap(),
        Progress::NeedMore
    );
    assert!(matches!(
        verdict(r#"{"code":1,"message":"success"}"#),
        Err(Error::Refused { .. })
    ));

    let result = |code: u8| {
        let mut answer = b"\x05\x81\x53\x00\x00\x02{}".to_vec();
        answer.extend([0x05, code]);
        handshake(false).receive(&answer)
    };
    // After a refusal nothing more is read, so two bytes settle it.
    assert!(matches!(
        result(0x02),
        Err(Error::Connect(ReplyCode::ConnectionNotAllowed))
    ));
    assert!(matches!(
        result(0x05),
        Err(Error::Connect(ReplyCode::ConnectionRefused))
    ));
    assert!(matches!(
        result(0x09),
        Err(Error::Connect(ReplyCode::Other(9)))
    ));
    assert_eq!(result(0x00).unwrap(), Progress::NeedMore);
}

// V17: writing frames.
#[test]
fn frames_are_written_as_v17_says() {
    assert_eq!(
        frame(b"ping"),
        [0x01, 0x00, 0x00, 0x04, 0x70, 0x69, 0x6e, 0x67]
    );
    assert_eq!(frame(b""), [0u8; 0], "a write of nothing sends nothing");

    let long = vec![0x61; 65536];
    let frames = frame(&long);
    assert_eq!(frames.len(), 4 + 65535 + 4 + 1);
    assert_eq!(frames[..4], [0x01, 0x00, 0xff, 0xff]);
    assert_eq!(frames[4 + 65535..], [0x01, 0x00, 0x00, 0x01, 0x61]);
    assert_eq!(MAX_FRAME_DATA, 65535);
}

// V27, V17: reading frames, however they are cut.
#[test]
fn frames_are_read_as_v27_says_however_they_arrive() {
    let wire = [
        0x01, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x02, 0x68, 0x69, 0x01, 0x01, 0x00, 0x00,
    ];
    for piece in 1..=wire.len() {
        let (mut reader, mut stream, mut output) = (FrameReader::new(), Vec::new(), [0u8; 8]);
        for chunk in wire.chunks(piece) {
            let mut chunk = chunk;
            while !chunk.is_empty() && !reader.is_closed() {
                let (used, filled) = reader.read(chunk, &mut output).unwrap();
                assert!(
                    used > 0,
                    "a valid nonempty input must advance with room for output"
                );
                stream.extend_from_slice(&output[..filled]);
                chunk = &chunk[used..];
            }
        }
        assert_eq!(stream, b"hi", "pieces of {piece}");
        assert!(reader.is_closed(), "pieces of {piece}");
    }

    // A close frame is four bytes whatever its length says, and nothing after it is used.
    let mut reader = FrameReader::new();
    let (used, filled) = reader
        .read(
            &[0x01, 0x01, 0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x61],
            &mut [0; 8],
        )
        .unwrap();
    assert_eq!((used, filled, reader.is_closed()), (4, 0, true));
    assert_eq!(
        reader
            .read(&[0x01, 0x00, 0x00, 0x01, 0x61], &mut [0; 8])
            .unwrap(),
        (0, 0)
    );

    // Something that is not a frame breaks the stream for good.
    for bad in [[0x03, 0x00, 0x00, 0x00], [0x01, 0x02, 0x00, 0x00]] {
        let mut reader = FrameReader::new();
        assert!(
            matches!(reader.read(&bad, &mut [0; 8]), Err(Error::Protocol(_))),
            "{bad:?}"
        );
        assert!(matches!(
            reader.read(&[0x01, 0x00, 0x00, 0x00], &mut [0; 8]),
            Err(Error::Protocol(_))
        ));
    }

    // Output smaller than a frame's data: the rest waits for the next call.
    let mut reader = FrameReader::new();
    let mut small = [0u8; 3];
    let wire = frame(b"abcdef");
    assert_eq!(reader.read(&wire, &mut small).unwrap(), (7, 3));
    assert!(!reader.is_between_frames());
    assert_eq!(reader.read(&wire[7..], &mut small).unwrap(), (3, 3));
    assert_eq!(&small, b"def");
    assert!(reader.is_between_frames());
}

/// Open a stream over a node that answers V16 and then sends `pieces`.
fn stream(zero_rtt: bool, pieces: &[&[u8]]) -> Stream<Node> {
    let mut all: Vec<&[u8]> = vec![V16];
    all.extend(pieces);
    Stream::open(
        Node::sending(&all),
        &identity(),
        &v15_destination(),
        zero_rtt,
    )
    .unwrap()
}

fn read_to_end(stream: &mut Stream<Node>) -> io::Result<Vec<u8>> {
    let mut data = Vec::new();
    stream.read_to_end(&mut data).map(|_| data)
}

// §9.1 sequencing, §9.3 raw mode.
#[test]
fn a_raw_stream_is_the_connections_bytes() {
    let (_, first) = Handshake::new(&identity(), &v15_destination(), false).unwrap();
    let mut stream = stream(false, &[b"hello ", b"world"]);
    assert_eq!(
        stream.get_ref().written,
        first,
        "only the first flight is sent before the answer"
    );
    assert_eq!(
        stream.mode(),
        Mode {
            framed: false,
            reusable: false
        }
    );

    stream.write_all(b"ping").unwrap();
    assert_eq!(stream.get_ref().written[first.len()..], *b"ping");
    assert_eq!(read_to_end(&mut stream).unwrap(), b"hello world");

    // Half-close is the transport's notice, sent once.
    stream.close_write().unwrap();
    stream.close_write().unwrap();
    assert_eq!(stream.get_ref().write_closes, 1);
    assert_eq!(
        stream.write(b"more").unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
}

// §9.1: bytes that arrive with the answer are the start of the stream.
#[test]
fn data_that_arrives_with_the_answer_is_delivered_first() {
    let mut with_data = V16.to_vec();
    with_data.extend(b"early ");
    let node = Node::sending(&[&with_data, b"late"]);
    let mut stream = Stream::open(node, &identity(), &v15_destination(), false).unwrap();
    assert_eq!(read_to_end(&mut stream).unwrap(), b"early late");

    let mut framed = V16.to_vec();
    framed.extend(frame(b"early "));
    let node = Node::sending(&[&framed, &frame(b"late"), &[0x01, 0x01, 0x00, 0x00]]);
    let mut stream = Stream::open(node, &identity(), &v15_destination(), true).unwrap();
    assert_eq!(read_to_end(&mut stream).unwrap(), b"early late");
}

// §9.3 framed mode, V17.
#[test]
fn a_framed_stream_frames_what_it_writes_and_unframes_what_it_reads() {
    let (_, first) = Handshake::new(&identity(), &v15_destination(), true).unwrap();
    let hi = frame(b"hi");
    let mut stream = stream(
        true,
        &[
            &hi[..3],
            &hi[3..],
            &[0x01, 0x00, 0x00, 0x00],
            &[0x01, 0x01, 0x00, 0x00],
            b"junk",
        ],
    );
    assert_eq!(
        stream.mode(),
        Mode {
            framed: true,
            reusable: true
        }
    );

    stream.write_all(b"ping").unwrap();
    assert_eq!(
        stream.get_ref().written[first.len()..],
        [0x01, 0x00, 0x00, 0x04, 0x70, 0x69, 0x6e, 0x67]
    );
    assert_eq!(stream.write(b"").unwrap(), 0);
    assert_eq!(
        stream.get_ref().written.len(),
        first.len() + 8,
        "a write of nothing sends nothing"
    );

    // A long write goes out as whole frames of at most 65535 bytes.
    let before = stream.get_ref().written.len();
    stream.write_all(&vec![0x61; 65536]).unwrap();
    let sent = &stream.get_ref().written[before..];
    assert_eq!(sent.len(), 4 + 65535 + 4 + 1);
    assert_eq!(
        (&sent[..4], &sent[4 + 65535..4 + 65535 + 4]),
        (&[0x01, 0x00, 0xff, 0xff][..], &[0x01, 0x00, 0x00, 0x01][..])
    );

    // The close frame ends the stream, and it stays ended.
    assert_eq!(read_to_end(&mut stream).unwrap(), b"hi");
    assert_eq!(stream.read(&mut [0; 8]).unwrap(), 0);

    // Reusable: this end closes with a frame and the connection's write side stays open.
    let before = stream.get_ref().written.len();
    stream.close_write().unwrap();
    stream.close_write().unwrap();
    assert_eq!(stream.get_ref().written[before..], [0x01, 0x01, 0x00, 0x00]);
    assert_eq!(stream.get_ref().write_closes, 0);
    assert_eq!(
        stream.write(b"more").unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
}

// §9.3: without the node's flag a framed stream closes by the transport's notice.
#[test]
fn a_framed_stream_the_node_did_not_flag_closes_by_the_transport() {
    let mut unflagged = V16.to_vec();
    unflagged[V16.len() - 8] = 0x00;
    let node = Node::sending(&[&unflagged]);
    let mut stream = Stream::open(node, &identity(), &v15_destination(), true).unwrap();
    let before = stream.get_ref().written.len();
    stream.close_write().unwrap();
    assert_eq!(stream.get_ref().written.len(), before);
    assert_eq!(stream.get_ref().write_closes, 1);
}

// §9.3 framed reading: where a connection may end.
#[test]
fn a_framed_stream_ends_cleanly_only_between_frames() {
    let hi = frame(b"hi");
    // Between two frames: a clean end.
    assert_eq!(read_to_end(&mut stream(true, &[&hi])).unwrap(), b"hi");
    assert_eq!(read_to_end(&mut stream(true, &[])).unwrap(), b"");
    // Inside a header, inside the data, and right after a header: cut short.
    for cut in [&hi[..2], &hi[..5], &hi[..4]] {
        let found = read_to_end(&mut stream(true, &[cut])).unwrap_err();
        assert_eq!(found.kind(), io::ErrorKind::UnexpectedEof, "{cut:?}");
    }
    // Something that is not a frame.
    let found = read_to_end(&mut stream(true, &[&[0x03, 0x00, 0x00, 0x00]])).unwrap_err();
    assert_eq!(found.kind(), io::ErrorKind::InvalidData);
}

/// A node that answers V16 and whose connection is told to fail in some way.
fn failing_node(configure: impl FnOnce(&mut Node)) -> Node {
    let mut node = Node::sending(&[V16]);
    configure(&mut node);
    node
}

fn first_flight_len(zero_rtt: bool) -> usize {
    Handshake::new(&identity(), &v15_destination(), zero_rtt)
        .unwrap()
        .1
        .len()
}

// §9.3: frames must reach the node whole. A framed write that fails part-way cannot be
// retried: the node may hold half a frame, and anything sent after it would be read as data.
#[test]
fn a_framed_write_that_fails_part_way_ends_the_writing_half() {
    let start = first_flight_len(true);
    let node = failing_node(|node| node.write_limit = Some(start + 2));
    let mut stream = Stream::open(node, &identity(), &v15_destination(), true).unwrap();

    assert_eq!(
        stream.write(b"ping").unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(
        stream.write(b"ping").unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        stream.close_write().unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(
        stream.get_ref().written[start..],
        [0x01, 0x00],
        "nothing follows the cut frame"
    );
}

// §9.3: a half-close that did not go out has not happened. It can be tried again, unless it
// was a close frame cut part-way.
#[test]
fn a_half_close_that_fails_is_not_reported_as_done() {
    // The transport's notice fails once.
    let node = failing_node(|node| node.failing_closes = 1);
    let mut stream = Stream::open(node, &identity(), &v15_destination(), false).unwrap();
    assert_eq!(
        stream.close_write().unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(stream.get_ref().write_closes, 0);
    stream.close_write().unwrap();
    assert_eq!(stream.get_ref().write_closes, 1);

    // A close frame is cut after two bytes.
    let start = first_flight_len(true);
    let node = failing_node(|node| node.write_limit = Some(start + 2));
    let mut stream = Stream::open(node, &identity(), &v15_destination(), true).unwrap();
    assert_eq!(
        stream.close_write().unwrap_err().kind(),
        io::ErrorKind::TimedOut
    );
    assert_eq!(
        stream.close_write().unwrap_err().kind(),
        io::ErrorKind::BrokenPipe
    );
    assert_eq!(stream.get_ref().written[start..], [0x01, 0x01]);
}

// What is written is flushed: the first flight, each frame, and the close frame.
#[test]
fn what_is_sent_is_flushed() {
    let mut stream = stream(true, &[]);
    assert_eq!(
        stream.get_ref().flushed,
        stream.get_ref().written.len(),
        "the first flight"
    );
    stream.write_all(b"ping").unwrap();
    assert_eq!(
        stream.get_ref().flushed,
        stream.get_ref().written.len(),
        "a frame"
    );
    stream.close_write().unwrap();
    assert_eq!(
        stream.get_ref().flushed,
        stream.get_ref().written.len(),
        "the close frame"
    );
}

// §9.3: a write of nothing sends nothing, in raw mode too.
#[test]
fn a_raw_write_of_nothing_does_not_reach_the_transport() {
    let mut stream = stream(false, &[]);
    let before = stream.get_ref().writes;
    assert_eq!(stream.write(b"").unwrap(), 0);
    assert_eq!(stream.get_ref().writes, before);
}

// A read that is interrupted is tried again, during the handshake and after it.
#[test]
fn interrupted_reads_are_retried() {
    let hi = frame(b"hi");
    let mut node = Node::sending(&[V16, &hi, &[0x01, 0x01, 0x00, 0x00]]);
    node.interruptions = 3;
    let mut stream = Stream::open(node, &identity(), &v15_destination(), true).unwrap();
    assert_eq!(read_to_end(&mut stream).unwrap(), b"hi");
}

// §9.3: reusable needs the node's flag to be exactly 1.
#[test]
fn only_the_flag_value_one_makes_a_stream_reusable() {
    for (flag, reusable) in [(0x00, false), (0x01, true), (0x02, false), (0xff, false)] {
        let mut answer = V16.to_vec();
        answer[V16.len() - 8] = flag;
        let Progress::Open { mode, .. } = handshake(true).receive(&answer).unwrap() else {
            panic!("flag {flag:#04x} must not stop the tunnel opening");
        };
        assert_eq!(
            mode,
            Mode {
                framed: true,
                reusable
            },
            "flag {flag:#04x}"
        );
    }
}

// §9.2: the name sent is the normalised queried name, not the rule's text.
#[test]
fn the_name_on_the_wire_is_the_normalised_queried_name() {
    let policy = Policy::parse(
        br#"{"data":{"appList":{"data":{"appInfo":[{"apps":[{"id":"app-0001","accessModel":"L3VPN",
            "addressList":[{"protocol":"tcp","port":"443","host":"*.example.net"}]}]}]}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    let found = Destination::route(&policy, " WWW.Example.NET. ", 443, false, unasked).unwrap();
    assert_eq!(
        found.wire,
        Endpoint::domain(*b"www.example.net", 443).unwrap()
    );
    assert_eq!(found.dest_addr, "www.example.net:443");
}

// §6.6 step 5.1: a name whose rule gives TCP to L3 is not the tunnel's, whatever the address
// rules say; a client with no L3 tunnel takes it by that rule. The resolver, when asked,
// gets the normalised name.
#[test]
fn a_name_rule_that_prefers_l3_does_not_fall_through_to_the_address_rules() {
    let policy = Policy::parse(
        br#"{"data":{"appList":{"data":{"appInfo":[{"apps":[
            {"id":"l3","accessModel":"L3VPN","enableTCPPrefL3":true,"addrPretend":false,
             "addressList":[{"protocol":"tcp","port":"443","host":"db.example.net"}]},
            {"id":"tcp","accessModel":"L3VPN",
             "addressList":[{"protocol":"tcp","port":"443","host":"192.0.2.0/24"}]}]}]}}}}"#,
        "sdp.example.org",
    )
    .unwrap();
    assert_eq!(
        Destination::route(&policy, "db.example.net", 443, false, unasked),
        None
    );

    let resolver = |name: &str| {
        assert_eq!(
            name, "db.example.net",
            "the resolver gets the normalised name"
        );
        Some(ip("192.0.2.7"))
    };
    let found = Destination::route(&policy, "DB.Example.NET.", 443, true, resolver).unwrap();
    assert_eq!(found.app_id, "l3");
    assert_eq!(
        (found.dest_addr.as_str(), found.dest_ip),
        ("db.example.net:443", Some(ip("192.0.2.7")))
    );
}

// §9.2: the sid is read anew for each stream, so one a refresh replaced is the one sent.
#[test]
fn a_replaced_session_identifier_is_the_one_sent() {
    let mut identity = identity();
    identity.set_sid("S1D-renewed");
    let (_, first) = Handshake::new(&identity, &v15_destination(), false).unwrap();
    let text = String::from_utf8_lossy(&first);
    assert!(text.contains(r#"{"sid":"S1D-renewed","appId""#) && !text.contains("S1D-example"));
}

// §9.3: data read before a bad frame is delivered, then the break is reported, however the
// bytes were cut.
#[test]
fn data_before_a_bad_frame_is_delivered_before_the_error() {
    let wire = [&frame(b"hi")[..], &[0x03, 0x00, 0x00, 0x00]].concat();
    for pieces in [vec![&wire[..]], vec![&wire[..6], &wire[6..]]] {
        let mut stream = stream(true, &pieces);
        let mut data = [0u8; 8];
        assert_eq!(stream.read(&mut data).unwrap(), 2);
        assert_eq!(&data[..2], b"hi");
        assert_eq!(
            stream.read(&mut data).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            stream.read(&mut data).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
}
