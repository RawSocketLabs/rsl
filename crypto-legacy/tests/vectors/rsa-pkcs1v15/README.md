# RSA PKCS #1 v1.5 fixture provenance

These fixtures establish compatibility only. They do not make PKCS #1 v1.5 encryption,
SHA-1 signatures, or this variable-time RSA engine safe for new protection.

## NIST signature vector

- **Source:** NIST Cryptographic Algorithm Validation Program, `186-3rsatestvectors.zip`, file
  `SigGen15_186-3.txt`, CAVS 11.4, `[mod = 2048]`, first `SHAAlg = SHA256` case:
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Algorithm-Validation-Program/documents/dss/186-3rsatestvectors.zip>.
- **Fields copied:** the group `n`, `e`, and `d`, then the first SHA-256 `Msg` and `S`.
- **Coverage:** exact private signature generation, public verification, and changed-message
  rejection.

## Project Wycheproof encryption vectors

- **Source:** Project Wycheproof `rsa_pkcs1_2048_test.json`, algorithm
  `RSAES-PKCS1-v1_5`, first `RsaesPkcs1Decrypt` group:
  <https://github.com/C2SP/wycheproof/blob/main/testvectors_v1/rsa_pkcs1_2048_test.json>.
- **Key fields copied:** `privateKey.modulus` and `privateKey.privateExponent`.
- **Cases copied:** tcId 3 (`"Test"`, valid) and tcId 14 (a zero at padding-string byte seven,
  invalid).
- **Coverage:** correct decryption and the exact eight-byte minimum padding boundary. Local unit
  tests separately cover wrong block type, missing separator, too-short input, and an entropy
  source that cannot produce a nonzero padding byte.

## Conversion policy

Wrapped hexadecimal lines are concatenated without delimiters, then decoded in their printed
big-endian byte order. The leading `00` on Wycheproof's unsigned modulus is retained in the test
fixture; component import intentionally normalizes it. No fixture is generated or downloaded at
test time. Sources were accessed 2026-08-27.
