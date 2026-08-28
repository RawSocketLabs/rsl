# GCM evidence provenance

## Controlling definition

GCM uses the same NIST SP 800-38D November 2007 source baseline and pending-revision status
recorded in [`../../../STANDARDS.md`](../../../STANDARDS.md). The first non-GHASH operation uses
§6.2's `inc_s` definition with `s = 32`, as required by §6.5 Algorithm 3 and Algorithms 4–5.

## Supplementary NIST example

- **Evidence class:** published input and output counter blocks.
- **Publisher:** National Institute of Standards and Technology.
- **Index:**
  <https://csrc.nist.gov/projects/cryptographic-standards-and-guidelines/example-values>.
- **Document:**
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/AES_GCM.pdf>.
- **Case used:** GCM-AES128 Example 2 publishes
  `J0 = cafebabefacedbaddecaf88800000001`, then GCTR counter blocks ending in `00000002`,
  `00000003`, `00000004`, and `00000005`. It publishes the four complete plaintext and
  ciphertext blocks. The common setup also publishes key-derived hash subkey
  `H = b83b533708bf535d0aa6e52980d53b78`. Example 5 publishes the same first three blocks and a final
  12-byte partial plaintext/ciphertext block. Examples 2–5 publish final GHASH intermediate `S`
  for empty AAD, empty ciphertext, both complete, and both partial cases. Examples 1–5 publish
  full 128-bit tags.
- **Checked:** 2026-08-27.
- **Conversion policy:** spaces and line wrapping are removed; hexadecimal pairs remain in the
  displayed byte order. No byte reversal is applied to the rightmost integer field.

Algorithm 4 step 3 specifies the first published counter block as `inc32(J0)`. Algorithm 3 step 5
specifies every later block by another `inc32`. The focused tests therefore compare directly with
published values; separate carry and modulo-wrap cases are standard-derived boundary evidence.

The GCTR tests remove only the document's whitespace from Example 2's 64-byte plaintext and
ciphertext and Example 5's 60-byte values. They exercise Algorithm 3 directly with
`ICB = inc32(J0)`. The partial case confirms that only the leftmost 96 cipher-output bits affect
Example 5's last 96 input bits. Empty-input and double-application tests are standard-derived
rather than additional NIST vectors.

The setup tests compare `AES_K(0^128)` directly with the published `H` and the 96-bit direct
construction `IV || 0^31 || 1` directly with published `J0`. A distinct-byte IV test is
standard-derived position evidence. No fixture or API implies support for SP 800-38D's separate
variable-length-IV GHASH branch.

The authentication-input tests compare directly with published `S` values:

- Example 2: empty AAD and 64-byte ciphertext;
- Example 3: 64-byte AAD and empty ciphertext;
- Example 4: 64-byte AAD and 64-byte ciphertext; and
- Example 5: 20-byte AAD and 60-byte ciphertext, requiring two independent zero-padded blocks.

The explicit `[160]_64 || [480]_64` length-block test is standard-derived encoding evidence. NIST
publishes those input lengths and final `S`, but does not separately print the encoded length block.

The tag-layer test takes each published `S` from Examples 1–5, applies Algorithm 4 step 6 with the
published `J0`, and compares directly with the corresponding published full tag. Example 1's
all-zero `S` also shows that its tag equals the document's published `CIPH_K(J0)` value. No test
truncates a tag or implies support for a shorter tag profile.

The private Algorithm 4 composition tests reproduce Example 1's empty ciphertext and tag,
Example 2's four complete ciphertext blocks and tag, and Example 5's partial ciphertext and tag.
The private Algorithm 5 composition tests recover Example 1's empty plaintext and Example 5's
60-byte plaintext, then independently alter IV, AAD, ciphertext, and tag. The implementation uses
the verify-before-plaintext order explicitly permitted by the paragraph following Algorithm 5
step 8.

Public integration tests retain Examples 1 and 5 as end-to-end known answers through only
`Aes128Gcm`'s exported types. Separate negative tests alter every byte position in a full tag and
every byte position in a multi-block ciphertext. A cleartext-AAD test records the intended wire
boundary: the caller's AAD bytes remain unchanged and sendable, but changing them prevents the
ciphertext from opening.

## Differential evidence

- **Evidence class:** development-only differential comparison, not standards authority.
- **Implementation:** `RustCrypto` [`aes-gcm` 0.11.1](https://crates.io/crates/aes-gcm/0.11.1).
- **Upstream repository:** <https://github.com/RustCrypto/AEADs/tree/master/aes-gcm>.
- **Source checked:** 2026-08-27.
- **Cases:** 32 deterministic AES-128 keys and 96-bit nonces with independently varied AAD and
  payload lengths covering empty, one-byte, both sides of 16/32/64-byte block boundaries, and a
  129-byte multi-block value.

The oracle is a development dependency only. Production `rsl-crypto` code does not call it.
