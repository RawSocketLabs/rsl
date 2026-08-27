use binrw::binrw;

#[binrw]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Encryption {
    #[brw(magic = b"edes-cbc")]
    TripleDesCbc,

    #[brw(magic = b"blowfish-cbc")]
    BlowfishCbc,

    #[brw(magic = b"twofish256-cbc")]
    Twofish256Cbc,

    #[brw(magic = b"twofish-cbc")]
    TwofishCbc,

    #[brw(magic = b"twofish192-cbc")]
    Twofish192Cbc,

    #[brw(magic = b"twofish128-cbc")]
    Twofish128Cbc,

    #[brw(magic = b"aes256-cbc")]
    Aes256Cbc,

    #[brw(magic = b"aes192-cbc")]
    Aes192Cbc,

    #[brw(magic = b"aes128-cbc")]
    Aes128Cbc,

    #[brw(magic = b"serpent256-cbc")]
    Serpent256Cbc,

    #[brw(magic = b"serpent192-cbc")]
    Serpent192Cbc,

    #[brw(magic = b"serpent128-cbc")]
    Serpent128Cbc,

    #[brw(magic = b"arcfour")]
    Arcfour,

    #[brw(magic = b"idea-cbc")]
    IdeaCbc,

    #[brw(magic = b"cast128-cbc")]
    Cast128Cbc,

    #[brw(magic = b"none")]
    NoEncryption,
}

#[binrw]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hmac {
    #[brw(magic = b"hmac-sha1")]
    Sha1,

    #[brw(magic = b"hmac-sha1-96")]
    Sha1_96,

    #[brw(magic = b"hmac-md5")]
    Md5,

    #[brw(magic = b"hmac-md5-96")]
    Md5_96,

    #[brw(magic = b"none")]
    NoHmac,
}

#[binrw]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Exchange {
    #[brw(magic = b"diffie-hellman-group1-sha1")]
    DiffieHellmanGroup1Sha1,

    #[brw(magic = b"diffie-hellman-group14-sha1")]
    DiffieHellmanGroup14Sha1,
}
