use std::io::Read;

use binrw::{binrw, io::Cursor, BinRead};
use derive_builder::Builder;

use crate::error::Result;
use crate::v5::method::Method;

/// Server method-selection reply: VER, METHOD (RFC 1928 §3).
//~ models rfc1928#3
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

#[cfg(test)]
mod unit {
    use binrw::BinWrite;

    use super::*;

    /// The §3 method-selection reply round-trips: VER, METHOD.
    #[test]
    fn offer_matches_wire_format() {
        let offer = OfferBuilder::default()
            .method(Method::NoAuth)
            .build()
            .unwrap();
        let mut cursor = Cursor::new(Vec::new());
        offer.write(&mut cursor).unwrap();
        let bytes = cursor.into_inner();
        assert_eq!(bytes, vec![0x05, 0x00]); // VER=5, METHOD=NoAuth

        let parsed = Offer::read_from(&mut bytes.as_slice()).expect("parses");
        assert_eq!(parsed.version, 5);
        assert_eq!(parsed.method, Method::NoAuth);
    }
}
