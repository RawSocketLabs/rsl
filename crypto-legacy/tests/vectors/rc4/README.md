# RC4 vector provenance

- **Compatibility baseline:** RFC 6229, *Test Vectors for the Stream Cipher RC4*, §2:
  <https://www.rfc-editor.org/rfc/rfc6229.html>.
- **Historical SSH profile:** RFC 4345, §4:
  <https://www.rfc-editor.org/rfc/rfc4345.html#section-4>.
- **TLS prohibition:** RFC 7465: <https://www.rfc-editor.org/rfc/rfc7465.html>.
- **SSH deprecation:** RFC 8758: <https://www.rfc-editor.org/rfc/rfc8758.html>.
- **Accessed:** 2026-08-27.

The public tests copy RFC 6229's 40-bit Key 1 (`01 02 03 04 05`) output at offsets 0–31,
240–271, 1520–1551, and 4080–4111. The explicit discard test uses the 1536-byte offset selected
by RFC 4345's historical improved Arcfour profiles. RustCrypto `rc4` 0.2.0 is a development-only
differential oracle across all accepted key-length boundaries and fragmented stream updates; it
is never used by library code.
