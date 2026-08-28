# RSL PKI design

## Boundaries

`rsl-asn1` owns strict DER transport, `rsl-x509` owns untrusted certificate syntax, and `rsl-pki`
owns an explicitly configured trust decision. Protocol crates own certificate negotiation,
transcript binding, clocks, root stores, and revocation transport.

## Guided path and explicit escape hatches

The common path should be conspicuous: parse with `Certificate::from_der`, configure every
required trust input through the `PathValidator` typestate builder, and consume a `ValidatedPath`.
Defaults are bounded and unsupported critical semantics fail closed.

Experimentation stays possible through lower layers:

| Need | Explicit lower-level surface | What it does not claim |
|---|---|---|
| Inspect exact or unfamiliar ASN.1 | `rsl_asn1::Decoder`, `Element::contents`, and `Element::encoded` | BER acceptance, schema meaning, or trust |
| Inspect unsupported certificate fields | `Certificate`, raw `AlgorithmIdentifier` parameters, `Extension::value`, and exact `TBSCertificate` bytes | Algorithm support or certificate validity |
| Test one signature relationship | `rsl_pki::verify_certificate_signature` | A path, purpose, identity, time, or revocation decision |
| Explore unusual path graphs | `intermediates`, `max_depth`, and `max_candidate_checks` | Permission to silently exceed the caller-selected work budget |

Unlike a general protocol codec, signed-certificate parsing cannot preserve and accept arbitrary
non-canonical BER as if it were DER: canonical encoding is part of the authenticated format. A
test that needs malformed transport can operate on exact bytes or the lower-level cursor, but the
strict certificate constructor must continue to reject it.

An escape hatch may bypass a policy layer or expose a primitive result. It must be explicit in the
call site and must not manufacture `ValidatedPath` or another trusted-state type without running
the configured trust checks. Future experimental algorithm hooks follow the same rule and should
identify the caller-supplied policy in their output type.

## Assurance posture

Published fixtures, negative tests, differential parsing, and fuzz targets are engineering
evidence. They do not replace independent audit or make a production-security claim.
