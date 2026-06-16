use bnb::wire;

#[wire(big)]
struct X<T> {
    a: T,
}

fn main() {}
