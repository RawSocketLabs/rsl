//! **ctx** — context-driven parsing: a tagged-union `Body` whose variant is chosen by an
//! **off-wire** selector (`tag`, threaded down via `ctx`), and a parent that recomputes that
//! selector from the chosen variant so the two never drift. The "a header field decides how the
//! body parses" orchestration — where the discriminant lives apart from the payload.
//!
//! Run with: `cargo run -p bitsandbytes --example ctx`

use bnb::bin;

/// The payload. Its discriminant (`kind`) is **not** part of this type's bytes — the parent
/// carries it and passes it down as context; each variant declares which `kind` selects it.
#[bin(big, ctx(kind: u8), tag = kind)]
#[derive(Debug, PartialEq, Clone)]
enum Body {
    #[bin(tag = 1)]
    Login { user_id: u32 },
    #[bin(tag = 2)]
    Chat {
        #[br(temp)]
        #[bw(calc = text.len() as u8)]
        len: u8,
        #[br(count = len)]
        #[try_str]
        text: Vec<u8>,
    },
    #[bin(tag = 3)]
    Ping,
}

#[bin(big)]
#[derive(Debug, PartialEq, Clone)]
struct Packet {
    // `kind` is written from the body (so it can't drift) and read as a throwaway local that
    // selects the variant — it's on the wire once, but isn't a stored field.
    #[br(temp)]
    #[bw(calc = self.body.tag())]
    kind: u8,
    seq: u16,
    #[br(ctx { kind })]
    body: Body,
}

fn main() {
    for body in [
        Body::Login { user_id: 0xCAFE },
        Body::Chat {
            text: b"hi there".to_vec(),
        },
        Body::Ping,
    ] {
        let pkt = Packet {
            seq: 1,
            body: body.clone(),
        };
        let bytes = pkt.to_bytes().unwrap();
        println!("kind {} -> {} bytes  {bytes:02x?}", body.tag(), bytes.len());
        assert_eq!(Packet::decode_exact(&bytes).unwrap(), pkt);
    }

    // Show one fully decoded.
    let pkt = Packet::decode_exact(
        &Packet {
            seq: 9,
            body: Body::Login { user_id: 0xCAFE },
        }
        .to_bytes()
        .unwrap(),
    )
    .unwrap();
    println!("{pkt:#?}");
    println!("all checks passed");
}
