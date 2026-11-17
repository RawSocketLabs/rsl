//! One real TLS 1.2 handshake, as an independent implementation put it on the wire.
//!
//! Golden vectors (characterization). Captured 2026-11-16 on loopback between
//! `openssl s_server` and `openssl s_client` (OpenSSL 4.0.3), from the client's `-msg` output:
//!
//! ```text
//! openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=tls.test -addext subjectAltName=DNS:tls.test
//! openssl s_server -tls1_2 -cipher ECDHE-RSA-AES128-GCM-SHA256
//! openssl s_client -tls1_2 -servername tls.test -sigalgs rsa_pss_rsae_sha256 -groups X25519 -msg
//! ```
//!
//! Each constant is one handshake message (header and body) or alert, in hex, before record
//! protection. The certificate is a throwaway self-signed one; its private key was discarded.
//! Replace these only with a fresh capture, never by editing bytes to make a test pass.

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the transcript"
)]

/// Decodes a hex string into bytes.
pub(crate) fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

pub(crate) const CLIENT_HELLO: &str = concat!(
    "0100008f03037fe766c31d539bb7cb03fb9a03449719f604c77c2c782a4adb94bc5a104c2c6e00002ec030cc",
    "a8c02fc027c014c013009dc09d009cc09c003d003c0035002f009fccaac09f009ec09e006b00670039003301",
    "000038ff01000100000b00020100000a00040002001d002300000016000000170000000d0004000208040000",
    "000d000b000008746c732e74657374",
);

pub(crate) const SERVER_HELLO: &str = concat!(
    "0200003b03032f065f08464cdd408348e780cabe966618d4c675d4a7ce56aa618cdb27984a6c00c02f000013",
    "ff01000100000b000201000023000000170000",
);

pub(crate) const CERTIFICATE: &str = concat!(
    "0b0003050003020002ff308202fb308201e3a00302010202146b519dd0e48f280fc016026704ed3d4c35f9d5",
    "d0300d06092a864886f70d01010b050030133111300f06035504030c08746c732e74657374301e170d323631",
    "3131373030353233315a170d3336313131343030353233315a30133111300f06035504030c08746c732e7465",
    "737430820122300d06092a864886f70d01010105000382010f003082010a0282010100b347a94a02959b7df4",
    "8330722ddf8014d956bb6d934d56b7e29e4fa68ef9020807dc05ce64f8541afaafeafdcb7b69ba8d7c8a99b4",
    "c3d6a9c53e9044ea04e1c8b9f0c46d491a05d41be32e4abdc659770a8624a7065c2c189e3156867a257d0762",
    "a3ff3de4f9192b9583f660424d8f52dea63a9b9642e8c794b84258aa818b2d25320fdef15403ae3c6e35df46",
    "59258dbb2461a7a0c4cae7b6f3ac625f1381205a2065efffcb245f3347543277eb732a3f700490b5a3cd0f23",
    "bc2e7e379c26c64eba05efba86ac6304d1e430ecb7a39bfb2a4839a5f830fd1f5ec90c6932c615b73797ef10",
    "2ab98c8d37050f7326a802e6049d3e4ff95e675d2021595d28232f0203010001a3473045301d0603551d0e04",
    "16041463e52f9e5462a257b56097a76a95a72e0490296b300f0603551d130101ff040530030101ff30130603",
    "551d11040c300a8208746c732e74657374300d06092a864886f70d01010b050003820101006d6b96fd1ec05e",
    "81e210e05977aebec04c2d5dde3e6a8bb824a31ededbb504204936fce4ed3e97ad6ff17a7ecf2c584ada15ed",
    "f820389b058b3b737517d6189e3f711c247c8966ee9b6f8375b6dbfbf9c0d74af18c94e354207d448d964b19",
    "dc806b4b328f5ac08e263eb37bf6d8a48cb785ae4b3245c985d688fdc6f644e2c030a5dfb400b2851ebaa22b",
    "dc1b05076687e4fa019dc87c5d18eeba1383839500d1ce803b4f31e0f49c894bc5534f0d50b8c4ef96c0989d",
    "29dce1c3f2c99d972031354e8b7e715f263db5d2fb6ce354dfa677ae269e10787221755df99d40cf73ff759c",
    "6ccf72c793da3b958ef7fc699e436f690f31fa986c6661957702569371",
);

pub(crate) const SERVER_KEY_EXCHANGE: &str = concat!(
    "0c00012803001d2073899e761f756f3a662757295b7bfe76c807c06b5e0b6e2ce9a5aa3ddf78194408040100",
    "9833c3ea8358dbff1a600840a43c88cb7debaceb80a80cce7fffac9f29964eff549c4e9e7a3929a6cff6bcb8",
    "08d6a7610e6ac87c5d417040d686fb9073e3730c38e6f35d99842dd5ca9e2e99f2a807481a32ffc469dc992f",
    "a5bfbff25d2dcd31f8ae6c2965acd31c451e6f07e0b6b2d9af1e9bb93c7373beddfbfc8957aeca7c13f3fccd",
    "30c1d9f1aef47f366d71110e05a6a44f30b360d6b54011221fee588b4353f3b8d07ddba1b143ac5290c511ab",
    "25ab5e66c1af5de22d05f5a3ca529c006f2e29c49b54ce5c629641a4ee6fc03f46f86746edd7a67f1ef2e93d",
    "686c4adbc1f6851e0ca7241030aa5f25ba2fa0158a462ff4e52a2bb677abc7f50079c07b",
);

pub(crate) const SERVER_HELLO_DONE: &str = "0e000000";

pub(crate) const CLIENT_KEY_EXCHANGE: &str =
    "1000002120a42bcd96b53535e7a8d895b553361adf926c069c1442395d5d8e1ead88e2a648";

pub(crate) const CLIENT_FINISHED: &str = "1400000ca93108b4a7c8b145e4be5c40";

pub(crate) const CLOSE_NOTIFY: &str = "0100";
