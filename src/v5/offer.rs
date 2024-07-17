use binrw::binrw;
use derive_builder::Builder;

use crate::v5::method::Method;

#[binrw]
#[derive(Builder, Clone, Debug)]
pub struct Offer {
    #[builder(default = "5")]
    pub version: u8,
    pub method: Method,
}
