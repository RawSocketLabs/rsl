use binrw::binrw;
use derive_builder::Builder;

use crate::v5::method::Method;

#[binrw]
#[brw(big)]
#[derive(Builder, Clone, Debug)]
pub struct Identifier {
    #[builder(default = "5")]
    pub version: u8,

    #[builder(default = "None")]
    #[builder(setter(into, strip_option))]
    #[bw(map = |n| n.unwrap_or(methods.len() as u8))]
    pub number_of_methods: Option<u8>,

    #[br(count = number_of_methods.unwrap_or(0))]
    pub methods: Vec<Method>,
}

#[cfg(test)]
mod unit {
    use binrw::{io::Cursor, BinWrite};

    use super::*;

    #[test]
    fn test_identifier() {
        let id = IdentifierBuilder::default()
            .methods(vec![Method::NoAuth, Method::GssApi])
            .build()
            .unwrap();
        println!("{:?}", id);

        let mut cursor = Cursor::new(Vec::new());

        id.write(&mut cursor).unwrap();

        println!("{:?}", cursor.into_inner());
    }
}
