//! Input a peer controls, and values the format cannot carry.

use sangfor_atrust_wire::{
    AuthReply, Endpoint, MethodReply, Negotiation, TunnelRequest, VipAddresses, VipHead,
    tcp::{ConnectReply, FrameHeader, FrameKind, ReplyHead},
};

/// The auth reply and the connect reply of vector V16.
const AUTH_REPLY: &[u8] = b"\x53\x00\x00\x0a{\"code\":0}";
const CONNECT_REPLY: &[u8] = &[0x01, 0x01, 0xc0, 0x00, 0x02, 0x50, 0x1f, 0x90];
/// The addresses of vector V12.
const VIP_ADDRESSES: &[u8] = &[0x01, 0xc0, 0x00, 0x02, 0x21, 0x00, 0x00];

/// Whether a decode stopped for want of input, which is the signal to read more and retry.
fn incomplete<T>(result: Result<T, bnb::BitError>) -> bool {
    result.is_err_and(|error| error.is_incomplete())
}

/// Check, for input a peer chose, that reading `$type` from a growing buffer behaves: every
/// answer before the first settled one is a request for more, the settled answer does not
/// change as more arrives, and a decoded value encodes back to exactly the bytes it took.
macro_rules! settles_once {
    ($type:ty, $wire:expr) => {{
        let wire: &[u8] = $wire;
        let settled =
            (0..=wire.len()).find(|&end| !incomplete(<$type>::decode_prefix(&wire[..end])));
        if let Some(end) = settled {
            let first = <$type>::decode_prefix(&wire[..end]);
            let whole = <$type>::decode_prefix(wire);
            match (first, whole) {
                (Ok((value, consumed)), Ok((again, consumed_again))) => {
                    assert_eq!(
                        consumed,
                        end,
                        "{}: decoded before its last byte",
                        stringify!($type)
                    );
                    assert_eq!((&value, consumed), (&again, consumed_again));
                    assert_eq!(value.to_bytes().unwrap(), wire[..consumed]);
                }
                (Err(_), Err(_)) => {}
                _ => panic!("{}: the answer changed as input arrived", stringify!($type)),
            }
        }
    }};
}

#[test]
fn generated_input_settles_once_and_round_trips() {
    // A fixed linear congruential sequence: the test is deterministic.
    let mut state = 0x2545_f491_4f6c_dd1d_u64;
    let mut next = || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        state.to_be_bytes()[0]
    };
    for _ in 0..2000 {
        let mut wire: Vec<u8> = (0..40).map(|_| next()).collect();
        // Small values in the bytes that select a layout or a length, so that most inputs
        // reach past the first field instead of failing or starving at once.
        for index in [0, 1, 2, 3, 5] {
            wire[index] %= 8;
        }
        settles_once!(Negotiation, &wire);
        settles_once!(TunnelRequest, &wire);
        settles_once!(MethodReply, &wire);
        settles_once!(AuthReply, &wire);
        settles_once!(VipHead, &wire);
        settles_once!(VipAddresses, &wire);
        settles_once!(ReplyHead, &wire);
        settles_once!(ConnectReply, &wire);
        settles_once!(FrameHeader, &wire);
    }
}

#[test]
fn every_truncated_message_asks_for_more_instead_of_decoding() {
    for end in 0..AUTH_REPLY.len() {
        assert!(
            incomplete(AuthReply::decode_prefix(&AUTH_REPLY[..end])),
            "auth reply cut at {end}"
        );
    }
    for end in 0..CONNECT_REPLY.len() {
        assert!(
            incomplete(ConnectReply::decode_prefix(&CONNECT_REPLY[..end])),
            "connect reply cut at {end}"
        );
    }
    for end in 0..VIP_ADDRESSES.len() {
        assert!(
            incomplete(VipAddresses::decode_prefix(&VIP_ADDRESSES[..end])),
            "addresses cut at {end}"
        );
    }
    for end in 0..3 {
        assert!(incomplete(VipHead::decode_prefix(
            &[0x05, 0x00, 0x00][..end]
        )));
    }
    for end in 0..2 {
        assert!(incomplete(MethodReply::decode_prefix(&[0x05, 0x81][..end])));
        assert!(incomplete(ReplyHead::decode_prefix(&[0x05, 0x00][..end])));
    }
    for end in 0..FrameHeader::LEN {
        assert!(incomplete(FrameHeader::decode_prefix(
            &[0x01, 0x00, 0x00, 0x04][..end]
        )));
    }
}

#[test]
fn a_json_length_larger_than_the_input_is_not_a_message() {
    // Claims 65535 bytes of JSON and supplies two.
    let wire = [0x53, 0x00, 0xff, 0xff, b'{', b'}'];
    assert!(incomplete(AuthReply::decode_prefix(&wire)));
    assert!(AuthReply::decode_exact(&wire).is_err());
}

#[test]
fn an_empty_json_document_is_a_complete_auth_reply() {
    let reply = AuthReply::decode_exact(&[0x53, 0x07, 0x00, 0x00]).unwrap();
    assert_eq!((reply.status, reply.json.len()), (7, 0));
}

