use std::io::Read;

use binrw::{binrw, io::Cursor, BinRead};
use derive_builder::Builder;

use crate::error::Result;
use crate::v5::address::{read_addressed_tail, Address, AddressType};
use crate::v5::response::Response;

/// Server reply: VER, REP, RSV, ATYP, BND.ADDR, BND.PORT (RFC 1928 §6).
//~ models rfc1928#6
#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Reply {
    #[builder(default = "5")]
    pub version: u8,
    pub reply: Response,
    #[builder(default = "0")]
    pub reserved: u8,
    pub address_type: AddressType,
    #[br(args {address_type })]
    pub bind_addr: Address,
    pub bind_port: u16,
}

impl Reply {
    /// Reads a `Reply` from a byte-oriented stream, consuming exactly the
    /// bytes that belong to the message.
    ///
    /// # Errors
    /// Returns an error if I/O fails, the address type is unknown, or the
    /// message cannot be parsed.
    pub fn read_from(reader: &mut impl Read) -> Result<Self> {
        let mut head = [0u8; 4];
        reader.read_exact(&mut head)?;

        let buf = read_addressed_tail(reader, head.to_vec())?;

        Self::read(&mut Cursor::new(buf)).map_err(Into::into)
    }
}

#[cfg(test)]
mod unit {
    use std::net::Ipv4Addr;

    use binrw::BinWrite;

    use super::*;

    /// The §6 reply round-trips: VER, REP, RSV=0, ATYP, BND.ADDR, BND.PORT.
    #[test]
    fn reply_matches_wire_format() {
        let reply = ReplyBuilder::default()
            .reply(Response::Succeeded)
            .address_type(AddressType::V4)
            .bind_addr(Address::V4(Ipv4Addr::new(127, 0, 0, 1)))
            .bind_port(1080)
            .build()
            .unwrap();
        let mut cursor = Cursor::new(Vec::new());
        reply.write(&mut cursor).unwrap();
        let bytes = cursor.into_inner();
        // VER=5, REP=0 (succeeded), RSV=0, ATYP=1 (IPv4), 127.0.0.1, port 1080.
        assert_eq!(bytes, vec![5, 0, 0, 1, 127, 0, 0, 1, 0x04, 0x38]);

        let parsed = Reply::read_from(&mut bytes.as_slice()).expect("parses");
        assert_eq!(parsed.version, 5);
        assert_eq!(parsed.reply, Response::Succeeded);
        assert_eq!(parsed.reserved, 0);
        assert_eq!(parsed.bind_port, 1080);
    }
}
