use std::io::Read;

use binrw::{binrw, io::Cursor, BinRead};
use derive_builder::Builder;

use crate::error::Result;
use crate::v5::method::Method;

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Identifier {
    #[builder(default = "5")]
    pub version: u8,

    #[builder(default = "None")]
    #[builder(setter(into, strip_option))]
    #[bw(map = |n| n.unwrap_or(methods.len() as u8))]
    pub number_of_methods: Option<u8>,

    #[br(count = number_of_methods.unwrap_or(0))]
    pub methods: Vec<Method>,
}

impl Identifier {
    /// Reads an `Identifier` from a byte-oriented stream, consuming exactly
    /// the bytes that belong to the message.
    ///
    /// # Errors
    /// Returns an error if I/O fails or the message cannot be parsed.
    pub fn read_from(reader: &mut impl Read) -> Result<Self> {
        let mut head = [0u8; 2];
        reader.read_exact(&mut head)?;

        let mut buf = head.to_vec();
        buf.resize(2 + head[1] as usize, 0);
        reader.read_exact(&mut buf[2..])?;

        Self::read(&mut Cursor::new(buf)).map_err(Into::into)
    }
}

#[cfg(test)]
mod unit {
    use binrw::{io::Cursor, BinWrite};

    use super::*;

    #[test]
    fn test_identifier() {
        let id = IdentifierBuilder::default()
            .methods(vec![Method::NoAuth, Method::GssApi])
            .build()
            .unwrap();
        println!("{:?}", id);

        let mut cursor = Cursor::new(Vec::new());

        id.write(&mut cursor).unwrap();

        println!("{:?}", cursor.into_inner());
    }
}
