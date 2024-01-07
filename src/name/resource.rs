use binrw::{binrw, BinRead, BinWrite};
use derive_builder::Builder;
use modular_bitfield::prelude::*;

use crate::name::{parse_labels, Label};

#[binrw]
#[derive(Debug, Clone, Builder)]
pub struct Resource {
    #[br(parse_with = parse_labels)]
    pub name: Vec<Label>,
    pub rtype: ResourceType,
    pub rclass: ResourceClass,
    pub ttl: u32,
    pub length: u16,
    #[br(args(rtype))]
    pub data: ResourceData,
}

#[repr(u16)]
#[derive(BinRead, BinWrite, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    #[brw(magic = 0x0001u16)]
    A = 0x0001,
    #[brw(magic = 0x0002u16)]
    NS = 0x0002,
    #[brw(magic = 0x000Au16)]
    NULL = 0x000A,
    #[brw(magic = 0x0020u16)]
    NB = 0x0020,
    #[brw(magic = 0x0021u16)]
    NBSTAT = 0x0021,
    Unknown(u16),
}

#[repr(u16)]
#[derive(BinRead, BinWrite, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceClass {
    #[brw(magic = 0x0001u16)]
    Internet = 0x0001,
    Unknown(u16),
}

#[binrw]
#[br(import(rtype: ResourceType))]
#[derive(Debug, Clone)]
pub enum ResourceData {
    #[br(pre_assert(rtype == ResourceType::NB))]
    Record(Record),
    #[br(pre_assert(rtype == ResourceType::NBSTAT))]
    Status(Status),
    #[br(pre_assert(rtype == ResourceType::A || rtype == ResourceType::NS))]
    Redirect(),
    #[br(pre_assert(rtype == ResourceType::NBSTAT))]
    Acknowledgement(),
}

#[binrw]
#[derive(Debug, Clone)]
pub struct Record {
    pub flags: NBFlags,
    pub address: NBAddress,
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
pub struct NBFlags {
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
#[derive(Debug, Clone)]
pub struct NBAddress {
    pub address: [u8; 4],
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
