//! A stored `#[bw(calc = …)]` on an enum-variant field would be inert: a variant is encoded
//! as stored and an enum has no canonical form, so the expression would never even compile.
use bnb::bin;

#[bin(big)]
enum Message {
    #[bin(magic = 1u8)]
    Data {
        #[bw(calc = this_is_never_compiled())]
        len: u8,
        body: u8,
    },
}

fn main() {}
