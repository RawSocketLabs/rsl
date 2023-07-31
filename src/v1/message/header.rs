use binrw::binrw;
use modular_bitfield::prelude::*;
use thiserror::Error;

use crate::shared::PROTOCOL_ID;

pub const SMB_SUPPORTED_DIALECTS: &[&str] = &["NT LM 0.12"];

pub type PROTO = u32;
pub type TID = u16;
pub type UID = u16;
pub type MID = u16;

#[binrw]
#[brw(little)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Header {
    pub protocol_id: PROTO,
    pub command: Command,
    pub status: Status,
    pub flags: Flags,
    pub flags2: Flags2,
    pub process_id_high: u16,
    pub signature: Signature,
    pub reserved: u16,
    pub tree_id: TID,
    pub process_id_low: u16,
    pub user_id: UID,
    pub multiplex_id: MID,
}

impl Default for Header {
    fn default() -> Self {
        Self {
            protocol_id: PROTOCOL_ID,
            command: Command::Invalid,
            status: Status::Success,
            flags: Flags::default(),
            flags2: Flags2::default(),
            process_id_high: 0,
            signature: Signature::default(),
            reserved: 0,
            tree_id: 0,
            process_id_low: 0,
            user_id: 0,
            multiplex_id: 0,
        }
    }
}

#[derive(Error, Debug)]
pub enum HeaderError {
    #[error("Invalid Protocol Id: {0}")]
    InvalidProtocolId(u32),
    #[error("Protcol Id failed to parse!")]
    ParseProtocolId,
    #[error("Command failed to parse!")]
    ParseCommand,
    #[error("Status failed to parse!")]
    ParseStatus,
    #[error("Flags failed to parse!")]
    ParseFlags,
    #[error("Flags2 failed to parse!")]
    ParseFlags2,
    #[error("Process Id High failed to parse!")]
    ParseProcessIdHigh,
    #[error("Signature failed to parse!")]
    ParseSignature,
    #[error("Reserved failed to parse!")]
    ParseReserved,
    #[error("Tree Id failed to parse!")]
    ParseTreeId,
    #[error("Process Id Low failed to parse!")]
    ParseProcessIdLow,
    #[error("User Id failed to parse!")]
    ParseUserId,
    #[error("Multiplex Id failed to parse!")]
    ParseMultiplexId,
    #[error("Invalid command: {0}")]
    InvalidCommand(u8),
    #[error("Invalid command: {0}")]
    InvalidStatus(u32),
    #[error("Invalid command: {0}")]
    InvalidFlags(u8),
    #[error("Invalid command: {0}")]
    InvalidFlags2(u16),
    #[error("Invalid length: {0}")]
    InvalidLength(usize),
}

#[binrw]
#[repr(u8)]
#[brw(repr = u8)]
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum Command {
    /// Create a new directory.
    CreateDirectory = 0x00,
    /// Delete an empty directory.
    DeleteDirectory = 0x01,
    /// Open a file.
    Open = 0x02,
    /// Create or open a file.
    Create = 0x03,
    /// Close a file.
    Close = 0x04,
    /// Flush data for a file, or all files associated with a client, PID pair.
    Flush = 0x05,
    /// Delete a file.
    Delete = 0x06,
    /// Rename a file or set of files .
    Rename = 0x07,
    /// Query information about a file.
    QueryInformation = 0x08,
    SetInformation = 0x09,
    Read = 0x0A,
    Write = 0x0B,
    LockByteRange = 0x0C,
    UnlockByteRange = 0x0D,
    CreateTemporary = 0x0E,
    CreateNew = 0x0F,
    CheckDirectory = 0x10,
    ProcessExit = 0x11,
    Seek = 0x12,
    LockAndRead = 0x13,
    WriteAndUnlock = 0x14,
    ReadRaw = 0x1A,
    ReadMpx = 0x1B,
    ReadMpxSecondary = 0x1C,
    WriteRaw = 0x1D,
    WriteMpx = 0x1E,
    WriteMpxSecondary = 0x1F,
    WriteComplete = 0x20,
    QueryServer = 0x21,
    SetInformation2 = 0x22,
    QueryInformation2 = 0x23,
    LockingAndX = 0x24,
    Transaction = 0x25,
    TransactionSecondary = 0x26,
    Ioctl = 0x27,
    IoctlSecondary = 0x28,
    Copy = 0x29,
    Move = 0x2A,
    Echo = 0x2B,
    WriteAndClose = 0x2C,
    OpenAndX = 0x2D,
    ReadAndX = 0x2E,
    WriteAndX = 0x2F,
    NewFileSize = 0x30,
    CloseAndTreeDisc = 0x31,
    Transaction2 = 0x32,
    Transaction2Secondary = 0x33,
    FindClose2 = 0x34,
    FindNotifyClose = 0x35,
    TreeConnect = 0x70,
    TreeDisconnect = 0x71,
    Negotiate = 0x72,
    SessionSetupAndX = 0x73,
    LogoffAndX = 0x74,
    TreeConnectAndX = 0x75,
    SecurityPackageAndX = 0x7E,
    QueryInformationDisk = 0x80,
    Search = 0x81,
    Find = 0x82,
    FindUnique = 0x83,
    FindClose = 0x84,
    NtTransact = 0xA0,
    NtTransactSecondary = 0xA1,
    NtCreateAndX = 0xA2,
    NtCancel = 0xA4,
    NtRename = 0xA5,
    OpenPrintFile = 0xC0,
    WritePrintFile = 0xC1,
    ClosePrintFile = 0xC2,
    GetPrintQueue = 0xC3,
    ReadBulk = 0xD8,
    WriteBulk = 0xD9,
    WriteBulkData = 0xDA,
    Invalid = 0xFE,
    NoAndXCommand = 0xFF,
}

