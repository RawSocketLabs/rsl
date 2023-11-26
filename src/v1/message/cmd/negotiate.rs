use binrw::io::Cursor;
use binrw::{binrw, BinRead};
use thiserror::Error;

use crate::v1::message::session::{Capabilities, SecurityMode};
use crate::v1::message::{Command, Data, DataLength, DataType, Header, Message, Parameter};

impl Message {
    /// Construct a negotiate message
    ///
    /// The negotiate message is used to negotiate the SMB dialect that will be used for the
    /// remainder of the session. The negotiate message is sent by the client and the server
    /// responds with a negotiate response message.
    ///
    /// # Example
    /// ```
    /// # use binrw::io::Cursor;
    /// # use client::v1::message::Message;
    /// // Must have the BinWrite trait in scope
    /// use binrw::BinWrite;
    /// # fn main() {
    /// // Build the buffer
    /// let mut buffer = Cursor::new(Vec::new());
    ///
    /// // Construct a negotiate message
    /// let negotiation_message = Message::negotiate();
    ///
    /// // Write the message to the buffer
    /// negotiation_message.write(&mut buffer).unwrap();
    /// # }
    pub fn negotiate() -> Self {
        // Construct the negotiate message header
        let mut header = Header::default();
        header.command = Command::Negotiate;

        // Construct the negotiate message data.
        // For each supported SMB dialect push that as part of the data field
        let data = DataType::NegotiateRequest(NegotiateRequest::default());

        // Construct the negotiate message
        Message::new(header, Parameter::default(), Data::new(Some(data)))
    }
}

#[binrw]
#[brw(little, magic = 2u8)]
#[derive(PartialEq, Copy, Clone, Debug)]
pub enum Dialect {
    #[brw(magic = b"PC NETWORK PROGRAM 1.0\x00")]
    PCNETWORKPROGRAM10,

    #[brw(magic = b"NT LANMAN 1.0\x00")]
    LANMAN10,

    #[brw(magic = b"NT LM 0.12\x00")]
    NTLM012,

    #[brw(magic = b"LANMAN2.1\x00")]
    LANMAN21,
}

impl Dialect {
    pub fn all_dialects() -> Vec<Dialect> {
        vec![
            Dialect::PCNETWORKPROGRAM10,
            Dialect::LANMAN10,
            Dialect::NTLM012,
            Dialect::LANMAN21,
        ]
    }

    pub fn from_index(dialects: &[Dialect], idx: u16) -> Dialect {
        dialects[idx as usize]
    }
}

impl Default for Dialect {
    fn default() -> Self {
        Dialect::NTLM012
    }
}

impl DataLength for Dialect {
    /// The length of the dialect in bytes including the prefix 2u8 magic number
    /// and the null terminator
    fn len(&self) -> u16 {
        match self {
            Dialect::PCNETWORKPROGRAM10 => 0x18,
            Dialect::LANMAN10 => 0x0F,
            Dialect::NTLM012 => 0x0C,
            Dialect::LANMAN21 => 0x0B,
        }
    }
}

#[binrw]
#[brw(little)]
#[br(import(count: usize))]
#[derive(Debug, Clone, PartialEq)]
pub struct NegotiateRequest {
    #[br(count = count)]
    pub dialects: Vec<Dialect>,
}

impl NegotiateRequest {
    pub fn new(dialects: &[Dialect]) -> Self {
        Self {
            dialects: dialects.to_vec(),
        }
    }
}

impl Default for NegotiateRequest {
    fn default() -> Self {
        Self::new(&Dialect::all_dialects())
    }
}

impl DataLength for NegotiateRequest {
    fn len(&self) -> u16 {
        let mut len = 0;
        for dialect in &self.dialects {
            len += dialect.len();
        }
        len as u16
    }
}

#[binrw]
#[brw(little)]
#[derive(Debug, Clone, PartialEq)]
pub struct NegotiateResponse {
    #[br(map = |x: u16| Dialect::from_index(&Dialect::all_dialects(), x))]
    pub dialect: Dialect,
    pub security_mode: SecurityMode,
    pub max_mpx_count: u16,
    pub max_vcs: u16,
    pub max_buffer_size: u32,
    pub max_raw_size: u32,
    pub session_key: u32,
    pub capabilities: Capabilities,
    pub system_time: u64,
    pub time_zone: u16,
    pub challenge_length: u8,
}

