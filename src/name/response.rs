use derive_builder::Builder;
use rand::Rng;

use crate::name::{
    Flags, Header, HeaderBuilder, NBAddress, NBFlags, NameLabel, Op, OpCode, PointerLabel, Query,
    QuestionBuilder, QuestionClass, QuestionType, RCode, Record, Registration, Release,
    ResourceBuilder, ResourceClass, ResourceType,
};

#[derive(Builder, Debug, Clone)]
pub struct Response {
    pub id: u16,
    pub name: String,
    pub ttl: Option<u32>,
    pub flags: Option<NBFlags>,
    pub address: Option<NBAddress>,
    pub rcode: RCode,
    #[builder(default = "false")]
    pub end_node_challenge: bool,
}

impl Response {
    pub fn response(self) -> Result<Header, ()> {
        // Attempt to create the name label
        let name = NameLabel::new(self.name).map_err(|_| ())?;

        // Set the flags and the address
        let flags = self.flags.unwrap_or(NBFlags::new().with_group(false));
        let address = self
            .address
            .unwrap_or(NBAddress::new("192.168.1.2".parse().unwrap()));

        // Create the record
        let record = Record::new(flags, address);

        // Create the resource

        // Set the opcode to registration and indicate that this is a request.
        let opcode = OpCode::new().with_response(true).with_op(Op::Registration);

        // Set the flags depending on the type of operation being set.
        let mut hflags = Flags::new().with_broadcast(true);

        // Create the response header
        let header = HeaderBuilder::default()
            .transaction_id(self.id)
            .opcode(opcode)
            .flags(hflags)
            // TODO: Into would be a good call here in the builder
            .rcode(Query::Success.into())
            .questions(1)
            .additional(1)
            .questions_entries(vec![question])
            .additional_records(vec![resource])
            .build()
            .unwrap();

        Ok(header)
    }
}
