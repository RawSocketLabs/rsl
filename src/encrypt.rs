use strum_macros::{Display, EnumString, IntoStaticStr};

#[derive(Debug, Clone, Copy, PartialEq, Display, EnumString, IntoStaticStr)]
pub enum Encryption {
    #[strum(serialize = "3des-cbc")]
    TripleDesCbc,

    #[strum(serialize = "blowfish-cbc")]
    BlowfishCbc,

    #[strum(serialize = "twofish256-cbc")]
    Twofish256Cbc,

    #[strum(serialize = "twofish-cbc")]
    TwofishCbc,

    #[strum(serialize = "twofish192-cbc")]
    Twofish192Cbc,

    #[strum(serialize = "twofish128-cbc")]
    Twofish128Cbc,

    #[strum(serialize = "aes256-cbc")]
    Aes256Cbc,

    #[strum(serialize = "aes192-cbc")]
    Aes192Cbc,

    #[strum(serialize = "aes128-cbc")]
    Aes128Cbc,

    #[strum(serialize = "serpent256-cbc")]
    Serpent256Cbc,

    #[strum(serialize = "serpent192-cbc")]
    Serpent192Cbc,

    #[strum(serialize = "serpent128-cbc")]
    Serpent128Cbc,

    #[strum(serialize = "arcfour")]
    Arcfour,

    #[strum(serialize = "idea-cbc")]
    IdeaCbc,

    #[strum(serialize = "cast128-cbc")]
    Cast128Cbc,

    #[strum(serialize = "none")]
    NoEncryption,
}

#[derive(Debug, Clone, Copy, PartialEq, Display, EnumString, IntoStaticStr)]
pub enum Hmac {
    #[strum(serialize = "hmac-sha1")]
    Sha1,

    #[strum(serialize = "hmac-sha1-96")]
    Sha1_96,

    #[strum(serialize = "hmac-md5")]
    Md5,

    #[strum(serialize = "hmac-md5-96")]
    Md5_96,

    #[strum(serialize = "none")]
    NoHmac,
}

#[derive(Debug, Clone, Copy, PartialEq, Display, EnumString, IntoStaticStr)]
pub enum Exchange {
    #[strum(serialize = "diffie-hellman-group1-sha1")]
    DiffieHellmanGroup1Sha1,

    #[strum(serialize = "diffie-hellman-group14-sha1")]
    DiffieHellmanGroup14Sha1,
}
