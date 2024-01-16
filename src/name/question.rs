use binrw::binrw;
use derive_builder::Builder;

use crate::name::label::{parse_labels, Label};

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Question {
    #[br(parse_with = parse_labels)]
    pub name: Vec<Label>,
    pub qtype: QuestionType,
    pub class: QuestionClass,
}

#[binrw]
#[brw(big)]
#[repr(u16)]
#[bw(magic = 0x00u8)]
#[derive(Clone, Copy, Debug)]
pub enum QuestionType {
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
pub enum QuestionClass {
    #[brw(magic = 0x0001u16)]
    Internet = 0x0001,
    Custom(u16),
}

#[cfg(test)]
mod unit {
    use super::*;
    use binrw::{io::Cursor, BinRead, BinWrite};

    use crate::name::label::NameLabel;

    #[test]
    fn question() {
        let question = QuestionBuilder::default()
            .name(vec![
                NameLabel::new("YYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYY".into())
                    .unwrap()
                    .into(),
                NameLabel::new("NETBIOS".into()).unwrap().into(),
                NameLabel::new("COM".into()).unwrap().into(),
            ])
            .qtype(QuestionType::NBSTAT)
            .class(QuestionClass::Custom(45))
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
