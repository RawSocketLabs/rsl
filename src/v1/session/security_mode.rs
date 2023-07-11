use binrw::{binrw, BinRead, BinWrite};
use modular_bitfield::prelude::*;

#[bitfield]
#[derive(BinRead, BinWrite, Clone, Copy, Debug, PartialEq)]
#[brw(little)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
pub struct SecurityMode {
    #[allow(dead_code)]
    pub user_mode: bool,
    #[allow(dead_code)]
    pub encrypt_passwords: bool,
    #[allow(dead_code)]
    pub signing_enabled: bool,
    #[allow(dead_code)]
    pub signing_required: bool,
    #[allow(dead_code)]
    pub reserved: B4,
}
