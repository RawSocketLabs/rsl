use crate::name::{
    Flags, Header, HeaderBuilder, NBFlags, NameLabel, Op, OpCode, Query, QuestionBuilder,
    QuestionClass, QuestionType,
};

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
    override_address: Option<u32>,
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

    // let ttl = ttl_override.unwrap_or(0);
    // let flags = flags_override.unwrap_or(Flags::new());
    // let address = address_override.unwrap_or(0);
    // let record = ResourceBuilder::default().build().unwrap();

    let header = HeaderBuilder::default()
        .transacition_id(id)
        .opcode(opcode)
        .flags(flags)
        .rcode(Query::Success.into())
        .questions(1)
        .additional(1)
        .questions_entries(vec![question])
        //.additional_records(vec![record])
        .build()
        .unwrap();

    Request::new(id, header)
}
pub fn overwrite_request() -> Request {}
pub fn refresh_request() -> Request {}
pub fn registration_response() -> Request {}
pub fn challenge_registration_response() -> Header {}
pub fn conflict_request() -> Request {}
pub fn release_request() -> Request {}
pub fn query_request() -> Request {}
pub fn query_response() -> Header {}
pub fn acknowledgement_response() -> Header {}
pub fn status_request() -> Request {}
pub fn status_response() -> Header {}
