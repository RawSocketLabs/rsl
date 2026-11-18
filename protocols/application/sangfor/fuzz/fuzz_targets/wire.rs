//! Every wire type (spec §7 and §9), fed bytes a node chose, must never panic; a value that decodes must
//! encode back to exactly the bytes it took, decode the same from those bytes alone, and
//! not have been decodable from one byte fewer.
//!
//! Run: `cargo +nightly fuzz run wire --fuzz-dir protocols/application/sangfor/fuzz`.
#![no_main]

use libfuzzer_sys::fuzz_target;
use sangfor_atrust_wire::{
    AuthReply, MethodReply, Negotiation, TunnelRequest, VipAddresses, VipHead,
    tcp::{ConnectReply, FrameHeader, ReplyHead},
};

macro_rules! settles {
    ($type:ty, $wire:expr) => {{
        let wire: &[u8] = $wire;
        if let Ok((value, consumed)) = <$type>::decode_prefix(wire) {
            assert_eq!(
                value.to_bytes().unwrap(),
                wire[..consumed],
                "{}: a decoded value must encode to the bytes it took",
                stringify!($type)
            );
            let (again, consumed_again) = <$type>::decode_prefix(&wire[..consumed]).unwrap();
            assert_eq!(
                (&again, consumed_again),
                (&value, consumed),
                "{}: bytes after a value must not change it",
                stringify!($type)
            );
            if let Some(shorter) = consumed.checked_sub(1) {
                let early = <$type>::decode_prefix(&wire[..shorter]);
                assert!(
                    early.is_err_and(|error| error.is_incomplete()),
                    "{}: decoded before its last byte",
                    stringify!($type)
                );
            }
        }
    }};
}

fuzz_target!(|data: &[u8]| {
    settles!(Negotiation, data);
    settles!(TunnelRequest, data);
    settles!(MethodReply, data);
    settles!(AuthReply, data);
    settles!(VipHead, data);
    settles!(VipAddresses, data);
    settles!(ReplyHead, data);
    settles!(ConnectReply, data);
    settles!(FrameHeader, data);
});
