use std::io::Read;

use binrw::{binrw, io::Cursor, BinRead};
use derive_builder::Builder;

use crate::error::Result;

/// Username/Password authentication status code (RFC 1929).
///
/// A value other than [`UserPassStatus::Success`] indicates failure and the
/// connection must be closed.
#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum UserPassStatus {
    #[brw(magic = 0x00u8)]
    Success = 0x00,

    Custom(u8),
}

/// Username/Password authentication response: VER, STATUS (RFC 1929 §2).
///
/// Sent by the server after verifying a [`UserPassRequest`](crate::v5::UserPassRequest).
//~ models rfc1929#2
#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct UserPassResponse {
    #[builder(default = "1")]
    pub version: u8,
    pub status: UserPassStatus,
}

impl UserPassResponse {
    /// Reads a `UserPassResponse` from a byte-oriented stream, consuming
    /// exactly the bytes that belong to the message.
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
    use binrw::{io::Cursor, BinRead, BinWrite};

    use super::*;

    #[test]
    fn test_user_pass_response_roundtrip() {
        let response = UserPassResponseBuilder::default()
            .status(UserPassStatus::Success)
            .build()
            .unwrap();

        let mut cursor = Cursor::new(Vec::new());
        response.write(&mut cursor).unwrap();

        let mut cursor = Cursor::new(cursor.into_inner());
        let parsed = UserPassResponse::read(&mut cursor).expect("parses");
        assert_eq!(parsed.version, 1);
        assert_eq!(parsed.status, UserPassStatus::Success);
    }

    #[test]
    fn test_user_pass_response_failure() {
        let mut cursor = Cursor::new(vec![0x01u8, 0x01]);
        let parsed = UserPassResponse::read(&mut cursor).expect("parses");
        assert_eq!(parsed.status, UserPassStatus::Custom(0x01));
    }
}
