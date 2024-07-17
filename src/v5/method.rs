use binrw::binrw;

#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Method {
    #[brw(magic = 0x00u8)]
    NoAuth = 0x00,

    #[brw(magic = 0x01u8)]
    GssApi = 0x01,

    #[brw(magic = 0x02u8)]
    Plain = 0x02,

    #[brw(magic = 0x03u8)]
    IanaReserved = 0x03,

    #[brw(magic = 0x80u8)]
    PrivateMethods = 0x80,

    Custom(u8),

    #[brw(magic = 0xFFu8)]
    NoAcceptableMethods = 0xFF,
}

#[cfg(test)]
mod unit {}
