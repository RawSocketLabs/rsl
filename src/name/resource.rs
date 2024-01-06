use std::net::IpAddr;

use binrw::binrw;
use derive_builder::Builder;
use modular_bitfield::prelude::*;

use crate::name::Label;

pub struct Resource {
    pub name: Vec<Label>,
    pub rtype: ResourceType,
    pub rclass: ResourceClass,
    pub ttl: u32,
    pub length: u16,
}

#[repr(u16)]
pub enum ResourceType {
    A = 0x0001,
    NS = 0x0002,
    NULL = 0x000A,
    NB = 0x0020,
    NBSTAT = 0x0021,
    Unknown(u16),
}

#[repr(u16)]
pub enum ResourceClass {
    Internet = 0x0001,
    Unknown(u16),
}

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
    HNode = 3,
}

pub struct NBAddress {
    pub address: IpAddr,
}

pub struct NameFlags {
    pub group: bool,
    pub owner: NodeType,
    pub deregister: bool,
    pub confict: bool,
    pub active: bool,
    pub permanent: bool,
    pub reserved: B9,
}
