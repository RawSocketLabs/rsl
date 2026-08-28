# Ed25519 vector provenance

- **Controlling publication:** RFC 8032, *Edwards-Curve Digital Signature Algorithm (EdDSA)*.
- **Publication record:** <https://www.rfc-editor.org/info/rfc8032/>.
- **Normative text:** <https://www.rfc-editor.org/rfc/rfc8032.html>.
- **Errata record:** <https://errata.rfc-editor.org/search/?rfc_number=8032>.
- **Publication date and stream:** January 2017, IRTF Informational.
- **Accessed:** 2026-08-27.

The known-answer tests copy §7.1 tests 1–3 exactly. Printed line breaks and spaces are removed;
hexadecimal pairs remain in their published byte order. Verified Errata 5968 clarifies that an
encoded scalar may be any integer `0 <= S <= L-1`; the implementation accepts exactly that range.
Verified Errata 5930 fixes a missing `raise` in the illustrative verifier and reinforces exact
signature-length rejection. Other verified and held errata are tracked in `STANDARDS.md`.

`ed25519-dalek` 3.0.0 is a development-only differential oracle, not an implementation dependency
or standards authority. Differential verification uses its explicit strict path.
