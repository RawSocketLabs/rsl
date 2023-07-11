use binrw::binrw;

#[binrw]
#[brw(little)]
#[derive(Debug, Clone, PartialEq)]
pub struct Data {
    pub size: u16,
    #[bw(if(*size > 0))]
    #[br(if(size >0), count = size)]
    pub data: Option<Vec<u8>>,
}

impl Data {
    pub fn new(data: Option<Vec<u8>>) -> Self {
        Self {
            size: match &data {
                Some(data) => data.len() as u16,
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
    fn simple() {
        let data = Data::new(Some(vec![0x01, 0x02, 0x03]));
        let mut buffer = Cursor::new(Vec::new());
        data.write(&mut buffer).unwrap();
        assert_eq!(buffer.into_inner(), vec![0x03, 0x00, 0x01, 0x02, 0x03]);
    }
}
