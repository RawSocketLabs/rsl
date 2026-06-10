use std::io::Read;

use binrw::{binrw, io::Cursor, BinRead};
use derive_builder::Builder;

use crate::error::Result;
use crate::v5::method::Method;

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Offer {
    #[builder(default = "5")]
    pub version: u8,
    pub method: Method,
}

impl Offer {
    /// Reads an `Offer` from a byte-oriented stream, consuming exactly the
    /// bytes that belong to the message.
    ///
    /// # Errors
    /// Returns an error if I/O fails or the message cannot be parsed.
    pub fn read_from(reader: &mut impl Read) -> Result<Self> {
        let mut buf = [0u8; 2];
        reader.read_exact(&mut buf)?;

        Self::read(&mut Cursor::new(buf)).map_err(Into::into)
    }
}
