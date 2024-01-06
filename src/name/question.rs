use binrw::{binrw, io::Cursor, BinRead, BinResult, BinWrite};
use derive_builder::Builder;

use crate::name::{LType, Label};

#[binrw::parser(reader, endian)]
fn parse_labels() -> BinResult<Vec<Label>> {
    let mut vec = Vec::new();

    loop {
        let label = <Label>::read_options(reader, endian, ())?;

        match label.ltype {
            LType::Name(ref name) => {
                if name.info.length() == 0 {
                    break;
                }
                vec.push(label)
            }
            LType::Pointer(_) => vec.push(label),
            LType::Custom(_) => {
                vec.push(label);
                break;
            }
        }
    }

    Ok(vec)
}

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Question {
    #[br(parse_with = parse_labels)]
    pub name: Vec<Label>,
    pub qtype: QType,
    pub class: QClass,
}

#[binrw]
#[brw(big)]
#[repr(u16)]
#[bw(magic = 0x00u8)]
#[derive(Clone, Copy, Debug)]
pub enum QType {
    #[brw(magic = 0x0020u16)]
    NB = 0x0020,
    #[brw(magic = 0x0021u16)]
    NBSTAT = 0x0021,
    Custom(u16),
}

#[binrw]
#[brw(big)]
#[repr(u16)]
#[derive(Clone, Copy, Debug)]
pub enum QClass {
    #[brw(magic = 0x0001u16)]
    Internet = 0x0001,
    Custom(u16),
}

#[cfg(test)]
mod unit {
    use super::*;
    use crate::name::NameLabelBuilder;

    #[test]
    fn question() {
        let question = QuestionBuilder::default()
            .name(vec![
                NameLabelBuilder::default()
                    .name("YYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYY")
                    .build()
                    .unwrap()
                    .into(),
                NameLabelBuilder::default()
                    .name("NETBIOS")
                    .build()
                    .unwrap()
                    .into(),
                NameLabelBuilder::default()
                    .name("COM")
                    .build()
                    .unwrap()
                    .into(),
            ])
            .qtype(QType::NBSTAT)
            .class(QClass::Custom(45))
            .build()
            .unwrap();

        let mut buffer = Cursor::new(Vec::new());
        question.write(&mut buffer).unwrap();

        let pbuf = buffer.clone();
        println!("{:?}", pbuf.into_inner());

        let mut rbuf = Cursor::new(buffer.into_inner());

        let question = Question::read(&mut rbuf).unwrap();
        println!("{:#?}", question);
    }
}
