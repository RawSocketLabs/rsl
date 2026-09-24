//! Wire-format contract: fixed byte vectors for each packet, taken straight
//! from RFC 1350 (and RFC 2347 for OACK), exercised in both directions. A codec
//! regression shows up here as a vector mismatch rather than a subtle drift.

use tftp::wire::{Ack, Data, ErrorCode, ErrorPacket, Packet, Request, RequestKind, TransferMode};
#[cfg(feature = "options")]
use tftp::wire::{OptionAck, TftpOption};

#[test]
fn rrq_vector() {
    // RRQ for "rfc1350.txt" in octet mode.
    let mut expected = vec![0x00, 0x01];
    expected.extend_from_slice(b"rfc1350.txt\0octet\0");
    let req = Request::read("rfc1350.txt", TransferMode::Octet);
    assert_eq!(req.encode(), expected);
    let decoded = Request::decode(&expected).unwrap();
    assert_eq!(decoded.kind, RequestKind::Read);
    assert_eq!(decoded.mode, TransferMode::Octet);
}

#[test]
fn wrq_vector() {
    let mut expected = vec![0x00, 0x02];
    expected.extend_from_slice(b"out.bin\0netascii\0");
    let req = Request::write("out.bin", TransferMode::NetAscii);
    assert_eq!(req.encode(), expected);
}

#[test]
fn data_vector() {
    // DATA block 1 carrying " abc".
    let expected = vec![0x00, 0x03, 0x00, 0x01, b'a', b'b', b'c'];
    let data = Data::new(1, b"abc".to_vec());
    assert_eq!(data.encode(), expected);
    assert_eq!(Data::decode(&expected).unwrap(), data);
}

#[test]
fn ack_vector() {
    let expected = vec![0x00, 0x04, 0x12, 0x34];
    assert_eq!(Ack::new(0x1234).encode(), expected);
    assert_eq!(Ack::decode(&expected).unwrap(), Ack::new(0x1234));
}

#[test]
fn error_vector() {
    // ERROR code 1, "File not found".
    let mut expected = vec![0x00, 0x05, 0x00, 0x01];
    expected.extend_from_slice(b"File not found\0");
    let err = ErrorPacket::new(ErrorCode::FileNotFound, "File not found");
    assert_eq!(err.encode(), expected);
    assert_eq!(ErrorPacket::decode(&expected).unwrap(), err);
}

#[test]
fn packet_enum_round_trips_each_vector() {
    let vectors: &[&[u8]] = &[
        b"\x00\x01file\0octet\0",
        b"\x00\x02file\0octet\0",
        b"\x00\x03\x00\x01data",
        b"\x00\x04\x00\x01",
        b"\x00\x05\x00\x01msg\0",
    ];
    for &bytes in vectors {
        let packet = Packet::decode(bytes).unwrap();
        assert_eq!(packet.encode(), bytes, "round-trip mismatch for {bytes:?}");
    }
}

#[cfg(feature = "options")]
#[test]
fn oack_vector() {
    let mut expected = vec![0x00, 0x06];
    expected.extend_from_slice(b"blksize\x001024\0");
    let oack = OptionAck::new(vec![TftpOption::BlkSize(1024)]);
    assert_eq!(oack.encode(), expected);
    assert_eq!(OptionAck::decode(&expected).unwrap(), oack);
}

#[cfg(feature = "options")]
#[test]
fn rrq_with_options_vector() {
    let mut expected = vec![0x00, 0x01];
    expected.extend_from_slice(b"big\0octet\0blksize\x001432\0tsize\x000\0");
    let req = Request::read("big", TransferMode::Octet)
        .with_options(vec![TftpOption::BlkSize(1432), TftpOption::TSize(0)]);
    assert_eq!(req.encode(), expected);
    assert_eq!(Request::decode(&expected).unwrap(), req);
}
