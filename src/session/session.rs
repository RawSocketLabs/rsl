// 3rd Party
use binrw::{binrw, io::Cursor, BinRead, BinWrite};
use derive_builder::Builder;
use modular_bitfield::prelude::*;
use thiserror::Error;

#[binrw]
#[brw(big)]
#[derive(Builder, Debug, Clone, PartialEq, Eq)]
#[builder(build_fn(skip))]
pub struct Session {
    #[br(dbg)]
    pub packet_type: PacketType,
    #[builder(setter(custom))]
    pub flags: PacketFlags,
    #[builder(setter(skip))]
    pub length: u16,
    #[br(count = if flags.length_extension() { 65_536 + length as u32} else { length as u32})]
    pub payload: Vec<u8>,
}

impl Session {
    pub fn as_bytes(&self) -> Vec<u8> {
        self.into()
    }
}

impl Into<Vec<u8>> for &Session {
    fn into(self) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        self.write(&mut buffer).unwrap();
        buffer.into_inner()
    }
}

impl SessionBuilder {
    pub fn flags(&mut self, flags: Flags) -> &mut Self {
        self.flags = Some(flags.into());
        self
    }

    pub fn build(&mut self) -> Result<Session, BuildError> {
        // Ensure that each required field is set on the builder before attempting to build
        let (packet_type, internal_flags, payload) =
            match (self.packet_type, self.flags, self.payload.clone()) {
                (Some(ptype), Some(flags), Some(payload)) => (ptype, flags, payload),
                (None, _, _) => return Err(BuildError::PacketTypeNotSet),
                (_, None, _) => return Err(BuildError::FlagsNotSet),
                (_, _, None) => return Err(BuildError::PayloadNotSet),
            };

        // Ensure that the payload length is not too long
        let (flags, length) = match payload.len() {
            x @ 0..=65535 => (internal_flags.with_length_extension(false), x as u16),
            x @ 65536..=131_071 => (
                internal_flags.with_length_extension(true),
                (x & 0xFFFF) as u16,
            ),
            _ => return Err(BuildError::PayloadLength),
        };

        // Return the session
        Ok(Session {
            packet_type,
            flags,
            length,
            payload,
        })
    }
}

#[binrw]
#[brw(big)]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketType {
    #[brw(magic = 0x00u8)]
    Message = 0x00,
    #[brw(magic = 0x81u8)]
    Request = 0x81,
    #[brw(magic = 0x82u8)]
    Positive = 0x82,
    #[brw(magic = 0x83u8)]
    Negative = 0x83,
    #[brw(magic = 0x84u8)]
    Retarget = 0x84,
    #[brw(magic = 0x85u8)]
    KeepAlive = 0x85,
    Custom(u8),
}

#[bitfield]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Flags {
    pub reserved: B7,
    #[skip(setters)]
    pub length_extension: bool,
}

#[bitfield]
#[derive(BinRead, BinWrite)]
#[brw(big)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PacketFlags {
    pub reserved: B7,
    pub length_extension: bool,
}

impl From<Flags> for PacketFlags {
    fn from(flags: Flags) -> Self {
        Self::new().with_reserved(flags.reserved())
    }
}

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
pub enum BuildError {
    #[error("Payload length is too long")]
    PayloadLength,
    #[error("Packet type is not set")]
    PacketTypeNotSet,
    #[error("Flags are not set")]
    FlagsNotSet,
    #[error("Payload is not set")]
    PayloadNotSet,
}

#[cfg(test)]
mod unit {
    use super::*;
    use binrw::io::Cursor;

    #[test]
    fn session_message() {
        let mut buffer = Cursor::new(Vec::new());

        let session = SessionBuilder::default()
            .packet_type(PacketType::Custom(0x34))
            .flags(Flags::new().with_reserved_checked(0).unwrap())
            .payload(vec![0x00, 0x01, 0x02, 0x03, 0x04])
            .build()
            .unwrap();

        session.write(&mut buffer).unwrap();
        buffer.set_position(0);

        let read_session: Session = Session::read(&mut buffer).unwrap();
        println!("{:?}", read_session);
    }
}
