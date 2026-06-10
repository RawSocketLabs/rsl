use binrw::binrw;

#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Command {
    #[brw(magic = 0x01u8)]
    Connect = 0x01,

    #[brw(magic = 0x02u8)]
    Bind = 0x02,

    #[brw(magic = 0x03u8)]
    UdpAssociate = 0x03,

    Custom(u8),
}
