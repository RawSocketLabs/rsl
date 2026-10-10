use binrw::binrw;
use modular_bitfield::prelude::*;

#[bitfield]
#[binrw]
#[brw(little)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SecurityMode {
    pub user_mode: bool,
    pub encrypt_passwords: bool,
    pub signing_enabled: bool,
    pub signing_required: bool,
    pub reserved: B4,
}

impl Default for SecurityMode {
    fn default() -> Self {
        Self::from_bytes([0; 1])
    }
}
