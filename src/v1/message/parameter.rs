use binrw::{binrw, BinRead, BinWrite};
use modular_bitfield::prelude::*;

#[binrw]
#[brw(little)]
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub size: u8,
    #[bw(if(*size > 0))]
    #[br(count = size * 2, if(size > 0))]
    pub param: Option<Vec<u8>>,
}

impl Parameter {
    pub fn new(param: Option<Vec<u8>>) -> Self {
        Self {
            size: match param {
                Some(ref p) => (p.len() / 2) as u8,
                None => 0,
            },
            param,
        }
    }

    pub fn encapsulate(&mut self, param: &[u8]) {
        self.size = (param.len() / 2) as u8;
        self.param = Some(param.to_vec());
    }
}

impl Default for Parameter {
    fn default() -> Self {
        Self::new(None)
    }
}

#[binrw]
#[repr(u8)]
#[brw(repr = u8)]
#[derive(Debug, PartialEq, Copy, Clone)]
pub enum AndXCommand {
    Locking = 0x24,
    Open = 0x2D,
    Read = 0x2E,
    Write = 0x2F,
    SessionSetup = 0x73,
    Logoff = 0x74,
    TreeConnect = 0x75,
    SecurityPackage = 0x7E,
    NtCreate = 0xA2,
    Final = 0xFF,
}

#[binrw]
#[brw(little)]
#[derive(Debug, Clone, PartialEq)]
pub struct AndX {
    pub command: AndXCommand,
    pub reserved: u8,
    pub offset: u16,
}

#[cfg(test)]
mod tests {
    use super::*;
    use binrw::io::Cursor;
    use binrw::BinWrite;

    #[test]
    fn empty() {
        let params = Parameter::new(None);
        let mut buffer = Cursor::new(Vec::new());
        params.write(&mut buffer).unwrap();
        assert_eq!(buffer.into_inner(), vec![0x00]);
    }

    #[test]
    fn simple() {
        let params = Parameter::new(Some(vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06]));
        let mut buffer = Cursor::new(Vec::new());
        params.write(&mut buffer).unwrap();
        assert_eq!(
            buffer.into_inner(),
            vec![0x03, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06]
        );
    }
}
