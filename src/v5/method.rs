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

    #[brw(magic = 0xFFu8)]
    NoAcceptableMethods = 0xFF,

    // Discriminant is arbitrary (binrw matches by magic); 0x04 avoids
    // overflowing past NoAcceptableMethods under repr(u8).
    Custom(u8) = 0x04,
}

#[cfg(test)]
mod unit {
    use binrw::{io::Cursor, BinRead};

    use super::*;

    #[test]
    fn test_no_acceptable_methods_parses() {
        let mut cursor = Cursor::new(vec![0xFFu8]);
        let method = Method::read_be(&mut cursor).expect("parses");
        assert_eq!(method, Method::NoAcceptableMethods);
    }

    #[test]
    fn test_custom_parses() {
        let mut cursor = Cursor::new(vec![0x42u8]);
        let method = Method::read_be(&mut cursor).expect("parses");
        assert_eq!(method, Method::Custom(0x42));
    }
}
