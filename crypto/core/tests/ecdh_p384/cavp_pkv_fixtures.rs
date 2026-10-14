//! NIST CAVP ECDSA public-key validation P-384 cases, copied from `PKV.rsp`.
//!
//! Provenance: `tests/vectors/ecdh-p384/README.md`. Out-of-range coordinates are printed with
//! 97 hexadecimal digits; the harness treats any non-96-digit coordinate as out of range.

/// One published case and NIST's printed verdict.
pub(crate) struct PkvCase {
    pub(crate) x: &'static str,
    pub(crate) y: &'static str,
    pub(crate) verdict: &'static str,
}

pub(crate) const CASES: [PkvCase; 12] = [
    PkvCase {
        x: "e87cc868cdf196471d3fc78c324be2c4a0de8dbde182afea88baa51666f3cc9993eae5f1d60d4aec58894f0357273c48",
        y: "187219b0adc398c835791798053cc6a0bcc6e43228ac23101ee93dfce0e508be988a55fa495eb93b832064dc035e7720",
        verdict: "F (2 - Point not on curve)",
    },
    PkvCase {
        x: "6e9c7e92ee23713fabb05d0b50e088eb534fd1e2b257c03304cfa33598f88a07c7e31a13e24707a7057ca2919323058e",
        y: "a218a485e22eae08c3618cfd73befcfcd13c3f196c08df99d7f79ebffe9f127b896aa0cb36cfdf2fc4818b8cd766f185",
        verdict: "P (0 )",
    },
    PkvCase {
        x: "452eb75736ac00974f953a0ce6060c19911a3463b045cb15ad6c0fa5045d66b04252a9001e8c4a9a6a0293f127bd20d9",
        y: "a1da4fbf8f0726fb9e04cf3ed0404af6cafb028b924c1951165f0ffe7caf04c05444cc7defb8cb62381727b6c1589f13",
        verdict: "P (0 )",
    },
    PkvCase {
        x: "25e5509a54f5fa62f94551dff3dfe210db1bb2bbc8fd4e672fbd5a211f9fd2f7eadc2b83fcd4198b7f857d9a2dc39c11",
        y: "98a4a13bc2f2d04bebd6d4e04412a9d306e57b90364583a6ec25bf6f0175bb5b397b8cfea83fd5d1e0ad052852b4aba7",
        verdict: "F (2 - Point not on curve)",
    },
    PkvCase {
        x: "11a14be72dd023667047c260dd1960dd16555289d9570001d53ea3e494c1c107800dc5b24dd4de8490a071658702a0962",
        y: "78d65f6975d10df838b96a16cba873b59c28f2c7d05654b8c8b78bd193694ae45d6c6e046a20b984c3467c72d49395fe",
        verdict: "F (1 - Q_x or Q_y out of range)",
    },
    PkvCase {
        x: "a953eafd9dae3862d1049dd99cf628745bfb8f1024aaa567c51e9da01eb9bda996a7b1c906b3bb44a94649df2bcef304",
        y: "2f66dda137d3a408e2498d532f652e668f09b86bc056ff699efcc71ed1f22967ca7a99c8bf64f246b93c1982f856ed27",
        verdict: "P (0 )",
    },
    PkvCase {
        x: "1bf2238026a2489fb6ac1a8d6b82fdb33b05e8d01f1e2671eb22e61734031cc63efbf7e14d23e81fd432fc9935c627cdd",
        y: "6b377c8b187d568b782a28b38a7861b69e3d016f9f9ebb7eff2e7732a5132785b5a32e069dcef12875a995908a8b72f1",
        verdict: "F (1 - Q_x or Q_y out of range)",
    },
    PkvCase {
        x: "a999b80932ea62b4689769225b3ff34b0709c4e32342a824799ca63dcce1f3ed8819e080fc7fa130c1881c8131f4bcb5",
        y: "b8c77d0868c2c159e1be6bcd60ec488ab31531c21e1cb8fe2493ed26ac848fde7d27823a9a4912650511a3d460e25ef2",
        verdict: "F (2 - Point not on curve)",
    },
    PkvCase {
        x: "5cbaa8088b0804fe14c2a6fa54b1adee1690fd17a682ea9ec5202ba7575e217c37e98565b7e95e7c882bb6eef76a3df1",
        y: "79d8c7e96ae7a7668496317c596b24ebe56e6ea5bc64b74c38867eb2c419d8277d20b9c27a2d5c75d1c7a47885d38d0e",
        verdict: "F (2 - Point not on curve)",
    },
    PkvCase {
        x: "cfb4dbdcb1a8c6e8c6b4a9dd091eed015476ebd20837de1f6261a27999a08cff345f0d4627eb7778fc3495916a6d017b",
        y: "1c08f7a421bc0731321374f9b31ecf5ca820c006180da4c496f29f0d0e4947f368808fd3052ee4f1afb8c2005fd0c0ee8",
        verdict: "F (1 - Q_x or Q_y out of range)",
    },
    PkvCase {
        x: "1adaff25f37c8dfd33ecf216691a2107e522c21c99e29a76d8c1757ef84cc37c73ec5c2aa3be2fb0d5f1d372e08fbf9e",
        y: "1f39c8f86a20c130c34f767e085217232599541516e2d79d8e526fa03082bed2a5dc5fde6fd410c30245212e7816dd014",
        verdict: "F (1 - Q_x or Q_y out of range)",
    },
    PkvCase {
        x: "31951643c18400593f2d7cb32a3acf6071b4d95b8ab80a0535aa5edc9e01145f6dcc91a9977eb450eb077112edf887b2",
        y: "098a9e569684ca517bfdd5bc4b57876b210c3d7598e4f989e8f88f9f103b5d90d6baaa1a6617d524001c44a677bd13d0",
        verdict: "P (0 )",
    },
];
