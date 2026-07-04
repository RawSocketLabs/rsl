//! The 1.0 **scope line** for serde, pinned (see `ROADMAP.md`): bnb is *not* a serde data
//! format (serde's data model has no bit widths, byte order, magic, or count — `binrw`
//! reached the same conclusion), but user-side serde derives **coexist** with `#[bin]` on
//! plain messages — one type can carry both codecs, JSON for config/logs and bnb for the wire.
//!
//! The documented boundaries (each verified, not snapshot-pinned — the error text belongs to
//! serde/rustc and drifts across versions):
//! - a `reserved`/`calc` message rejects serde derives (the injected `encode_mode` field's
//!   `EncodeMode` implements no `Serialize` — same root cause as "no struct literals");
//! - bnb's own field types (`uN`, `#[bitfield]` structs) ship no serde impls (a post-1.0
//!   additive `serde` feature, if ever demanded).

mod macro_ {
    use bnb::{BitEnum, bin};
    use serde::{Deserialize, Serialize};

    /// One type, two codecs: `#[bin]` owns the wire bytes, serde owns the data model.
    #[bin(big)]
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Heartbeat {
        device_id: u16,
        uptime_s: u32,
        errors: u8,
    }

    #[test]
    fn serde_derives_coexist_with_bin_on_a_plain_message() {
        let h = Heartbeat {
            device_id: 0x0A01,
            uptime_s: 86_400,
            errors: 2,
        };

        // serde path: JSON round-trip (config files, logs, HTTP APIs).
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(json, r#"{"device_id":2561,"uptime_s":86400,"errors":2}"#);
        assert_eq!(serde_json::from_str::<Heartbeat>(&json).unwrap(), h);

        // bnb path: the wire bytes, unaffected by the serde derives.
        let wire = h.to_bytes().unwrap();
        assert_eq!(wire, [0x0A, 0x01, 0x00, 0x01, 0x51, 0x80, 0x02]);
        assert_eq!(Heartbeat::decode_exact(&wire).unwrap(), h);
    }

    /// A user `BitEnum` is a normal enum — serde derives apply to it like any other,
    /// independent of the bnb impls the derive adds.
    #[derive(BitEnum, Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
    #[bit_enum(u8)]
    #[repr(u8)]
    enum Kind {
        Ping = 1,
        Pong = 2,
        #[catch_all]
        Other(u8),
    }

    #[bin(big)]
    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct Tagged {
        kind: Kind,
        seq: u16,
    }

    #[test]
    fn a_bit_enum_field_carries_serde_derives_too() {
        let t = Tagged {
            kind: Kind::Pong,
            seq: 7,
        };
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(serde_json::from_str::<Tagged>(&json).unwrap(), t);
        assert_eq!(Tagged::decode_exact(&t.to_bytes().unwrap()).unwrap(), t);
        // The catch-all (tuple) variant serializes as serde's usual externally-tagged form.
        let o = Tagged {
            kind: Kind::Other(9),
            seq: 1,
        };
        assert_eq!(
            serde_json::from_str::<Tagged>(&serde_json::to_string(&o).unwrap()).unwrap(),
            o
        );
    }
}
