use binrw::binrw;
use modular_bitfield::prelude::*;

#[bitfield]
#[binrw]
#[brw(little)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capabilities {
    pub raw_mode: bool,
    pub mpx_mode: bool,
    pub unicode: bool,
    pub large_files: bool,
    pub nt_smb: bool,
    pub rpc_api: bool,
    pub nt_status: bool,
    pub l2_oplocks: bool,
    pub lock_and_read: bool,
    pub nt_find: bool,
    pub r1: B2,
    pub distributed_file_system: bool,
    pub info_level: bool,
    pub large_read_x: bool,
    pub large_write_x: bool,
    pub lwio: bool,
    pub r2: B6,
    pub unix: bool,
    pub r3: B1,
    pub compressed: bool,
    pub r4: B3,
    pub dynamic_re_auth: bool,
    pub r5: B1,
    pub extend_security: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self::from_bytes([0; 4])
    }
}
