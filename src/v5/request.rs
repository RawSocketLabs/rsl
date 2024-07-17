use binrw::binrw;
use derive_builder::Builder;

use crate::v5::address::{Address, AddressType};
use crate::v5::command::Command;

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Request {
    #[builder(default = "5")]
    pub version: u8,
    pub command: Command,
    #[builder(default = "0")]
    pub reserved: u8,
    pub address_type: AddressType,
    #[br(args { address_type })]
    pub dest_addr: Address,
    pub dest_port: u16,
}
