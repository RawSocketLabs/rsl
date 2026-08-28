//! Structured certificate mutations, decoy anchors, policy choices, and issuer-search budgets
//! must produce a validation result without panicking or escaping the configured work bound.
//!
//! Run: `cargo +nightly fuzz run path_validation --fuzz-dir pki/fuzz`.
#![no_main]

use std::sync::LazyLock;

use libfuzzer_sys::fuzz_target;
use rsl_asn1::{Encoder, ObjectIdentifier, Tag};
use rsl_crypto::signature::ed25519::Ed25519SigningKey;
use rsl_pki::{
    ErrorKind, PathValidator, Purpose, RequiredKeyUsage, RevocationChecker, RevocationMode,
    RevocationStatus,
};
use rsl_x509::{Certificate, Time, oid};

struct ChainDer {
    root: Vec<u8>,
    decoy: Vec<u8>,
    leaf: Vec<u8>,
}

static CHAIN: LazyLock<ChainDer> = LazyLock::new(|| {
    let root_key = Ed25519SigningKey::from_seed([1; 32]);
    let leaf_key = Ed25519SigningKey::from_seed([2; 32]);
    let decoy_key = Ed25519SigningKey::from_seed([3; 32]);
    ChainDer {
        root: certificate(
            "Root",
            "Root",
            root_key.verifying_key().as_bytes(),
            &root_key,
            true,
            None,
        ),
        decoy: certificate(
            "Root",
            "Root",
            decoy_key.verifying_key().as_bytes(),
            &decoy_key,
            true,
            None,
        ),
        leaf: certificate(
            "leaf.test",
            "Root",
            leaf_key.verifying_key().as_bytes(),
            &root_key,
            false,
            Some("leaf.test"),
        ),
    }
});

struct FixedRevocation(RevocationStatus);

impl RevocationChecker for FixedRevocation {
    fn status(
        &self,
        _certificate: &Certificate<'_>,
        _issuer: &Certificate<'_>,
    ) -> RevocationStatus {
        self.0
    }
}

fn built(build: impl FnOnce(&mut Encoder) -> rsl_asn1::Result<()>) -> Vec<u8> {
    let mut encoder = Encoder::new();
    build(&mut encoder).unwrap();
    encoder.finish()
}

fn object_identifier(arcs: &[u64]) -> ObjectIdentifier {
    ObjectIdentifier::from_arcs(arcs).unwrap()
}

fn algorithm() -> Vec<u8> {
    built(|encoder| {
        encoder.sequence(|sequence| sequence.object_identifier(&object_identifier(oid::ED25519)))
    })
}

fn name(common_name: &str) -> Vec<u8> {
    let attribute = built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.object_identifier(&object_identifier(oid::COMMON_NAME))?;
            sequence.element(Tag::UTF8_STRING, common_name.as_bytes())
        })
    });
    let set = built(|encoder| encoder.element(Tag::SET, &attribute));
    built(|encoder| encoder.element(Tag::SEQUENCE, &set))
}

fn extension(arcs: &[u64], critical: bool, value: &[u8]) -> Vec<u8> {
    built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.object_identifier(&object_identifier(arcs))?;
            if critical {
                sequence.boolean(true)?;
            }
            sequence.octet_string(value)
        })
    })
}

fn certificate(
    subject_name: &str,
    issuer_name: &str,
    public_key: &[u8; 32],
    issuer_key: &Ed25519SigningKey,
    ca: bool,
    dns_name: Option<&str>,
) -> Vec<u8> {
    let signature_algorithm = algorithm();
    let version = built(|encoder| encoder.unsigned_integer(&[2]));
    let validity = built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.element(Tag::UTC_TIME, b"260101000000Z")?;
            sequence.element(Tag::UTC_TIME, b"270101000000Z")
        })
    });
    let spki = built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.encoded(&algorithm())?;
            sequence.bit_string(0, public_key)
        })
    });
    let mut extensions = Vec::new();
    if ca {
        let basic = built(|encoder| encoder.sequence(|sequence| sequence.boolean(true)));
        extensions.push(extension(oid::BASIC_CONSTRAINTS, true, &basic));
        let usage = built(|encoder| encoder.bit_string(2, &[0x04]));
        extensions.push(extension(oid::KEY_USAGE, true, &usage));
    } else {
        let usage = built(|encoder| encoder.bit_string(7, &[0x80]));
        extensions.push(extension(oid::KEY_USAGE, true, &usage));
        let eku = built(|encoder| {
            encoder.sequence(|sequence| {
                sequence.object_identifier(&object_identifier(oid::SERVER_AUTH))
            })
        });
        extensions.push(extension(oid::EXTENDED_KEY_USAGE, false, &eku));
        if let Some(dns_name) = dns_name {
            let san = built(|encoder| {
                encoder.sequence(|sequence| {
                    sequence.element(Tag::context(2, false), dns_name.as_bytes())
                })
            });
            extensions.push(extension(oid::SUBJECT_ALT_NAME, false, &san));
        }
    }
    let extension_sequence = built(|encoder| {
        encoder.sequence(|sequence| {
            for extension in &extensions {
                sequence.encoded(extension)?;
            }
            Ok(())
        })
    });
    let tbs = built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.element(Tag::context(0, true), &version)?;
            sequence.unsigned_integer(&[1])?;
            sequence.encoded(&signature_algorithm)?;
            sequence.encoded(&name(issuer_name))?;
            sequence.encoded(&validity)?;
            sequence.encoded(&name(subject_name))?;
            sequence.encoded(&spki)?;
            sequence.element(Tag::context(3, true), &extension_sequence)
        })
    });
    let signature = issuer_key.sign(&tbs).unwrap();
    built(|encoder| {
        encoder.sequence(|sequence| {
            sequence.encoded(&tbs)?;
            sequence.encoded(&signature_algorithm)?;
            sequence.bit_string(0, signature.as_bytes())
        })
    })
}

