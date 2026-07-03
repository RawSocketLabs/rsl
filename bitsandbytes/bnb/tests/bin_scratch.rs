//! `Sink::scratch` — type-erased, encode-scoped shared state threaded through a whole
//! message's fields. The mechanism behind back-reference codecs (DNS name compression):
//! one `BitWriter::with_scratch` value is visible to every field's encoder because the
//! sink is the single `&mut` passed through them all.

mod macro_ {
    use bnb::bitstream::BitError;
    use bnb::{BitEncode, BitReader, BitWriter, Sink, Source, bin};
    use std::collections::HashMap;

    // A field codec that adds each written value into a `u32` accumulator in the scratch
    // (if present), then writes the value verbatim.
    fn tally_read<S: Source>(r: &mut S) -> Result<u8, BitError> {
        r.read()
    }
    fn tally_write<K: Sink>(v: &u8, w: &mut K) -> Result<(), BitError> {
        if let Some(acc) = w.scratch().and_then(|s| s.downcast_mut::<u32>()) {
            *acc += u32::from(*v);
        }
        w.write(*v)
    }

    #[bin(big)]
    #[derive(Debug, PartialEq)]
    struct Pair {
        #[br(parse_with = tally_read)]
        #[bw(write_with = tally_write)]
        a: u8,
        #[br(parse_with = tally_read)]
        #[bw(write_with = tally_write)]
        b: u8,
    }

    #[test]
    fn scratch_threads_and_accumulates_across_fields() {
        let mut w = BitWriter::new().with_scratch(Box::new(0u32));
        Pair { a: 3, b: 4 }.bit_encode(&mut w).unwrap();
        // Both field encoders saw the same scratch: 3 + 4.
        let acc = *w.scratch().unwrap().downcast_ref::<u32>().unwrap();
        assert_eq!(acc, 7);
    }

    #[test]
    fn no_scratch_sink_encodes_normally() {
        // The default sink has no scratch — the codec's tally is a no-op, and the bytes
        // are still written (backward compatible).
        assert_eq!(Pair { a: 3, b: 4 }.to_bytes().unwrap(), [3, 4]);
    }

    #[test]
    fn downcast_to_the_wrong_type_is_none() {
        let mut w = BitWriter::new().with_scratch(Box::new(0u32));
        assert!(w.scratch().unwrap().downcast_mut::<u64>().is_none());
        assert!(w.scratch().unwrap().downcast_mut::<u32>().is_some());
    }

    // A back-reference codec mirroring the DNS shape: the first time a token is written it
    // records its byte offset; a repeat emits a 0xFF marker + the prior offset instead.
    #[derive(Default)]
    struct BackrefDict(HashMap<u8, u16>);

    fn token_write<K: Sink>(tok: &u8, w: &mut K) -> Result<(), BitError> {
        let offset = (w.bit_pos() / 8) as u16;
        let prior = {
            match w.scratch().and_then(|s| s.downcast_mut::<BackrefDict>()) {
                Some(d) => match d.0.get(tok).copied() {
                    Some(off) => Some(off),
                    None => {
                        d.0.insert(*tok, offset);
                        None
                    }
                },
                None => None,
            }
        };
        match prior {
            Some(off) => {
                w.write(0xFFu8)?; // back-reference marker
                w.write(off)
            }
            None => w.write(*tok),
        }
    }

    #[bin(big)]
    struct Tokens {
        #[bw(write_with = token_write)]
        #[br(parse_with = tally_read)]
        x: u8,
        #[bw(write_with = token_write)]
        #[br(parse_with = tally_read)]
        y: u8,
        #[bw(write_with = token_write)]
        #[br(parse_with = tally_read)]
        z: u8,
    }

    #[test]
    fn back_reference_emits_a_pointer_on_repeat() {
        let mut w = BitWriter::new().with_scratch(Box::new(BackrefDict::default()));
        // x=0xAA (new, offset 0), y=0xBB (new, offset 1), z=0xAA (repeat → marker+offset 0).
        Tokens {
            x: 0xAA,
            y: 0xBB,
            z: 0xAA,
        }
        .bit_encode(&mut w)
        .unwrap();
        assert_eq!(w.into_bytes(), [0xAA, 0xBB, 0xFF, 0x00, 0x00]);
    }

    #[test]
    fn clone_starts_a_fresh_scratchless_session() {
        let w = BitWriter::new().with_scratch(Box::new(0u32));
        let mut cloned = w.clone();
        assert!(cloned.scratch().is_none()); // a clone does not carry the scratch
    }
}
