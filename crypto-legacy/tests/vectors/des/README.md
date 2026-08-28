# DES and Triple-DES vector provenance

- **DES mechanics:** withdrawn FIPS 46-3, *Data Encryption Standard*:
  <https://csrc.nist.gov/pubs/fips/46-3/final>.
- **Triple-DES mechanics:** withdrawn NIST SP 800-67 Rev. 2:
  <https://csrc.nist.gov/pubs/sp/800/67/r2/final>.
- **Official intermediate values:** NIST `TDES_Core.pdf`:
  <https://csrc.nist.gov/CSRC/media/Projects/Cryptographic-Standards-and-Guidelines/documents/examples/TDES_Core.pdf>.
- **Withdrawal announcement:**
  <https://csrc.nist.gov/News/2023/nist-to-withdraw-sp-800-67-rev-2>.
- **Accessed:** 2026-08-27.

The public tests copy all four blocks from both two-key and three-key EDE examples in NIST's
`TDES_Core.pdf`. The first block additionally checks the published output after the `K1` DES
encryption and `K2` DES decryption, so the three-stage composition is visible rather than tested
only end to end. RustCrypto `des` 0.9.0 is a development-only differential oracle for DES, EDE2,
and EDE3 over deterministic key/block variation; it is never used by library code.

The classic first-round values checked inside `permutation.rs`, `schedule.rs`, and `round.rs` are
standard-derived regression evidence, not mislabeled NIST vectors. They follow FIPS 46-3's tables
and are separately confirmed by the full published and differential tests.