fn mutate(target: &mut [u8], mutation: &[u8]) {
    let index = usize::from(u16::from_le_bytes([mutation[0], mutation[1]])) % target.len();
    target[index] ^= mutation[2];
}

fuzz_target!(|data: &[u8]| {
    let controls = [
        data.first().copied().unwrap_or(0),
        data.get(1).copied().unwrap_or(0),
        data.get(2).copied().unwrap_or(8),
        data.get(3).copied().unwrap_or(16),
    ];
    let mut leaf_der = CHAIN.leaf.clone();
    let mut root_der = CHAIN.root.clone();
    let mut decoy_der = CHAIN.decoy.clone();
    let (mutations, _) = data.get(4..).unwrap_or_default().as_chunks::<4>();
    for mutation in mutations.iter().take(64) {
        match mutation[0] % 3 {
            0 => mutate(&mut leaf_der, &mutation[1..]),
            1 => mutate(&mut root_der, &mutation[1..]),
            _ => mutate(&mut decoy_der, &mutation[1..]),
        }
    }

    let (Ok(leaf), Ok(root), Ok(decoy)) = (
        Certificate::from_der(&leaf_der),
        Certificate::from_der(&root_der),
        Certificate::from_der(&decoy_der),
    ) else {
        return;
    };
    let decoy_count = usize::from(controls[0] & 0x0f);
    let mut anchors = Vec::with_capacity(decoy_count + 1);
    anchors.resize(decoy_count, decoy);
    anchors.push(root);

    let year = match controls[1] % 3 {
        0 => 2026,
        1 => 2025,
        _ => 2027,
    };
    let time = Time::new(year, 6, 1, 0, 0, 0).unwrap();
    let max_depth = usize::from(controls[2] & 0x0f);
    let max_candidate_checks = usize::from(controls[3] & 0x1f);
    let mut builder = PathValidator::builder()
        .leaf(&leaf)
        .trust_anchors(&anchors)
        .at_time(time)
        .max_depth(max_depth)
        .max_candidate_checks(max_candidate_checks);
    if controls[0] & 0x10 != 0 {
        builder = builder.purpose(Purpose::ServerAuth);
    }
    if controls[0] & 0x20 != 0 {
        builder = builder.required_key_usage(RequiredKeyUsage::DigitalSignature);
    }
    if controls[0] & 0x40 != 0 {
        builder = builder.dns_name(if controls[1] & 0x80 == 0 {
            "leaf.test"
        } else {
            "other.test"
        });
    }
    let revocation = FixedRevocation(match controls[2] >> 6 {
        0 => RevocationStatus::Good,
        1 => RevocationStatus::Revoked,
        _ => RevocationStatus::Unknown,
    });
    if controls[0] & 0x80 != 0 {
        builder = builder.revocation(
            &revocation,
            if controls[1] & 0x40 == 0 {
                RevocationMode::SoftFail
            } else {
                RevocationMode::RequireKnown
            },
        );
    }
    let result = builder.validate();

    if data.is_empty() {
        assert!(result.is_ok());
    }
    let policy_reaches_path_search = mutations.is_empty()
        && year == 2026
        && max_depth >= 2
        && (controls[0] & 0x40 == 0 || controls[1] & 0x80 == 0);
    if policy_reaches_path_search && max_candidate_checks < decoy_count + 1 {
        assert!(matches!(
            result.map_err(|error| error.kind),
            Err(ErrorKind::PathSearchLimitExceeded)
        ));
    }
});
