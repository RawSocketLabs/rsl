# HKDF-SHA-256 vector provenance

## RFC 5869

- **Evidence class:** published vector.
- **Publication:** RFC 5869, *HMAC-based Extract-and-Expand Key Derivation Function (HKDF)*.
- **Revision:** May 2010.
- **Status:** Informational IETF consensus document.
- **Persistent identifier:** <https://doi.org/10.17487/RFC5869>.
- **Authoritative document:** <https://www.rfc-editor.org/rfc/rfc5869.html>.
- **Cases used:** Appendix A.1, A.2, and A.3, the three SHA-256 cases. Each publishes `IKM`,
  `salt`, `info`, `L`, `PRK`, and `OKM`.
- **Checked:** 2026-08-27.
- **Conversion policy:** remove the printed `0x` prefix, concatenate wrapped hexadecimal lines,
  and decode complete octets. Appendix A.3's zero-length salt and info remain semantically absent
  or empty as specified rather than being replaced with display placeholders.

The RFC Editor listed one reported editorial erratum, Errata ID 5161, for §2.3 when checked. It
clarifies the prose describing the single-octet block counter. The equations already show
`0x01`, `0x02`, `0x03`, and so on, and the normative `N <= 255` bound means this implementation
emits counter octets with values 1 through `N` without wrapping.

## Differential implementation

- **Evidence class:** differential result.
- **Implementation:** RustCrypto `hkdf` 0.13.0 instantiated with `sha2` 0.11.0, both with default
  features disabled.
- **Upstream:** <https://github.com/RustCrypto/KDFs/tree/master/hkdf>.
- **Role:** development-only oracle over optional salts, varied input/context sizes, output block
  boundaries, and the exact RFC maximum.
- **Checked:** 2026-08-27.

Production `rsl-crypto` code depends on neither differential implementation.
