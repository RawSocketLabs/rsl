use binrw::binrw;

#[repr(u8)]
#[binrw]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Response {
    #[brw(magic = 0x00u8)]
    Succeeded = 0x00,

    #[brw(magic = 0x01u8)]
    GeneralFailure = 0x01,

    #[brw(magic = 0x02u8)]
    ConnectionNotAllowed = 0x02,

    #[brw(magic = 0x03u8)]
    NetworkUnreachable = 0x03,

    #[brw(magic = 0x04u8)]
    HostUnreachable = 0x04,

    #[brw(magic = 0x05u8)]
    ConnectionRefused = 0x05,

    #[brw(magic = 0x06u8)]
    TtlExpired = 0x06,

    #[brw(magic = 0x07u8)]
    CommandNotSupported = 0x07,

    #[brw(magic = 0x08u8)]
    AddressTypeNotSupported = 0x08,

    Custom(u8),
}
