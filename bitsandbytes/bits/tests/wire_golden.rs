//! Golden-vector tests: `#[wire]` must reproduce real on-the-wire bytes.
//!
//! A faithful DNS header (RFC 1035 §4.1.1) with the flags word modeled as an
//! 8-member bit-group (QR, Opcode, AA, TC, RD, RA, Z, RCODE = 1+4+1+1+1+1+3+4 =
//! 16 bits). These are byte strings you can recognize on the wire / in a capture.
#![cfg(feature = "binrw")]

use binrw::{BinRead, BinWrite};
use bits::{u1, u3, u4, wire};
use std::io::Cursor;

#[wire(big, group(qr, opcode, aa, tc, rd, ra, z, rcode => u16))]
#[derive(Debug, Clone, PartialEq)]
struct DnsHeader {
    id: u16,
    qr: u1,
    opcode: u4,
    aa: u1,
    tc: u1,
    rd: u1,
    ra: u1,
    z: u3,
    rcode: u4,
    qdcount: u16,
    ancount: u16,
    nscount: u16,
    arcount: u16,
}

fn check(h: &DnsHeader, golden: &[u8]) {
    let mut buf = Cursor::new(Vec::new());
    h.write(&mut buf).unwrap();
    assert_eq!(buf.get_ref().as_slice(), golden, "encoded bytes mismatch");
    let back = DnsHeader::read(&mut Cursor::new(golden)).unwrap();
    assert_eq!(&back, h, "decoded value mismatch");
}

#[test]
fn dns_standard_query() {
    // id=0x1234, standard recursive query: RD=1, everything else 0; QDCOUNT=1.
    // flags word = 0x0100 (RD is bit 8).
    let h = DnsHeader {
        id: 0x1234,
        qr: u1::new(0),
        opcode: u4::new(0),
        aa: u1::new(0),
        tc: u1::new(0),
        rd: u1::new(1),
        ra: u1::new(0),
        z: u3::new(0),
        rcode: u4::new(0),
        qdcount: 1,
        ancount: 0,
        nscount: 0,
        arcount: 0,
    };
    check(
        &h,
        &[
            0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
    );
}

#[test]
fn dns_authoritative_response_nxdomain() {
    // id=0x8765, response (QR=1), authoritative (AA=1), RD=1, RA=1, RCODE=3
    // (NXDOMAIN); QDCOUNT=1, ANCOUNT=1.
    // flags word = 0x8000|0x0400|0x0100|0x0080|0x0003 = 0x8583.
    let h = DnsHeader {
        id: 0x8765,
        qr: u1::new(1),
        opcode: u4::new(0),
        aa: u1::new(1),
        tc: u1::new(0),
        rd: u1::new(1),
        ra: u1::new(1),
        z: u3::new(0),
        rcode: u4::new(3),
        qdcount: 1,
        ancount: 1,
        nscount: 0,
        arcount: 0,
    };
    check(
        &h,
        &[
            0x87, 0x65, 0x85, 0x83, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
        ],
    );
}

#[test]
fn dns_inverse_query_opcode_in_high_bits() {
    // Opcode=1 (IQUERY) occupies bits 14..11, i.e. 0x0800 in the flags word.
    let h = DnsHeader {
        id: 0x0001,
        qr: u1::new(0),
        opcode: u4::new(1),
        aa: u1::new(0),
        tc: u1::new(0),
        rd: u1::new(0),
        ra: u1::new(0),
        z: u3::new(0),
        rcode: u4::new(0),
        qdcount: 0,
        ancount: 0,
        nscount: 0,
        arcount: 0,
    };
    check(
        &h,
        &[
            0x00, 0x01, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ],
    );
}
