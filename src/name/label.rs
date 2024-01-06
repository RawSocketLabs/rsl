use binrw::{binrw, io::Cursor, BinRead, BinResult, BinWrite};
use modular_bitfield::prelude::*;

#[binrw]
#[brw(big)]
#[derive(Clone, Debug)]
pub struct Label {
    #[bw(ignore)]
    #[br(restore_position, temp)]
    pub(crate) check: FirstLabelByte,

    #[br(args(check))]
    pub ltype: LabelType,
}

impl Label {
    pub fn into_bytes(self) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        self.write(&mut buffer).unwrap();
        buffer.into_inner()
    }

    // TODO: Determine if these methods should be here or in From/Into blocks?
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut cursor = Cursor::new(bytes);
        Self::read(&mut cursor).unwrap()
    }
}

impl TryFrom<Label> for NameLabel {
    type Error = Box<dyn std::error::Error>;

    fn try_from(label: Label) -> Result<Self, Self::Error> {
        match label.ltype {
            LabelType::Name(name_label) => Ok(name_label),
            _ => Err("Label is not a NameLabel".into()),
        }
    }
}

impl TryFrom<Label> for PointerLabel {
    type Error = Box<dyn std::error::Error>;

    fn try_from(label: Label) -> Result<Self, Self::Error> {
        match label.ltype {
            LabelType::Pointer(pointer_label) => Ok(pointer_label),
            _ => Err("Label is not a PointerLabel".into()),
        }
    }
}

#[bitfield]
#[derive(BinRead, Clone, Copy, BinWrite, Debug, Default)]
pub struct FirstLabelByte {
    pub indeterminate: B6,
    pub marker: Marker,
}

#[derive(BinRead, BinWrite, Clone, Debug)]
#[br(big, import(check: FirstLabelByte))]
#[bw(big)]
pub enum LabelType {
    #[br(pre_assert(check.marker() == Marker::NetbiosName))]
    Name(NameLabel),

    #[br(pre_assert(check.marker() == Marker::StringPtr))]
    Pointer(PointerLabel),

    // TODO: We don't know how to parse these...
    #[br(pre_assert(check.marker() == Marker::ReservedOne || check.marker() == Marker::ReservedTwo))]
    Custom(FirstLabelByte),
}

#[binrw]
#[brw(big)]
#[derive(Clone, Debug)]
pub struct NameLabel {
    pub info: NameInfo,

    #[br(count = info.length(), if(info.length() > 0))]
    pub name: Vec<u8>,
}

impl NameLabel {
    pub fn new(name: String) -> Result<NameLabel, Box<dyn std::error::Error>> {
        let info = name.as_bytes().try_into()?;
        let name = name.into_bytes();
        Ok(NameLabel { info, name })
    }

    pub fn set_length(&mut self, len: u8) -> Result<(), Box<dyn std::error::Error>> {
        if len > 0x3F {
            return Err("Length exceeds the 6-bit field".into());
        }

        Ok(self.info.set_length(len))
    }

    pub fn into_bytes(self) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        self.write(&mut buffer).unwrap();
        buffer.into_inner()
    }

    // TODO: Make a from_stream | from_cursor method?
}

impl From<NameLabel> for Label {
    fn from(name_label: NameLabel) -> Self {
        Self {
            check: FirstLabelByte::new(),
            ltype: LType::Name(name_label),
        }
    }
}

#[bitfield]
#[derive(BinRead, Clone, Copy, BinWrite, Debug, Default)]
pub struct NameInfo {
    pub length: B6,
    pub marker: Marker,
}

impl TryFrom<&[u8]> for NameInfo {
    type Error = Box<dyn std::error::Error>;

    fn try_from(name: &[u8]) -> Result<Self, Self::Error> {
        let length = name.len() as u8;
        if length > 0x3F {
            return Err("Name too long".into());
        }

        Ok(Self::new()
            .with_marker(Marker::NetbiosName)
            .with_length(length))
    }
}

#[bitfield]
#[derive(BinRead, Clone, Copy, BinWrite, Debug)]
pub struct PointerLabel {
    pub offset: B14,
    pub marker: Marker,
}

#[derive(BitfieldSpecifier, PartialEq, Eq, Clone, Copy, Debug)]
#[bits = 2]
pub enum Marker {
    NetbiosName = 0x00,
    ReservedOne = 0x01,
    ReservedTwo = 0x02,
    StringPtr = 0x03,
}

#[binrw::parser(reader, endian)]
pub fn parse_labels() -> BinResult<Vec<Label>> {
    let mut vec = Vec::new();

    loop {
        let label = <Label>::read_options(reader, endian, ())?;

        match label.ltype {
            LabelType::Name(ref name) => {
                if name.info.length() == 0 {
                    break;
                }
                vec.push(label)
            }
            LabelType::Pointer(_) => vec.push(label),
            LabelType::Custom(_) => {
                vec.push(label);
                break;
            }
        }
    }

    Ok(vec)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn label() {
        let label = NameLabel::new("TESTLABEL".into()).unwrap();

        assert_eq!(label.info.marker(), Marker::NetbiosName);
        assert_eq!(label.info.length(), 0x09);
        assert_eq!(label.name, "TESTLABEL".to_string().into_bytes());
    }

    #[test]
    fn label_bytes() {
        let name_label = NameLabel::new("YYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYY".into()).unwrap();

        let label: Label = name_label.into();
        let bytes = label.into_bytes();
        println!("{:?}", bytes);
    }
}
