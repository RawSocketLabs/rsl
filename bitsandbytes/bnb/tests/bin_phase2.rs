//! Phase 2 ergonomics: `ctx` is **decode-only** (a `#[bin(ctx(...))]` type gets a plain
//! `BitEncode`/`to_bytes` unless its write side actually reads a ctx param), the generated
//! `…Ctx` struct has a positional `new`, and a variant `Vec` field can carry per-element
//! `ctx`.

mod macro_ {

    use bnb::{BitError, Sink, Source, bin};

    // ── A ctx type whose encode does NOT read ctx → plain encode (no `to_bytes_with`). ──
    #[bin(big, ctx(len: u8))]
    #[derive(Debug, PartialEq)]
    struct Sized {
        #[br(count = len)] // decode-only: encode iterates `data`
        data: Vec<u8>,
    }

    #[test]
    fn ctx_decode_only_means_plain_encode() {
        let s = Sized {
            data: vec![1, 2, 3],
        };
        // Encode is plain — no context needed on the write path.
        assert_eq!(s.to_bytes().unwrap(), [1, 2, 3]);
        // Decode still needs the context; build it positionally with `new`.
        assert_eq!(
            Sized::decode_with_exact(&[1, 2, 3], SizedCtx::new(3)).unwrap(),
            s
        );
    }

    // ── A ctx type whose encode DOES read ctx (a keyed transform) keeps `encode_with`. ──
    #[bin(big, ctx(key: u8))]
    #[derive(Debug, PartialEq)]
    struct Masked {
        #[br(map = |raw: u8| raw ^ key)] // decode: unmask with the session key
        #[bw(map = |v: &u8| *v ^ key)] // encode: mask — the wire value depends on `key`
        value: u8,
    }

    #[test]
    fn encode_that_reads_ctx_keeps_encode_with() {
        let m = Masked { value: 0x0F };
        let bytes = m.to_bytes_with(MaskedCtx::new(0xFF)).unwrap();
        assert_eq!(bytes, [0xF0]); // 0x0F ^ 0xFF
        assert_eq!(
            Masked::decode_with_exact(&bytes, MaskedCtx::new(0xFF)).unwrap(),
            m
        );
    }

    // A ctx-reading encode via a `write_with` *custom writer* (a capturing closure) — the body
    // scan keeps `encode_with`, where a `calc`/`map`-only check would miss it and miscompile.
    fn unmask<S: Source>(r: &mut S, key: u8) -> Result<u8, BitError> {
        Ok(r.read::<u8>()? ^ key)
    }

    #[bin(big, ctx(key: u8))]
    #[derive(Debug, PartialEq)]
    struct Custom {
        #[br(parse_with = |r| unmask(r, key))]
        #[bw(write_with = |v: &u8, w: &mut _| Sink::write(w, *v ^ key))]
        value: u8,
    }

    #[test]
    fn encode_reading_ctx_via_write_with() {
        let c = Custom { value: 0x0F };
        assert_eq!(c.to_bytes_with(CustomCtx::new(0xFF)).unwrap(), [0xF0]);
        assert_eq!(
            Custom::decode_with_exact(&[0xF0], CustomCtx::new(0xFF)).unwrap(),
            c
        );
    }

    // ── Per-variant `ctx` on a `Vec` field: each element is a ctx-message taking a sibling. ──
    #[bin(big, ctx(width: u8))]
    #[derive(Debug, PartialEq)]
    struct Cell {
        #[br(count = width)]
        bytes: Vec<u8>,
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    enum Grid {
        #[bin(magic = 1u8)]
        Rows {
            width: u8,
            #[br(temp)]
            #[bw(calc = cells.len() as u8)]
            count: u8,
            #[br(count = count, ctx { width })] // hand `width` to every Cell
            cells: Vec<Cell>,
        },
    }

    #[test]
    fn per_variant_ctx_on_a_vec_field() {
        let g = Grid::Rows {
            width: 2,
            cells: vec![
                Cell {
                    bytes: vec![0xAA, 0xBB],
                },
                Cell {
                    bytes: vec![0xCC, 0xDD],
                },
            ],
        };
        let bytes = [0x01, 0x02, 0x02, 0xAA, 0xBB, 0xCC, 0xDD]; // magic, width, count, 2 cells
        assert_eq!(g.to_bytes().unwrap(), bytes);
        assert_eq!(Grid::decode_exact(&bytes).unwrap(), g);
    }
}
