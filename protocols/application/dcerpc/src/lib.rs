pub struct Version {
    pub major: u8,
    pub minor: u8,
}

pub enum PacketType {
    Request = 0,
    Ping = 1,
    Response = 2,
    Fault = 3,
    Working = 4,
    NoCall = 5,
    Reject = 6,
    Acknowledge = 7,
    ConnectionlessCancel = 8,
    FragAck = 9,
    CancelAck = 10,
    Bind = 11,
    BindAck = 12,
    BindNak = 13,
    AlterContext = 14,
    AlterContextResp = 15,
    Auth3 = 16,
    Shutdown = 17,
    CoCancel = 18,
    Orphaned = 19,
}

pub struct Flags {
    pub first_frag: bool,
    pub last_frag: bool,
    pub cancel_pending: bool,
    pub reserved: bool,
    pub maybe: bool,
}

pub enum NDRType {
    NDR,
    NDR64,
}

pub struct Header {
    pub version: Version,
    pub ptype: PacketType,
    pub flags: Flags,
    pub ndr_type: NDRType,
    pub frag_len: u16,
    pub auth_len: u16,
    pub call_id: u32,
}

pub struct Message {
    pub header: Header,
    pub auth_verifier: Vec<u8>,
    pub stub_data: Vec<u8>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {}
}
