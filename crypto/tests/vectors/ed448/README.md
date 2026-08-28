# Ed448 vector provenance

- **Controlling publication:** RFC 8032, *Edwards-Curve Digital Signature Algorithm (EdDSA)*,
  January 2017, IRTF Informational, §5.2 and §7.4–§7.5. <https://www.rfc-editor.org/info/rfc8032/>;
  text from <https://www.rfc-editor.org/rfc/rfc8032.txt>, accessed 2026-08-28. Curve constants
  (`p`, `d`, base point, order) come from RFC 7748 §4.2 and are converted from the printed
  decimal integers by a mechanical script; the base point is checked against the curve equation
  during conversion and again by the decoding round-trip test.
- **Errata:** the RFC 8032 errata record (see `tests/vectors/ed25519/README.md`) was re-checked
  2026-08-28; none change the Ed448 vectors or steps used here.

## Material used

- §7.4: all nine Ed448 vectors (`Blank`, `1 octet`, `1 octet (with context)`, `11`, `12`, `13`,
  `64`, `256`, and `1023 octets`), each with secret key, public key, message, optional context,
  and signature.
- §7.5: both Ed448ph vectors (`TEST abc`, with and without context).

## Project Wycheproof

- **File:** `testvectors_v1/ed448_test.json` from <https://github.com/C2SP/wycheproof>.
- **SHA-256 of file as downloaded 2026-08-28 from the `master` branch:**
  `3b3c7995853deb2fbbb49fba0fd292f314dc081f9154bd33252f294ca211289a`.
- **Cases:** all 87 across every group (17 valid, 70 invalid), each stored with its group's
  public key.
- **License:** Apache License 2.0.

## Conversion policy

Printed hexadecimal is joined across line breaks in printed byte order. No differential crate is
used: the RustCrypto Ed448 implementation is pre-release only, so Wycheproof's independently
generated suite is the scheme-level oracle.