impl Default for NegotiateResponse {
    fn default() -> Self {
        Self {
            dialect: Dialect::default(),
            security_mode: SecurityMode::default(),
            max_mpx_count: 0,
            max_vcs: 0,
            max_buffer_size: 0,
            max_raw_size: 0,
            session_key: 0,
            capabilities: Capabilities::default(),
            system_time: 0,
            time_zone: 0,
            challenge_length: 0,
        }
    }
}

impl TryFrom<Message> for NegotiateResponse {
    type Error = NegotiateError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        // Validate the header
        if message.header.command != Command::Negotiate {
            return Err(NegotiateError::Header);
        }

        // if message.data.data.is_some() {
        //     return Err(NegotiateError::Data);
        // }

        // Validate the parameter
        match message.parameter.param {
            Some(p) => {
                let mut params = Cursor::new(p);
                NegotiateResponse::read(&mut params).map_err(|_| NegotiateError::Parameter)
            }
            None => Err(NegotiateError::Parameter),
        }
    }
}

impl DataLength for NegotiateResponse {
    fn len(&self) -> u16 {
        0x2c
    }
}

#[derive(Error, Debug)]
pub enum NegotiateError {
    #[error("Header error")]
    Header,

    #[error("Parameter error")]
    Parameter,

    #[error("Data error")]
    Data,

    #[error("Write error")]
    Write,
}

impl Default for NegotiateError {
    fn default() -> Self {
        NegotiateError::Header
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::message::{Command, Status};

    use binrw::io::Cursor;
    use binrw::BinWrite;

    #[test]
    fn parse_negotiate_response() {
        let header = Vec::from([
            0xff, 0x53, 0x4d, 0x42, 0x72, 0x00, 0x00, 0x00, 0x00, 0x18, 0x43, 0xc8, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xfe, 0xff,
            0x00, 0x00, 0x01, 0x00,
        ]);
        let param = Vec::from([
            0x11, 0x00, 0x00, 0x03, 0x32, 0x00, 0x01, 0x00, 0x04, 0x41, 0x00, 0x00, 0x00, 0x00,
            0x01, 0x00, 0x19, 0x1e, 0x00, 0x00, 0xfd, 0xf3, 0x80, 0x80, 0x80, 0x4d, 0x8e, 0xd1,
            0xfa, 0x20, 0xce, 0x01, 0x00, 0x00, 0x00,
        ]);
        let data = Vec::from([
            0x3a, 0x00, 0x68, 0x6d, 0x6e, 0x68, 0x64, 0x2d, 0x74, 0x69, 0x31, 0x6b, 0x6c, 0x73,
            0x00, 0x00, 0x00, 0x00, 0x60, 0x28, 0x06, 0x06, 0x2b, 0x06, 0x01, 0x05, 0x05, 0x02,
            0xa0, 0x1e, 0x30, 0x1c, 0xa0, 0x0e, 0x30, 0x0c, 0x06, 0x0a, 0x2b, 0x06, 0x01, 0x04,
            0x01, 0x82, 0x37, 0x02, 0x02, 0x0a, 0xa3, 0x0a, 0x30, 0x08, 0xa0, 0x06, 0x1b, 0x04,
            0x4e, 0x4f, 0x4e, 0x45,
        ]);
        let buffer = [&header[..], &param[..], &data[..]].concat();
        //let negotiate_response: NegotiateResponse = Message::try_from(buffer.as_slice()).unwrap();
        //assert_eq!(details.dialect, Dialect::NTLM012);
    }

    #[test]
    fn negotiation() {
        let message = Message::negotiate();
        // Ensure that we are sending the negotiate command
        assert_eq!(message.header.command, Command::Negotiate);

        // This should be a successful message
        assert_eq!(message.header.status, Status::Success);

        // Ensure we are sending a zero size parameter field
        assert_eq!(message.parameter.size, 0);

        // When writing the parameter field ensure that only one byte is written and that it is a
        // zero
        let mut buffer = Cursor::new(Vec::new());
        message.parameter.write(&mut buffer).unwrap();
        let buffer = buffer.into_inner();
        assert_eq!(buffer.len(), 1);
        assert_eq!(buffer[0], 0);

        // Ensure that we are sending a data field of the correct size
        assert_eq!(message.data.size, 12);

        // Ensure that we are sending the correct data
        let mut buffer = Cursor::new(Vec::new());
        message.data.write(&mut buffer).unwrap();
        let buffer = buffer.into_inner();
        assert_eq!(buffer.len(), 14);
        assert_eq!(&buffer[2..], b"\x02NT LM 0.12\x00");
    }
}
