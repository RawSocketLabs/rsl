use binrw::{binrw, io::Cursor, BinRead, BinResult, BinWrite};
use modular_bitfield::prelude::*;

/// A DNS Label, used in NetBIOS for questions and resources records.
///
/// A label can be one of several types of labels, which are defined by the first byte of the label.
///
/// The first byte is split into two parts, the first 6 bits are used to define the length of the
/// label, and the last 2 bits are used to define the type of label.
///
/// Two of the possible label types are defined by the RFC
/// - NetBIOS Name Label
/// - Pointer Label
///
/// The other two label types are reserved for future use. These labels are represented by the
/// `Custom` variant of the `LabelType` enum.
#[binrw]
#[brw(big)]
#[derive(Clone, Debug)]
pub struct Label {
    /// The first byte of the label, which defines the label type.
    ///
    /// This byte is read twice during parsing, once to determine the label type, and again to
    /// construct the type of label that was determined during the first read.
    #[bw(ignore)]
    #[br(restore_position, temp)]
    pub(crate) check: FirstLabelByte,

    /// The type of label that was determined by the first byte.
    ///
    /// The first byte is read and used to determine the underlying type of label, which is then
    /// read and parsed into the appropriate type.
    #[br(args(check))]
    pub ltype: LabelType,
}

impl Label {
    /// Convert a label into a vector of bytes.
    pub fn into_bytes(self) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        self.write(&mut buffer).unwrap();
        buffer.into_inner()
    }

    // TODO: Determine if these methods should be here or in From/Into blocks?
    // TODO: Should this instead be from stream/cursor?
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

/// The first byte of a label is used to determine the type of label.
///
/// The first 6 bits of the label are indeterminate because we do not yet know the type of label.
/// The last 2 bits are used to determine the type of label.
#[bitfield]
#[derive(BinRead, Clone, Copy, BinWrite, Debug, Default)]
pub struct FirstLabelByte {
    pub indeterminate: B6,
    pub marker: Marker,
}

/// The types of labels.
///
/// There are two defined label types, and two reserved label types. The two defined label types
/// are NetBIOS Name Labels and Pointer Labels. The two reserved label types are reserved for future
/// use.
///
/// In this crate, the two reserved label types are represented by the `Custom` variant. Since the
/// reserved labels have no defined way of being parsed by the RFC this crate simply stores the
/// first byte of a label that uses the reserved components and will stop parsing a list of labels
/// upon encountering a reserved label.
#[derive(BinRead, BinWrite, Clone, Debug)]
#[br(big, import(check: FirstLabelByte))]
#[bw(big)]
pub enum LabelType {
    /// A NetBIOS Name Label.
    ///
    /// A NetBIOS Name Label has a length encoded in the first 6 bits of the first byte. This is
    /// used to read the rest of the label.
    #[br(pre_assert(check.marker() == Marker::NetbiosName))]
    Name(NameLabel),

    /// A Pointer Label.
    ///
    /// A Pointer Label is used to point to another label. The pointer label marked by the first
    /// two bits and is followed by a 14-bit offset to the label that it points to.
    #[br(pre_assert(check.marker() == Marker::StringPtr))]
    Pointer(PointerLabel),

    /// A reserved label type.
    ///
    /// This label type is reserved for future use. This crate will simply store the first byte of
    /// the label and stop parsing a list of labels upon encountering a reserved label.
    #[br(pre_assert(check.marker() == Marker::ReservedOne || check.marker() == Marker::ReservedTwo))]
    Custom(FirstLabelByte),
}

/// A NetBIOS Name Label.
///
/// A NetBIOS Name Label is marked by the first two bits of the first byte being set to `0b11`. The
/// remaining 6 bits are used to determine the length of the label.
///
/// The `NameInfo` struct is used to store the first byte of the label, which contains the marker
/// and length information.
///
/// The `name` field is used to store the rest of the label. The length set in the label is how
/// many bytes are read into the `name` field.
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
            ltype: LabelType::Name(name_label),
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

/// A Pointer Label.
///
/// A Pointer Label is marked by the first two bits of the first byte being set to `0b11`. The
/// remaining 14 bits are used to determine the offset to the label that it points to.
#[bitfield]
#[derive(BinRead, Clone, Copy, BinWrite, Debug)]
pub struct PointerLabel {
    pub offset: B14,
    pub marker: Marker,
}

/// The values of the 2 bits that determine the type of label.
#[derive(BitfieldSpecifier, PartialEq, Eq, Clone, Copy, Debug)]
#[bits = 2]
pub enum Marker {
    /// Indicates the label is a NetBIOS Name Label.
    NetbiosName = 0x00,

    /// Reserved for future use. This maps to a custom label while using this crate.
    ReservedOne = 0x01,

    /// Reserved for future use. This maps to a custom label while using this crate.
    ReservedTwo = 0x02,

    /// Indicates the label is a Pointer Label.
    StringPtr = 0x03,
}

/// A custom label parser.
///
/// This parser will read a list of labels until it encounters either:
/// - A label with a length of `0`.
/// - A custom label type. A custom label is any label that has a [Marker](crate::name::Marker) of `ReservedOne` or `ReservedTwo`.
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
