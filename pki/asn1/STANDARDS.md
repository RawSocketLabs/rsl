# Standards traceability

Accessed 2026-08-28.

| Publication | Revision and status | Authoritative source | Owned rules |
|---|---|---|---|
| ITU-T X.690 | 02/2021, in force; Erratum 1 (09/2021) noted | <https://www.itu.int/rec/T-REC-X.690-202102-I/en> | §8.1 identifier and definite length; §§8.2, 8.3, 8.6–8.9, 8.12, 8.19, 8.23 primitive and constructed contents; §§10–11 DER restrictions and SET OF ordering |

`asn1` defines the `UTCTime` and `GeneralizedTime` tags but does not decode time values; that is
`x509`'s `Validity` parsing, traced in `../x509/STANDARDS.md`.

Clause numbers above are not yet checked against the pinned 02/2021 text.

Tests label hand-written encodings as standard-derived or negative evidence. They are not
presented as published vectors.
