use binrw::binrw;

use crate::v1::cmd::*;
use crate::v1::message::header::Command;

#[binrw]
#[brw(little)]
#[br(import(cmd: Command))]
#[derive(Debug, Clone)]
pub struct Data {
    pub size: u16,
    #[bw(if(*size > 0))]
    #[br(if(size > 0), args(cmd, size))]
    pub data: Option<DataType>,
}

impl Data {
    pub fn new(data: Option<DataType>) -> Self {
        Self {
            size: match &data {
                Some(dt) => dt.len(),
                None => 0,
            },
            data,
        }
    }
}

impl Default for Data {
    fn default() -> Self {
        Self::new(None)
    }
}

#[binrw]
#[brw(little)]
#[br(import(cmd: Command, size: u16))]
#[derive(Debug, Clone)]
pub enum DataType {
    NegotiateRequest(NegotiateRequest),
    #[br(pre_assert(cmd == Command::Negotiate))]
    NegotiateResponse(NegotiateResponse),
    Unspecified(#[br(count = size)] Vec<u8>),
}

impl DataType {
    pub fn len(&self) -> u16 {
        match self {
            DataType::NegotiateRequest(data) => data.len(),
            DataType::NegotiateResponse(data) => data.len(),
            _ => panic!("Unsupported data type"),
        }
    }
}

pub trait DataLength {
    fn len(&self) -> u16;
}

#[cfg(test)]
mod tests {
    use super::*;
    use binrw::io::Cursor;
    use binrw::BinWrite;

    #[test]
    fn empty() {
        let data = Data::new(None);
        let mut buffer = Cursor::new(Vec::new());
        data.write(&mut buffer).unwrap();
        assert_eq!(buffer.into_inner(), vec![0x00, 0x00]);
    }

    #[test]
    fn simple_request() {
        let request = Data::new(Some(
            DataType::NegotiateRequest(NegotiateRequest::default()),
        ));
        let mut buffer = Cursor::new(Vec::new());
        request.write(&mut buffer).unwrap();
        assert_eq!(
            buffer.into_inner(),
            vec![12, 0, 2, 78, 84, 32, 76, 77, 32, 48, 46, 49, 50, 0]
        );
    }
}
