use std::io::Read;

use binrw::{BinRead, binrw, io::Cursor};
use derive_builder::Builder;

use crate::error::Result;

/// Username/Password authentication request: VER, ULEN, UNAME, PLEN, PASSWD
/// (RFC 1929 §2).
///
/// Sent by the client after the server selects [`Method::Plain`](crate::v5::Method::Plain).
//~ models rfc1929#2
#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct UserPassRequest {
    #[builder(default = "1")]
    pub version: u8,

    #[builder(default = "None")]
    #[builder(setter(into, strip_option))]
    #[bw(map = |n| n.unwrap_or(username.len() as u8))]
    pub username_length: Option<u8>,

    #[br(count = username_length.unwrap_or(0))]
    pub username: Vec<u8>,

    #[builder(default = "None")]
    #[builder(setter(into, strip_option))]
    #[bw(map = |n| n.unwrap_or(password.len() as u8))]
    pub password_length: Option<u8>,

    #[br(count = password_length.unwrap_or(0))]
    pub password: Vec<u8>,
}

impl UserPassRequest {
    /// Reads a `UserPassRequest` from a byte-oriented stream, consuming
    /// exactly the bytes that belong to the message.
    ///
    /// # Errors
    /// Returns an error if I/O fails or the message cannot be parsed.
    pub fn read_from(reader: &mut impl Read) -> Result<Self> {
        let mut head = [0u8; 2];
        reader.read_exact(&mut head)?;

        let username_length = head[1] as usize;
        let mut buf = head.to_vec();
        buf.resize(2 + username_length + 1, 0);
        reader.read_exact(&mut buf[2..])?;

        let password_length = buf[2 + username_length] as usize;
        let start = buf.len();
        buf.resize(start + password_length, 0);
        reader.read_exact(&mut buf[start..])?;

        Self::read(&mut Cursor::new(buf)).map_err(Into::into)
    }
}

#[cfg(test)]
mod unit {
    use binrw::{BinRead, BinWrite, io::Cursor};

    use super::*;

    #[test]
    fn test_user_pass_request_roundtrip() {
        let request = UserPassRequestBuilder::default()
            .username(b"user".to_vec())
            .password(b"hunter2".to_vec())
            .build()
            .unwrap();

        let mut cursor = Cursor::new(Vec::new());
        request.write(&mut cursor).unwrap();

        let bytes = cursor.into_inner();
        assert_eq!(bytes[0], 1);
        assert_eq!(bytes[1], 4);

        let mut cursor = Cursor::new(bytes);
        let parsed = UserPassRequest::read(&mut cursor).expect("parses");
        assert_eq!(parsed.username, b"user".to_vec());
        assert_eq!(parsed.password, b"hunter2".to_vec());
    }
}
