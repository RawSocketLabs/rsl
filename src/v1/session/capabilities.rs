use binrw::{binrw, BinRead, BinWrite};
use modular_bitfield::prelude::*;

#[bitfield]
#[derive(BinRead, BinWrite, Clone, Copy, Debug, PartialEq)]
#[brw(little)]
#[br(map = Self::from_bytes)]
#[bw(map = |&x| Self::into_bytes(x))]
pub struct Capabilities {
    #[allow(dead_code)]
    pub raw_mode: bool,
    #[allow(dead_code)]
    pub mpx_mode: bool,
    #[allow(dead_code)]
    pub unicode: bool,
    #[allow(dead_code)]
    pub large_files: bool,
    #[allow(dead_code)]
    pub nt_smb: bool,
    #[allow(dead_code)]
    pub rpc_api: bool,
    #[allow(dead_code)]
    pub nt_status: bool,
    #[allow(dead_code)]
    pub l2_oplocks: bool,
    #[allow(dead_code)]
    pub lock_and_read: bool,
    #[allow(dead_code)]
    pub nt_find: bool,
    #[allow(dead_code)]
    pub r1: B2,
    #[allow(dead_code)]
    pub distributed_file_system: bool,
    #[allow(dead_code)]
    pub info_level: bool,
    #[allow(dead_code)]
    pub large_read_x: bool,
    #[allow(dead_code)]
    pub large_write_x: bool,
    #[allow(dead_code)]
    pub lwio: bool,
    #[allow(dead_code)]
    pub r2: B6,
    #[allow(dead_code)]
    pub unix: bool,
    #[allow(dead_code)]
    pub r3: B1,
    #[allow(dead_code)]
    pub compressed: bool,
    #[allow(dead_code)]
    pub r4: B3,
    #[allow(dead_code)]
    pub dynamic_re_auth: bool,
    #[allow(dead_code)]
    pub r5: B1,
    #[allow(dead_code)]
    pub extend_security: bool,
}
