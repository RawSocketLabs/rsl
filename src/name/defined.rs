use crate::name::{
    Flags, Header, HeaderBuilder, NBFlags, NameLabel, Op, OpCode, PointerLabel, Query,
    QuestionBuilder, QuestionClass, QuestionType, Record, ResourceBuilder, ResourceClass,
    ResourceType,
};

use super::NBAddress;

pub struct Request {
    pub id: u16,
    pub header: Header,
}

impl Request {
    pub fn new(id: u16, header: Header) -> Request {
        Request { id, header }
    }
}

pub fn registration_request(
    question: String,
    override_ttl: Option<u32>,
    override_flags: Option<NBFlags>,
    override_address: Option<NBAddress>,
) -> Request {
    // Randomly generate a unique transaction id.
    let id = 0x1234;

    // Set the opcode to registration and indicate that this is a request.
    let opcode = OpCode::new().with_response(false).with_op(Op::Registration);

    // Set the flags to broadcast and recursion desired.
    let flags = Flags::new()
        .with_broadcast(true)
        .with_recursion_desired(true);

    // Set the question to the name we want to register.
    let question = QuestionBuilder::default()
        .name(vec![NameLabel::new(question).unwrap().into()])
        .qtype(QuestionType::NB)
        .class(QuestionClass::Internet)
        .build()
        .unwrap();

    let ttl = override_ttl.unwrap_or(0);
    // TODO: Confirm the default flag settings?
    // let nb_flags = flags_override.unwrap_or(Flags::new());
    // let nb_address = address_override.unwrap_or(0);

    let nb_flags = NBFlags::new().with_group(false);

    let nb_address = NBAddress::new("192.168.1.2".parse().unwrap());

    let record = Record::new(nb_flags, nb_address);

    let resource = ResourceBuilder::default()
        // TODO: Fix the offset this will take some calculation based on input information.
        .name(vec![PointerLabel::from_offset(0).unwrap().into()])
        .rtype(ResourceType::NB)
        .rclass(ResourceClass::Internet)
        .ttl(ttl)
        .length(6)
        .data(record.into())
        .build()
        .unwrap();

    let header = HeaderBuilder::default()
        .transacition_id(id)
        .opcode(opcode)
        .flags(flags)
        .rcode(Query::Success.into())
        .questions(1)
        .additional(1)
        .questions_entries(vec![question])
        .additional_records(vec![resource])
        .build()
        .unwrap();

    Request::new(id, header)
}
//pub fn overwrite_request() -> Request {}
//pub fn refresh_request() -> Request {}
//pub fn registration_response() -> Request {}
//pub fn challenge_registration_response() -> Header {}
//pub fn conflict_request() -> Request {}
//pub fn release_request() -> Request {}
//pub fn query_request() -> Request {}
//pub fn query_response() -> Header {}
//pub fn acknowledgement_response() -> Header {}
//pub fn status_request() -> Request {}
//pub fn status_response() -> Header {}
