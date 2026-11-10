use bnb::{bin, u6};

type Opcode = u6;

#[bin(ctx(fid: u8, code: Opcode))]
enum Message {
    #[bin(tag(fid = 0x00, code = 0x40))]
    Invalid(u8),
}

fn main() {
    let _ = Message::decode_with_exact(&[0], MessageCtx { fid: 0, code: u6::new(0) });
}
