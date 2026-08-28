//! NIST CAVP ECDSA public-key validation P-256 cases, copied from `PKV.rsp`.
//!
//! Provenance: `tests/vectors/ecdh-p256/README.md`. Out-of-range coordinates are printed with
//! 65 hexadecimal digits; the harness treats any non-64-digit coordinate as out of range.

/// One published case and NIST's printed verdict.
pub(crate) struct PkvCase {
    pub(crate) x: &'static str,
    pub(crate) y: &'static str,
    pub(crate) verdict: &'static str,
}

pub(crate) const CASES: [PkvCase; 12] = [
    PkvCase {
        x: "e0f7449c5588f24492c338f2bc8f7865f755b958d48edb0f2d0056e50c3fd5b7",
        y: "86d7e9255d0f4b6f44fa2cd6f8ba3c0aa828321d6d8cc430ca6284ce1d5b43a0",
        verdict: "P (0 )",
    },
    PkvCase {
        x: "d17c446237d9df87266ba3a91ff27f45abfdcb77bfd83536e92903efb861a9a9",
        y: "1eabb6a349ce2cd447d777b6739c5fc066add2002d2029052c408d0701066231c",
        verdict: "F (1 - Q_x or Q_y out of range)",
    },
    PkvCase {
        x: "17875397ae87369365656d490e8ce956911bd97607f2aff41b56f6f3a61989826",
        y: "980a3c4f61b9692633fbba5ef04c9cb546dd05cdec9fa8428b8849670e2fba92",
        verdict: "F (1 - Q_x or Q_y out of range)",
    },
    PkvCase {
        x: "f2d1c0dc0852c3d8a2a2500a23a44813ccce1ac4e58444175b440469ffc12273",
        y: "32bfe992831b305d8c37b9672df5d29fcb5c29b4a40534683e3ace23d24647dd",
        verdict: "F (2 - Point not on curve)",
    },
    PkvCase {
        x: "10b0ca230fff7c04768f4b3d5c75fa9f6c539bea644dffbec5dc796a213061b58",
        y: "f5edf37c11052b75f771b7f9fa050e353e464221fec916684ed45b6fead38205",
        verdict: "F (1 - Q_x or Q_y out of range)",
    },
    PkvCase {
        x: "2c1052f25360a15062d204a056274e93cbe8fc4c4e9b9561134ad5c15ce525da",
        y: "ced9783713a8a2a09eff366987639c625753295d9a85d0f5325e32dedbcada0b",
        verdict: "P (0 )",
    },
    PkvCase {
        x: "a40d077a87dae157d93dcccf3fe3aca9c6479a75aa2669509d2ef05c7de6782f",
        y: "503d86b87d743ba20804fd7e7884aa017414a7b5b5963e0d46e3a9611419ddf3",
        verdict: "F (2 - Point not on curve)",
    },
    PkvCase {
        x: "2633d398a3807b1895548adbb0ea2495ef4b930f91054891030817df87d4ac0a",
        y: "d6b2f738e3873cc8364a2d364038ce7d0798bb092e3dd77cbdae7c263ba618d2",
        verdict: "P (0 )",
    },
    PkvCase {
        x: "14bf57f76c260b51ec6bbc72dbd49f02a56eaed070b774dc4bad75a54653c3d56",
        y: "7a231a23bf8b3aa31d9600d888a0678677a30e573decd3dc56b33f365cc11236",
        verdict: "F (1 - Q_x or Q_y out of range)",
    },
    PkvCase {
        x: "2fa74931ae816b426f484180e517f5050c92decfc8daf756cd91f54d51b302f1",
        y: "5b994346137988c58c14ae2152ac2f6ad96d97decb33099bd8a0210114cd1141",
        verdict: "P (0 )",
    },
    PkvCase {
        x: "f8c6dd3181a76aa0e36c2790bba47041acbe7b1e473ff71eee39a824dc595ff0",
        y: "9c965f227f281b3072b95b8daf29e88b35284f3574462e268e529bbdc50e9e52",
        verdict: "F (2 - Point not on curve)",
    },
    PkvCase {
        x: "7a81a7e0b015252928d8b36e4ca37e92fdc328eb25c774b4f872693028c4be38",
        y: "08862f7335147261e7b1c3d055f9a316e4cab7daf99cc09d1c647f5dd6e7d5bb",
        verdict: "F (2 - Point not on curve)",
    },
];
