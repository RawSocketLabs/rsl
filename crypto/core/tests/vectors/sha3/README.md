# SHA3-256 and SHAKE256 vector provenance

## Controlling publication

- **Publication:** NIST FIPS 202, *SHA-3 Standard: Permutation-Based Hash and Extendable-Output
  Functions*, August 2015. <https://doi.org/10.6028/NIST.FIPS.202>; text from
  <https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.202.pdf>. Accessed 2026-08-28.
- **Sections used:** §3.1 (state and byte/lane conversion), §3.2 (step mappings, Table 2, and the
  Algorithm 5 LFSR for `RC`), §3.3–§3.4 (rounds), §4 (sponge), §5.1 (`pad10*1`), §6.1–§6.2
  (SHA3-256 and SHAKE256 suffixes). The rotation offsets and round constants are regenerated in
  unit tests from Algorithms 2 and 5 rather than trusted as transcriptions.

## NIST intermediate-value examples

- **Index:** <https://csrc.nist.gov/projects/cryptographic-standards-and-guidelines/example-values>.
- **Documents:** `SHA3-256_Msg0.pdf` (state after each of `θ`, `ρ`, `π`, `χ`, `ι` in round 0 and
  after round 23; final hash), `SHA3-256_1600.pdf` (1600-bit message and hash), and
  `SHAKE256_Msg0.pdf` (512-byte output for the empty message). Accessed 2026-08-28.
- **Conversion:** the 1600-bit message is printed as a bit string, least-significant bit
  first (`1 1 0 0 0 1 0 1` per byte), which Algorithm 10 maps to `0xa3`; the test uses 200 bytes of
  `0xa3`. The printed 200-byte state dumps are joined in order and lowercased by a
  mechanical script into `src/digest/sha3/nist_fixtures.rs`; the "Data to be absorbed" block for
  the empty message (`06 00 … 80`) is reproduced by the padding unit test.

## NIST CAVP byte-oriented vectors

- **Archives:** `sha-3bytetestvectors.zip` (SHA-256
  `cd07701af2e47f5cc889d642528b4bf11f8b6eb55797c7307a96828ed8d8fc8c`) and
  `shakebytetestvectors.zip` (SHA-256
  `debfebc3157b3ceea002b84ca38476420389a3bf7e97dc5f53ea4689a16de4c7`) from
  <https://csrc.nist.gov/projects/cryptographic-algorithm-validation-program/secure-hashing>,
  downloaded 2026-08-28.
- **Files and cases:** `SHA3_256ShortMsg.rsp` and `SHAKE256ShortMsg.rsp` at `Len = 0, 8, 1080,
  1088, 1096, 2168, 2176` bits (whichever exist in each file), bracketing the 136-byte rate;
  `SHAKE256VariableOut.rsp` cases with output lengths 16, 128, and 1088 bits (`COUNT` recorded
  in the test).

The `sha3` crate 0.12 (SHA3-256) and the `shake` crate 0.1 (SHAKE256) are development-only
differential oracles.