#[test]
fn address_types_without_a_defined_length_are_rejected_at_once() {
    // The addresses know types 1, 4 and 5; 3 has no defined length there.
    let mut vip = VIP_ADDRESSES.to_vec();
    vip[0] = 0x03;
    assert!(VipAddresses::decode_exact(&vip).is_err());

    let mut connect = CONNECT_REPLY.to_vec();
    connect[1] = 0x05;
    assert!(ConnectReply::decode_exact(&connect).is_err());

    // Definitive, not a request for more input: waiting would never help.
    assert!(!incomplete(VipAddresses::decode_prefix(&vip[..1])));
    assert!(!incomplete(ConnectReply::decode_prefix(&connect[..2])));
}

#[test]
fn bytes_after_a_whole_message_are_not_swallowed() {
    let mut vip = VIP_ADDRESSES.to_vec();
    vip.push(0);
    assert!(VipAddresses::decode_exact(&vip).is_err());
    let (_, consumed) = VipAddresses::decode_prefix(&vip).unwrap();
    assert_eq!(consumed, VIP_ADDRESSES.len());
}

#[test]
fn unexpected_constants_decode_and_round_trip_verbatim() {
    // Nothing here is unencodable, so the codec keeps it; the client decides what to refuse.
    // One off-nominal value in every stored field of every type.
    let negotiation = [0x06, 0x01, 0xff, 0x54, 0x09, 0x00, 0x01, b'x'];
    assert_eq!(
        Negotiation::decode_exact(&negotiation)
            .unwrap()
            .to_bytes()
            .unwrap(),
        negotiation
    );

    let request = [0x04, 0x09, 0xff, 0x01, 127, 0, 0, 1, 0x00, 0x50];
    let decoded = TunnelRequest::decode_exact(&request).unwrap();
    assert_eq!(
        (decoded.version, decoded.command, decoded.flag),
        (4, 9, 0xff)
    );
    assert_eq!(decoded.to_bytes().unwrap(), request);

    let method = MethodReply::decode_exact(&[0x04, 0xff]).unwrap();
    assert_eq!((method.version, method.method), (4, 0xff));

    let auth = AuthReply::decode_exact(&[0x54, 0xff, 0x00, 0x00]).unwrap();
    assert_eq!((auth.marker, auth.status), (0x54, 0xff));

    let vip = VipHead::decode_exact(&[0x04, 0xff, 0x7f]).unwrap();
    assert_eq!((vip.version, vip.status, vip.unused), (4, 0xff, 0x7f));
    assert_eq!(vip.to_bytes().unwrap(), [0x04, 0xff, 0x7f]);

    let head = ReplyHead::decode_exact(&[0x04, 0x00]).unwrap();
    assert_eq!(head.version, 4);

    let connect = [0xff, 0x01, 127, 0, 0, 1, 0x00, 0x50];
    let decoded = ConnectReply::decode_exact(&connect).unwrap();
    assert_eq!(decoded.flag, 0xff);
    assert_eq!(decoded.to_bytes().unwrap(), connect);

    let frame = FrameHeader::decode_exact(&[0x02, 0x09, 0x00, 0x01]).unwrap();
    assert_eq!((frame.marker, frame.kind), (2, FrameKind::Other(9)));
    assert_eq!(frame.to_bytes().unwrap(), [0x02, 0x09, 0x00, 0x01]);
}

#[test]
fn the_method_count_says_how_many_methods_follow() {
    // Two methods offered: the marker is the byte after both, not the second method.
    let wire = [0x05, 0x02, 0xd0, 0x81, 0x53, 0x00, 0x00, 0x00];
    let negotiation = Negotiation::decode_exact(&wire).unwrap();
    assert_eq!(negotiation.methods, [0xd0, 0x81]);
    assert_eq!(negotiation.marker, Negotiation::MARKER);
    assert_eq!(negotiation.to_bytes().unwrap(), wire);
}

#[test]
fn a_bound_name_decodes_for_the_client_to_refuse() {
    // §9.1 allows only address types 1 and 4 here. A name has the SOCKS5 layout, so it is
    // not unencodable; the codec keeps it and the client refuses it.
    let wire = [0x01, 0x03, 0x01, b'a', 0x00, 0x50];
    let reply = ConnectReply::decode_exact(&wire).unwrap();
    assert_eq!(reply.bound, Endpoint::domain(*b"a", 80).unwrap());
}

#[test]
fn a_json_document_too_long_for_its_length_field_does_not_encode() {
    assert!(Negotiation::tcp(vec![b' '; 0xffff]).to_bytes().is_ok());
    assert!(Negotiation::tcp(vec![b' '; 0x1_0000]).to_bytes().is_err());
}

#[test]
fn a_name_too_long_for_its_length_byte_does_not_encode() {
    // The reference writes such a name with a wrapped length byte, which no node can parse.
    assert!(Endpoint::domain(vec![b'a'; 256], 443).is_err());
    let raw = Endpoint::domain_raw(vec![b'a'; 256], 443);
    assert!(TunnelRequest::connect(raw, false).to_bytes().is_err());
}
