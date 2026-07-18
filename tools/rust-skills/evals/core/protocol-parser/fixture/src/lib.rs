#[derive(Debug, PartialEq)]
pub struct Packet {
    pub kind: u8,
    pub payload: Vec<u8>,
}

pub fn decode(input: &[u8]) -> Packet {
    let kind = input[0];
    let payload_len = usize::from(u16::from_be_bytes([input[1], input[2]]));
    let mut payload = Vec::with_capacity(payload_len);
    payload.extend_from_slice(&input[3..3 + payload_len]);
    Packet { kind, payload }
}
