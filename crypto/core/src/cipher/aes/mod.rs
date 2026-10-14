//! Advanced Encryption Standard algorithm family.
//!
//! ## Controlling standard
//!
//! Implementations in this family target the May 2023 update of
//! [NIST FIPS 197][fips-197]. NIST identifies that publication as `NIST FIPS 197-upd1` and states
//! that the update made no technical changes to the AES algorithm. The repository-level
//! `STANDARDS.md` records the exact revision, retrieval locations, checked date, notation map,
//! and implementation coverage.
//!
//! ## Export boundary
//!
//! [`aes128`] exports a readable block-cipher type after state mapping, finite-field arithmetic,
//! forward and inverse transformations, key expansion, published vectors, boundary tests, and an
//! independent differential comparison were added. Export records that initial evidence
//! milestone; it is not a production-security or side-channel-resistance claim.
//!
//! [`aes256`] adds the fourteen-round key size over the same layers.
//!
//! Start with [`aes128`] for a guided explanation of the 4-by-4 byte state, key expansion,
//! forward and inverse rounds, and the boundary between raw AES and AES-GCM.
//!
//! [fips-197]: https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf

pub mod aes128;
pub mod aes256;
