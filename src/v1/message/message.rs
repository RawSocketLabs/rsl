use binrw::io::Cursor;
use binrw::{binrw, BinRead, BinWrite};
use thiserror::Error;

use crate::v1::cmd::*;
use crate::v1::message::{Command, Data, Header, Parameter, Status};

#[binrw]
#[brw(little)]
#[derive(Debug)]
pub struct Message {
    pub header: Header,
    pub parameter: Parameter,
    #[br(args(header.command))]
    pub data: Data,
}

impl Message {
    /// Create a new message.
    ///
    /// SMB Messages are composed of a header, parameter, and data section.
    /// Messages can be written and read from either a stream or a completed buffer.
    ///
    /// # Examples
    /// ## Write to a buffer
    /// ```
    /// # use binrw::io::Cursor;
    /// # use client::v1::message::{Data, Header, Message, Parameter};
    /// // Must have the BinWrite trait in scope from binrw
    /// use binrw::BinWrite;
    /// # fn main () {
    /// # let header = Header::default();
    /// # let parameter = Parameter::default();
    /// # let data = Data::default();
    /// // Create a buffer for writing the message to.
    /// let mut buffer = Cursor::new(Vec::new());
    ///
    /// // Create a new message.
    /// let message = Message::new(header, parameter, data);
    ///
    /// // Write the message to the buffer.
    /// message.write(&mut buffer).expect("Message failed to write properly.");
    /// # }
    /// ```
    pub fn new(header: Header, parameter: Parameter, data: Data) -> Message {
        Message {
            header,
            parameter,
            data,
        }
    }

    /// Get response from a message.
    pub fn parse_response(buffer: &[u8]) -> Response {
        // Attempt to parse the buffer into a message.
        // TODO: Fix the unwrap here. This should instead safely unpack the data from the message
        // and provide the most detailed error possible.
        let message: Message = buffer.try_into().unwrap();

        // If the message is not a success, return the specified error.
        if message.header.status != Status::Success {
            return Response::Error(format!("{:?}", message.header.status));
        }

        // If the message is a success, determine the command we are responding to and return the
        // approrpriate response structure.
        match message.header.command {
            // Negotiate Protocol
            Command::Negotiate => {
                Response::Negotiate(NegotiateResponse::try_from(message).unwrap())
            }
            _ => Response::Unknown(message),
        }
    }
}

impl Default for Message {
    fn default() -> Self {
        let header = Header::default();
        let parameter = Parameter::default();
        let data = Data::default();
        Message::new(header, parameter, data)
    }
}

pub enum Response {
    Negotiate(NegotiateResponse),
    Unknown(Message),
    Error(String),
}

pub enum ResponseError {
    Negotiate(NegotiateError),
    Unknown(MessageError),
}

impl TryInto<Vec<u8>> for Message {
    type Error = binrw::Error;

    fn try_into(self) -> Result<Vec<u8>, Self::Error> {
        let mut buffer = Cursor::new(Vec::new());
        self.write(&mut buffer)?;
        Ok(buffer.into_inner())
    }
}

impl TryFrom<&[u8]> for Message {
    type Error = binrw::Error;

    fn try_from(buffer: &[u8]) -> Result<Self, Self::Error> {
        let mut cursor = Cursor::new(buffer);
        let message = Message::read(&mut cursor)?;
        Ok(message)
    }
}

#[derive(Error, Debug)]
pub enum MessageError {
    #[error("generic")]
    Generic,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::path::Path;

    #[test]
    fn test_message() {
        let mut path = Path::new("/tmp/hello.txt");
        let mut file = File::create(&path).unwrap();
        let message = Message::default();
        message.write(&mut file).unwrap();
    }
}