impl Default for Command {
    fn default() -> Self {
        Command::Invalid
    }
}

impl TryFrom<u8> for Command {
    type Error = HeaderError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(Command::CreateDirectory),
            0x01 => Ok(Command::DeleteDirectory),
            0x02 => Ok(Command::Open),
            0x03 => Ok(Command::Create),
            0x04 => Ok(Command::Close),
            0x05 => Ok(Command::Flush),
            0x06 => Ok(Command::Delete),
            0x07 => Ok(Command::Rename),
            0x08 => Ok(Command::QueryInformation),
            0x09 => Ok(Command::SetInformation),
            0x0A => Ok(Command::Read),
            0x0B => Ok(Command::Write),
            0x0C => Ok(Command::LockByteRange),
            0x0D => Ok(Command::UnlockByteRange),
            0x0E => Ok(Command::CreateTemporary),
            0x0F => Ok(Command::CreateNew),
            0x10 => Ok(Command::CheckDirectory),
            0x11 => Ok(Command::ProcessExit),
            0x12 => Ok(Command::Seek),
            0x13 => Ok(Command::LockAndRead),
            0x14 => Ok(Command::WriteAndUnlock),
            0x1A => Ok(Command::ReadRaw),
            0x1B => Ok(Command::ReadMpx),
            0x1C => Ok(Command::ReadMpxSecondary),
            0x1D => Ok(Command::WriteRaw),
            0x1E => Ok(Command::WriteMpx),
            0x1F => Ok(Command::WriteMpxSecondary),
            0x20 => Ok(Command::WriteComplete),
            0x21 => Ok(Command::QueryServer),
            0x22 => Ok(Command::SetInformation2),
            0x23 => Ok(Command::QueryInformation2),
            0x25 => Ok(Command::Transaction),
            0x26 => Ok(Command::TransactionSecondary),
            0x27 => Ok(Command::Ioctl),
            0x28 => Ok(Command::IoctlSecondary),
            0x29 => Ok(Command::Copy),
            0x2A => Ok(Command::Move),
            0x2B => Ok(Command::Echo),
            0x2C => Ok(Command::WriteAndClose),
            0x30 => Ok(Command::NewFileSize),
            0x31 => Ok(Command::CloseAndTreeDisc),
            0x32 => Ok(Command::Transaction2),
            0x33 => Ok(Command::Transaction2Secondary),
            0x34 => Ok(Command::FindClose2),
            0x35 => Ok(Command::FindNotifyClose),
            0x70 => Ok(Command::TreeConnect),
            0x71 => Ok(Command::TreeDisconnect),
            0x72 => Ok(Command::Negotiate),
            0x80 => Ok(Command::QueryInformationDisk),
            0x81 => Ok(Command::Search),
            0x82 => Ok(Command::Find),
            0x83 => Ok(Command::FindUnique),
            0x84 => Ok(Command::FindClose),
            0xA0 => Ok(Command::NtTransact),
            0xA1 => Ok(Command::NtTransactSecondary),
            0xA4 => Ok(Command::NtCancel),
            0xA5 => Ok(Command::NtRename),
            0xC0 => Ok(Command::OpenPrintFile),
            0xC1 => Ok(Command::WritePrintFile),
            0xC2 => Ok(Command::ClosePrintFile),
            0xC3 => Ok(Command::GetPrintQueue),
            0xD8 => Ok(Command::ReadBulk),
            0xD9 => Ok(Command::WriteBulk),
            0xDA => Ok(Command::WriteBulkData),
            0xFE => Ok(Command::Invalid),
            v => Err(HeaderError::InvalidCommand(v)),
        }
    }
}

