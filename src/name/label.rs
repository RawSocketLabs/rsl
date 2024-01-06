use binrw::{binrw, io::Cursor, BinRead, BinWrite};
use derive_builder::Builder;
use modular_bitfield::prelude::*;

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Label {
    #[builder(setter(skip))]
    #[br(restore_position)]
    #[bw(ignore)]
    pub(crate) check: FirstByte,

    #[br(args(check))]
    pub ltype: LType,
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

#[bitfield]
#[derive(BinRead, Clone, Copy, BinWrite, Debug, Default)]
pub struct FirstByte {
    pub offset: B6,
    pub marker: Marker,
}

#[derive(BinRead, BinWrite, Clone, Debug)]
#[br(big, import(check: FirstByte))]
#[bw(big)]
pub enum LType {
    #[br(pre_assert(check.marker() == Marker::NetbiosName))]
    Name(NameLabel),

    #[br(pre_assert(check.marker() == Marker::StringPtr))]
    Pointer(PointerLabel),

    // TODO: We don't know how to parse these...
    #[br(pre_assert(check.marker() == Marker::ReservedOne || check.marker() == Marker::ReservedTwo))]
    Custom(FirstByte),
}

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
#[builder(build_fn(skip))]
pub struct NameLabel {
    #[builder(setter(skip))]
    pub info: NameInfo,

    #[br(count = info.length(), if(info.length() > 0))]
    #[builder(setter(into))]
    pub name: Vec<u8>,
}

impl NameLabelBuilder {
    pub fn build(&self) -> Result<NameLabel, Box<dyn std::error::Error>> {
        let name = self.name.clone().unwrap();
        let info = NameInfo::try_from_name(&name)?;
        Ok(NameLabel { info, name })
    }
}

impl From<NameLabel> for Label {
    fn from(name_label: NameLabel) -> Self {
        LabelBuilder::default()
            .ltype(LType::Name(name_label))
            .build()
            .unwrap()
    }
}

#[bitfield]
#[derive(BinRead, Clone, Copy, BinWrite, Debug, Default)]
pub struct NameInfo {
    pub length: B6,
    pub marker: Marker,
}

impl NameInfo {
    pub fn try_from_name<N: AsRef<[u8]>>(name: N) -> Result<Self, Box<dyn std::error::Error>> {
        if name.as_ref().len() > 0x3F {
            return Err("Name too long".into());
        }

        Ok(Self::new()
            .with_marker(Marker::NetbiosName)
            .with_length(name.as_ref().len() as u8))
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

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn label() {
        let label = NameLabelBuilder::default()
            .name("TESTLABEL")
            .build()
            .unwrap();

        assert_eq!(label.info.marker(), Marker::NetbiosName);
        assert_eq!(label.info.length(), 0x09);
        assert_eq!(label.name, "TESTLABEL".to_string().into_bytes());
    }

    #[test]
    fn label_bytes() {
        let name_label = NameLabelBuilder::default()
            .name("YYYYYYYYYYYYYYYYYYYYYYYYYYYYYYYY")
            .build()
            .unwrap();

        let label: Label = name_label.into();
        let bytes = label.into_bytes();
        println!("{:?}", bytes);
    }
}
