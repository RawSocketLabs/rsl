//! NIST `AES_GCM.pdf` GCM-AES256 Examples 1–5, copied mechanically.
//!
//! Example 6 publishes a 96-bit tag and is outside the full-tag profile. Provenance:
//! `tests/vectors/gcm/README.md`.

/// One published example.
pub(crate) struct NistCase {
    pub(crate) example: u8,
    pub(crate) key: &'static str,
    pub(crate) iv: &'static str,
    pub(crate) aad: &'static str,
    pub(crate) plaintext: &'static str,
    pub(crate) ciphertext: &'static str,
    pub(crate) tag: &'static str,
}

pub(crate) const CASES: [NistCase; 5] = [
    NistCase {
        example: 1,
        key: "feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308",
        iv: "cafebabefacedbaddecaf888",
        aad: "",
        plaintext: "",
        ciphertext: "",
        tag: "fd2caa16a5832e76aa132c1453eeda7e",
    },
    NistCase {
        example: 2,
        key: "feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308",
        iv: "cafebabefacedbaddecaf888",
        aad: "",
        plaintext: "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b391aafd255",
        ciphertext: "522dc1f099567d07f47f37a32a84427d643a8cdcbfe5c0c97598a2bd2555d1aa8cb08e48590dbb3da7b08b1056828838c5f61e6393ba7a0abcc9f662898015ad",
        tag: "b094dac5d93471bdec1a502270e3cc6c",
    },
    NistCase {
        example: 3,
        key: "feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308",
        iv: "cafebabefacedbaddecaf888",
        aad: "3ad77bb40d7a3660a89ecaf32466ef97f5d3d58503b9699de785895a96fdbaaf43b1cd7f598ece23881b00e3ed0306887b0c785e27e8ad3f8223207104725dd4",
        plaintext: "",
        ciphertext: "",
        tag: "de34b6dcd4cee2fdbec3cea01af1ee44",
    },
    NistCase {
        example: 4,
        key: "feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308",
        iv: "cafebabefacedbaddecaf888",
        aad: "3ad77bb40d7a3660a89ecaf32466ef97f5d3d58503b9699de785895a96fdbaaf43b1cd7f598ece23881b00e3ed0306887b0c785e27e8ad3f8223207104725dd4",
        plaintext: "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b391aafd255",
        ciphertext: "522dc1f099567d07f47f37a32a84427d643a8cdcbfe5c0c97598a2bd2555d1aa8cb08e48590dbb3da7b08b1056828838c5f61e6393ba7a0abcc9f662898015ad",
        tag: "c06d76f31930fef37acae23ed465ae62",
    },
    NistCase {
        example: 5,
        key: "feffe9928665731c6d6a8f9467308308feffe9928665731c6d6a8f9467308308",
        iv: "cafebabefacedbaddecaf888",
        aad: "3ad77bb40d7a3660a89ecaf32466ef97f5d3d585",
        plaintext: "d9313225f88406e5a55909c5aff5269a86a7a9531534f7da2e4c303d8a318a721c3c0c95956809532fcf0e2449a6b525b16aedf5aa0de657ba637b39",
        ciphertext: "522dc1f099567d07f47f37a32a84427d643a8cdcbfe5c0c97598a2bd2555d1aa8cb08e48590dbb3da7b08b1056828838c5f61e6393ba7a0abcc9f662",
        tag: "e097195f4532da895fb917a5a55c6aa0",
    },
];
