//! A `#[bitflags]` flag whose generated items would reuse a name the macro already
//! generates is an error at the field.
use bnb::bitflags;

// `all()` is the generated union of every flag.
#[bitflags(u8)]
#[derive(Clone, Copy)]
struct Fixed {
    all: bool,
    other: bool,
}

// Both flags want the constant `FIN`.
#[bitflags(u8)]
#[derive(Clone, Copy)]
struct SameConst {
    fin: bool,
    r#FIN: bool,
}

// The getter of `set_ack` is the `set_` mutator of `ack`.
#[bitflags(u8)]
#[derive(Clone, Copy)]
struct Sibling {
    set_ack: bool,
    ack: bool,
}

fn main() {}