#[bitfield]
#[binrw]
#[brw(little)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flags {
    #[allow(dead_code)]
    lock_and_read_ok: bool,
    #[allow(dead_code)]
    buf_avail: bool,
    #[allow(dead_code)]
    reserved: bool,
    #[allow(dead_code)]
    case_insensitive: bool,
    #[allow(dead_code)]
    canonicalized_pathnames: bool,
    #[allow(dead_code)]
    oplock: bool,
    #[allow(dead_code)]
    notify: bool,
    #[allow(dead_code)]
    reply: bool,
}

impl Default for Flags {
    fn default() -> Self {
        Self::new()
            .with_case_insensitive(true)
            .with_canonicalized_pathnames(true)
    }
}

#[bitfield]
#[binrw]
#[brw(little)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flags2 {
    #[allow(dead_code)]
    long_names: bool,
    #[allow(dead_code)]
    extended_attributes: bool,
    #[allow(dead_code)]
    security_signatures_supported: bool,
    #[allow(dead_code)]
    compressed: bool,
    #[allow(dead_code)]
    security_signatures_required: bool,
    #[allow(dead_code)]
    reserved1: bool,
    #[allow(dead_code)]
    is_long_name: bool,
    #[allow(dead_code)]
    reserved2: B3,
    #[allow(dead_code)]
    reparse_path: bool,
    #[allow(dead_code)]
    extended_security_negotiation: bool,
    #[allow(dead_code)]
    distributed_file_system: bool,
    #[allow(dead_code)]
    execute_only_reads: bool,
    #[allow(dead_code)]
    is_smb_status: bool,
    #[allow(dead_code)]
    unicode: bool,
}

impl Default for Flags2 {
    fn default() -> Self {
        Self::new()
            .with_unicode(true)
            .with_is_smb_status(true)
            .with_extended_security_negotiation(true)
            .with_is_long_name(true)
            .with_extended_attributes(true)
            .with_long_names(true)
    }
}

#[binrw]
#[repr(u32)]
#[brw(repr = u32, little)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Status {
    Success = 0x00000000,
    MoreProcessingRequired = 0xc0000016,
    Invalid = 0x00010002,
}

impl Default for Status {
    fn default() -> Self {
        Status::Invalid
    }
}

impl TryFrom<u32> for Status {
    type Error = HeaderError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0x00000000 => Ok(Status::Success),
            0xc0000016 => Ok(Status::MoreProcessingRequired),
            v => Err(HeaderError::InvalidStatus(v)),
        }
    }
}

#[binrw]
#[brw(little)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Signature {
    pub key: u32,
    pub common_id: u16,
    pub sequence_number: u16,
}

impl Signature {
    pub fn new(key: u32, common_id: u16, sequence_number: u16) -> Self {
        Self {
            key,
            common_id,
            sequence_number,
        }
    }
}

impl Default for Signature {
    fn default() -> Self {
        Self {
            key: 0,
            common_id: 0,
            sequence_number: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use binrw::BinRead;
    use std::io::Cursor;

    #[test]
    fn empty_signature() {
        let mut buffer = Cursor::new([0u8; 8]);
        let signature = Signature::read(&mut buffer).unwrap();
        assert_eq!(signature, Signature::default());
    }

    #[test]
    fn filled_signature() {
        let mut buffer = Cursor::new([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x00, 0x00]);
        let signature = Signature::read(&mut buffer).unwrap();
        assert_eq!(
            signature,
            Signature {
                key: 0x04030201,
                common_id: 0x0605,
                sequence_number: 0
            }
        );
    }

    #[test]
    fn status_check() {
        let mut buffer = Cursor::new([0x16, 0x00, 0x00, 0xc0]);
        let status = Status::read(&mut buffer).unwrap();
        assert_eq!(status, Status::MoreProcessingRequired);
    }

    #[test]
    fn default_flags() {
        let mut buffer = Cursor::new(vec![0x18]);
        let flags = Flags::read(&mut buffer).unwrap();
        assert_eq!(flags, Flags::default());
    }

    #[test]
    fn default_flags2() {
        let mut buffer = Cursor::new(vec![0x43, 0xc8]);
        let flags2 = Flags2::read(&mut buffer).unwrap();
        assert_eq!(flags2, Flags2::default());
    }

    #[test]
    fn default_header() {
        let header = Vec::from([
            0xff, 0x53, 0x4d, 0x42, 0xFE, 0x00, 0x00, 0x00, 0x00, 0x18, 0x43, 0xc8, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ]);
        let mut buffer = Cursor::new(header);
        let header = Header::read(&mut buffer).unwrap();
        assert_eq!(header, Header::default());
    }
}
