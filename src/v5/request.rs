use std::io::Read;

use binrw::{binrw, io::Cursor, BinRead};
use derive_builder::Builder;

use crate::error::Result;
use crate::v5::address::{read_addressed_tail, Address, AddressType};
use crate::v5::command::Command;

/// Client request: VER, CMD, RSV, ATYP, DST.ADDR, DST.PORT (RFC 1928 §4).
//~ models rfc1928#4
#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Request {
    #[builder(default = "5")]
    pub version: u8,
    pub command: Command,
    #[builder(default = "0")]
    pub reserved: u8,
    pub address_type: AddressType,
    #[br(args { address_type })]
    pub dest_addr: Address,
    pub dest_port: u16,
}

impl Request {
    /// Reads a `Request` from a byte-oriented stream, consuming exactly the
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
    use binrw::BinWrite;

    use super::*;

    #[test]
    fn test_request_read_from_is_exactly_framed() {
        let request = RequestBuilder::default()
            .command(Command::Connect)
            .address_type(AddressType::Domain)
            .dest_addr(Address::Domain("example.com".into()))
            .dest_port(443)
            .build()
            .unwrap();

        let mut cursor = Cursor::new(Vec::new());
        request.write(&mut cursor).unwrap();
        let mut bytes = cursor.into_inner();
        bytes.extend_from_slice(b"trailing");

        let mut reader = bytes.as_slice();
        let parsed = Request::read_from(&mut reader).expect("parses");

        assert_eq!(parsed.dest_port, 443);
        assert_eq!(reader, b"trailing");
    }
}
