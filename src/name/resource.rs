use std::net::IpAddr;

use binrw::{BinRead, BinWrite, binrw};
use derive_builder::Builder;
use modular_bitfield::prelude::*;

use crate::name::label::{Label, parse_labels};

/// A NetBIOS resource record.
#[binrw]
#[derive(Builder, Debug, Clone)]
pub struct Resource {
    /// The name of the resource which is a sequence of labels.
    #[br(parse_with = parse_labels)]
    pub name: Vec<Label>,

    /// The type of the resource.
    pub rtype: ResourceType,

    /// The class of the resource.
    pub rclass: ResourceClass,

    /// The time to live of the resource.
    pub ttl: u32,

    /// The length of the resource data.
    pub length: u16,

    /// The resource data.
    #[br(args(rtype, length))]
    pub data: ResourceData,
}

/// The types of NetBIOS resources.
#[repr(u16)]
#[derive(BinRead, BinWrite, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    /// An A record maps to an IPv4 address
    #[brw(magic = 0x0001u16)]
    A = 0x0001,

    /// A NS record
    #[brw(magic = 0x0002u16)]
    NS = 0x0002,

    /// A Null record is utilized for WACK responses.
    #[brw(magic = 0x000Au16)]
    NULL = 0x000A,

    /// A NB record maps a full record to a NetBIOS name.
    #[brw(magic = 0x0020u16)]
    NB = 0x0020,

    /// A NBSTAT record maps a NetBIOS name to a full record.
    #[brw(magic = 0x0021u16)]
    NBSTAT = 0x0021,

    /// A custom resource type.
    Custom(u16),
}

/// The classes of NetBIOS resources.
#[repr(u16)]
#[derive(BinRead, BinWrite, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceClass {
    /// An Internet resource.
    #[brw(magic = 0x0001u16)]
    Internet = 0x0001,

    /// An custom resource class.
    CUstom(u16),
}

/// The data of a NetBIOS resource.
///
/// The data of a NetBIOS resource depends on the type of the resource.
#[binrw]
#[br(import(rtype: ResourceType, len: u16))]
#[derive(Debug, Clone)]
pub enum ResourceData {
    /// A record contains a NetBIOS address and flags.
    #[br(pre_assert(rtype == ResourceType::NB))]
    Record(#[br(args(len))] Record),

    /// A status contains a list of names and statistics.
    #[br(pre_assert(rtype == ResourceType::NBSTAT))]
    Status(Status),

    /// A null resource contains no data.
    #[br(pre_assert(rtype == ResourceType::A || rtype == ResourceType::NS))]
    Redirect(),

    /// A null resource contains no data.
    #[br(pre_assert(rtype == ResourceType::NBSTAT))]
    Acknowledgement(),
}

#[binrw]
#[br(import(len: u16))]
#[derive(Debug, Clone)]
pub struct Record {
    pub flags: RecordFlags,
    #[br(args(len))]
    pub address: RecordAddress,
}

impl Record {
    pub fn new(flags: RecordFlags, address: RecordAddress) -> Self {
        Self { flags, address }
    }
}

impl From<Record> for ResourceData {
    fn from(record: Record) -> Self {
        Self::Record(record)
    }
}

#[binrw]
#[derive(Debug, Clone)]
pub struct Status {
    pub num: u8,
    #[br(count = num)]
    pub names: Vec<NodeName>,
    pub statistics: Statistics,
}

#[bitfield]
#[derive(BinRead, BinWrite, Debug, Clone)]
pub struct Statistics {
    pub unit_id: B48,
    pub jumpers: u8,
    pub test_result: u8,
    pub version: u16,
    pub period_of_stats: u16,
    pub crcs: u16,
    pub errs: u16,
    pub collisions: u16,
    pub send_aborts: u16,
    pub sends: u32,
    pub recvs: u32,
    pub retransmits: u16,
    pub no_resource_conditions: u16,
    pub free_cmd_blocks: u16,
    pub total_cmd_blocks: u16,
    pub max_cmd_blocks: u16,
    pub pending_sessions: u16,
    pub max_pending_sessions: u16,
    pub max_total_sessions: u16,
    pub session_data_packet_size: u16,
}

#[bitfield]
#[derive(BinRead, BinWrite, Debug, Clone, Copy)]
pub struct RecordFlags {
    pub group: bool,
    pub owner: NodeType,
    pub reserved: B13,
}

#[derive(BitfieldSpecifier, Clone, Copy, Debug)]
#[bits = 2]
pub enum NodeType {
    BNode = 0,
    PNode = 1,
    MNode = 2,
    Reserved = 3,
}

#[binrw]
#[br(import(len: u16))]
#[derive(Debug, Clone)]
pub struct RecordAddress {
    #[br(count = len - 2)]
    pub address: Vec<u8>,
}

impl RecordAddress {
    pub fn new(address: IpAddr) -> Self {
        match address {
            IpAddr::V4(v4_addr) => Self {
                address: v4_addr.octets().to_vec(),
            },
            IpAddr::V6(v6_addr) => Self {
                address: v6_addr.octets().to_vec(),
            },
        }
    }
}

#[binrw]
#[derive(Debug, Clone)]
pub struct NodeName {
    #[br(parse_with = parse_labels)]
    pub labels: Vec<Label>,
    pub flags: NameFlags,
}

#[bitfield]
#[derive(BitfieldSpecifier, BinRead, BinWrite, Debug, Clone)]
pub struct NameFlags {
    pub group: bool,
    pub owner: NodeType,
    pub deregister: bool,
    pub confict: bool,
    pub active: bool,
    pub permanent: bool,
    pub reserved: B9,
}
