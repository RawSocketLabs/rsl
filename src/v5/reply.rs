use binrw::binrw;
use derive_builder::Builder;

use crate::v5::address::{Address, AddressType};
use crate::v5::response::Response;

#[binrw]
#[derive(Builder, Clone, Debug)]
pub struct Reply {
    #[builder(default = "5")]
    pub version: u8,
    pub reply: Response,
    #[builder(default = "0")]
    pub reserved: u8,
    pub address_type: AddressType,
    #[br(args {address_type })]
    pub bind_addr: Address,
    pub bind_port: u16,
}
