//! Strict DER decoding and typed accessors fed arbitrary bytes must never panic or read beyond
//! the declared element. Accepted input must retain its exact original span.
//!
//! Run: `cargo +nightly fuzz run der_decode --fuzz-dir pki/fuzz`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use rsl_asn1::{Decoder, Encoder, Tag, decode_exact};

fuzz_target!(|data: &[u8]| {
    if let Ok(element) = decode_exact(data) {
        assert_eq!(element.encoded(), data);

        if element.tag().constructed {
            let mut children = element.children().unwrap();
            while !children.is_finished() {
                let before = children.position();
                let child = children.read().unwrap();
                assert!(children.position() > before);
                assert!(!child.encoded().is_empty());
            }
            children.finish().unwrap();
        } else if element.tag() == Tag::BOOLEAN {
            let _ = element.boolean().unwrap();
        } else if element.tag() == Tag::INTEGER {
            let _ = element.unsigned_bytes();
            let _ = element.unsigned_u64();
        } else if element.tag() == Tag::BIT_STRING {
            let bits = element.bit_string().unwrap();
            let _ = bits.bit(bits.bit_len());
        } else if element.tag() == Tag::OBJECT_IDENTIFIER {
            let _ = element.object_identifier().unwrap();
        } else if element.tag() == Tag::UTF8_STRING {
            let _ = element.utf8_string().unwrap();
        }

        let mut copied = Encoder::new();
        copied.encoded(data).unwrap();
        assert_eq!(copied.finish(), data);
    }

    let mut region = Decoder::new(data);
    while !region.is_finished() {
        let before = region.position();
        if region.read().is_err() {
            break;
        }
        assert!(region.position() > before);
    }
});
