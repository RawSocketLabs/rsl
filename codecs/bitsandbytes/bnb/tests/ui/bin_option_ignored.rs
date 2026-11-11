//! A container option that the item's kind cannot honour is an error at the option, naming
//! where it belongs, instead of being parsed and ignored.
use bnb::bin;

#[bin(big)]
struct Wire {
    a: u16,
}

struct Logical(u16);
impl From<Wire> for Logical {
    fn from(w: Wire) -> Self {
        Logical(w.a)
    }
}
impl From<&Logical> for Wire {
    fn from(l: &Logical) -> Self {
        Wire { a: l.0 }
    }
}

// The wire type's own order applies: `little` here would silently do nothing.
#[bin(little, wire = Wire)]
struct MappedOrder(u16);

// A mapped struct has no builder to suppress.
#[bin(no_builder, map = |w: Wire| MappedBuilder(w.a))]
struct MappedBuilder(u16);

// The seek check `forward_only` asks for is the wire type's to make.
#[bin(forward_only, try_wire = Wire)]
struct MappedForward(u16);

// A codec newtype has no fields for `auto_len` to target.
#[bin(codec = bnb::codecs::leb128, auto_len(a.b = count(c)))]
struct CodecAutoLen(u64);

#[bin(codec = bnb::codecs::leb128, no_builder)]
struct CodecBuilder(u64);

// Enum-level `auto_len` was parsed and dropped: the lengths stayed `Auto`.
#[bin(big, auto_len(header.len = count(items)))]
enum EnumAutoLen {
    #[bin(magic = 1u8)]
    Data { len: u8, items: u8 },
}

// An enum has no builder to suppress.
#[bin(big, no_builder)]
enum EnumBuilder {
    #[bin(magic = 1u8)]
    Only,
}

fn main() {}
