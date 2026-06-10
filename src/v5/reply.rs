use std::io::Read;

use binrw::{binrw, io::Cursor, BinRead};
use derive_builder::Builder;

use crate::error::Result;
use crate::v5::address::{read_addressed_tail, Address, AddressType};
use crate::v5::response::Response;

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
