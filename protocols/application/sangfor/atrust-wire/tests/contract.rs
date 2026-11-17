//! The byte layouts, checked against the vectors of `../../SPEC-atrust.md` §12.

use std::net::{Ipv4Addr, Ipv6Addr};

use sangfor_atrust_wire::{
    AuthReply, Endpoint, MethodReply, Negotiation, TunnelRequest, VipAddresses, VipHead,
    tcp::{ConnectReply, FrameHeader, FrameKind, ReplyCode, ReplyHead},
};

/// Decode one `T` from the front of `wire` and return it with the bytes left over.
macro_rules! take {
    ($type:ty, $wire:expr) => {{
        let (value, consumed) = <$type>::decode_prefix($wire).unwrap();
        (value, &$wire[consumed..])
    }};
}

#[test]
fn session_authentication_request_matches_v12() {
    let mut wire = Negotiation::session(br#"{"sid":"S1D-example"}"#.to_vec())
        .to_bytes()
        .unwrap();
    wire.extend(TunnelRequest::query_vip().to_bytes().unwrap());

    let mut expected = vec![0x05, 0x01, 0xd0, 0x53, 0x00, 0x00, 0x15];
    expected.extend(br#"{"sid":"S1D-example"}"#);
    expected.extend([0x05, 0x04, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
    assert_eq!(wire, expected);
}

#[test]
fn session_authentication_reply_matches_v12() {
    let mut wire = vec![0x05, 0xd0, 0x53, 0x00, 0x00, 0x0a];
    wire.extend(br#"{"code":0}"#);
    wire.extend([0x05, 0x00, 0x00, 0x01, 0xc0, 0x00, 0x02, 0x21, 0x00, 0x00]);

    let (method, rest) = take!(MethodReply, &wire[..]);
    let (auth, rest) = take!(AuthReply, rest);
    let (head, rest) = take!(VipHead, rest);
    let (addresses, rest) = take!(VipAddresses, rest);

    assert_eq!(
        (method.version, method.method),
        (5, Negotiation::METHOD_SESSION)
    );
    assert_eq!((auth.marker, auth.status), (Negotiation::MARKER, 0));
    assert_eq!(auth.json, br#"{"code":0}"#);
    assert_eq!((head.version, head.status), (5, 0));
    assert_eq!(
        addresses,
        VipAddresses::V4 {
            address: Ipv4Addr::new(192, 0, 2, 33),
            trailer: [0, 0]
        }
    );
    assert!(rest.is_empty(), "the four steps are the whole vector");
}

#[test]
fn a_refused_auth_reply_is_consumed_whole() {
    // V12: a non-zero status fails only after all of its JSON has been read.
    let mut wire = vec![0x53, 0x01, 0x00, 0x11];
    wire.extend(br#"{"code":75500002}"#);
    wire.extend([0xaa, 0xbb]);

    let (auth, rest) = take!(AuthReply, &wire[..]);
    assert_eq!(auth.status, 1);
    assert_eq!(auth.json, br#"{"code":75500002}"#);
    assert_eq!(
        rest,
        [0xaa, 0xbb],
        "the stream stays framed after a refusal"
    );
}

#[test]
fn a_refused_vip_query_shows_its_status_without_addresses() {
    let (head, consumed) = VipHead::decode_prefix(&[0x05, 0x01, 0x7f]).unwrap();
    assert_eq!((head.status, head.unused, consumed), (1, 0x7f, 3));
}

#[test]
fn vip_address_forms_have_the_lengths_of_the_spec_table() {
    let v6 = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 0x22);
    let v4 = Ipv4Addr::new(192, 0, 2, 34);
    let trailer = [0x01, 0x02];
    let forms = [
        (
            VipAddresses::V4 {
                address: v4,
                trailer,
            },
            0x01,
            6,
        ),
        (
            VipAddresses::V6 {
                address: v6,
                trailer,
            },
            0x04,
            18,
        ),
        (VipAddresses::Both { v4, v6, trailer }, 0x05, 22),
    ];
    for (addresses, address_type, data_len) in forms {
        let wire = addresses.to_bytes().unwrap();
        assert_eq!(wire[0], address_type);
        assert_eq!(wire.len(), 1 + data_len, "address type {address_type:#04x}");
        assert_eq!(wire[wire.len() - 2..], trailer);
        assert_eq!(VipAddresses::decode_exact(&wire).unwrap(), addresses);
    }

    // The dual form is IPv4, then IPv6, then the trailer.
    let both = VipAddresses::Both { v4, v6, trailer }.to_bytes().unwrap();
    assert_eq!(both[1..5], v4.octets());
    assert_eq!(both[5..21], v6.octets());
}

#[test]
fn tcp_destination_matches_v14() {
    let v4 = Endpoint::addr(Ipv4Addr::new(192, 0, 2, 80), 8080);
    assert_eq!(
        TunnelRequest::connect(v4.clone(), false)
            .to_bytes()
            .unwrap(),
        [0x05, 0x01, 0x00, 0x01, 0xc0, 0x00, 0x02, 0x50, 0x1f, 0x90]
    );
    assert_eq!(
        TunnelRequest::connect(v4, true).to_bytes().unwrap(),
        [0x05, 0x01, 0x01, 0x01, 0xc0, 0x00, 0x02, 0x50, 0x1f, 0x90]
    );

    let v6 = Endpoint::addr(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 5), 443);
    assert_eq!(
        TunnelRequest::connect(v6, false).to_bytes().unwrap(),
        [
            0x05, 0x01, 0x00, 0x04, 0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x05,
            0x01, 0xbb
        ]
    );

    let name = Endpoint::domain(*b"app.example.net", 443).unwrap();
    let mut expected = vec![0x05, 0x01, 0x00, 0x03, 0x0f];
    expected.extend(b"app.example.net");
    expected.extend([0x01, 0xbb]);
    assert_eq!(
        TunnelRequest::connect(name, false).to_bytes().unwrap(),
        expected
    );
}

#[test]
fn tcp_negotiation_header_matches_v15() {
    let wire = Negotiation::tcp(vec![b' '; 796]).to_bytes().unwrap();
    assert_eq!(wire[..7], [0x05, 0x01, 0x81, 0x53, 0x03, 0x03, 0x1c]);
    assert_eq!(wire.len(), 7 + 796);
}

#[test]
fn tcp_reply_matches_v16_and_leaves_the_stream_data() {
    let mut wire = vec![0x05, 0x81, 0x53, 0x00, 0x00, 0x0a];
    wire.extend(br#"{"code":0}"#);
    wire.extend([0x05, 0x00, 0x01, 0x01, 0xc0, 0x00, 0x02, 0x50, 0x1f, 0x90]);
    wire.extend(b"early data");

    let (method, rest) = take!(MethodReply, &wire[..]);
    let (auth, rest) = take!(AuthReply, rest);
    let (head, rest) = take!(ReplyHead, rest);
    let (reply, rest) = take!(ConnectReply, rest);

    assert_eq!(method.method, Negotiation::METHOD_TCP);
    assert_eq!((auth.status, &auth.json[..]), (0, &br#"{"code":0}"#[..]));
    assert_eq!((head.version, head.code), (5, ReplyCode::Succeeded));
    assert_eq!(reply.flag, ConnectReply::FLAG_REUSABLE);
    assert_eq!(
        reply.bound,
        Endpoint::addr(Ipv4Addr::new(192, 0, 2, 80), 8080)
    );
    assert_eq!(
        rest, b"early data",
        "bytes after the reply belong to the stream"
    );
}

#[test]
fn a_refused_connect_shows_its_code_without_the_rest() {
    // §9.1: after a code other than success, nothing more is read.
    let codes = [
        (0x01, ReplyCode::GeneralFailure),
        (0x02, ReplyCode::ConnectionNotAllowed),
        (0x05, ReplyCode::ConnectionRefused),
        (0x09, ReplyCode::Other(9)),
    ];
    for (value, code) in codes {
        let (head, consumed) = ReplyHead::decode_prefix(&[0x05, value]).unwrap();
        assert_eq!((head.code, consumed), (code, 2), "code {value}");
        assert_eq!(head.to_bytes().unwrap(), [0x05, value]);
    }
}

#[test]
fn data_frames_match_v17() {
    assert_eq!(
        FrameHeader::data(4).to_bytes().unwrap(),
        [0x01, 0x00, 0x00, 0x04]
    );
    assert_eq!(
        FrameHeader::data(0xffff).to_bytes().unwrap(),
        [0x01, 0x00, 0xff, 0xff]
    );
    assert_eq!(
        FrameHeader::data(1).to_bytes().unwrap(),
        [0x01, 0x00, 0x00, 0x01]
    );
    assert_eq!(
        FrameHeader::close().to_bytes().unwrap(),
        [0x01, 0x01, 0x00, 0x00]
    );
    assert_eq!(
        FrameHeader::close().to_bytes().unwrap().len(),
        FrameHeader::LEN
    );
}

#[test]
fn a_close_frame_is_recognised_whatever_its_length_says() {
    let header = FrameHeader::decode_exact(&[0x01, 0x01, 0x12, 0x34]).unwrap();
    assert_eq!(header.kind, FrameKind::Close);
    assert_eq!(header.length, 0x1234);
}
