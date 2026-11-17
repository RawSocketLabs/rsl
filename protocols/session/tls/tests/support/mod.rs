//! One real TLS 1.2 connection, as an independent implementation put it on the wire.
//!
//! Golden vectors (characterization). Captured 2026-11-16 on loopback between
//! `openssl s_server` and `openssl s_client` (OpenSSL 4.0.3), from the client's `-msg` and
//! `-debug` output and its key log:
//!
//! ```text
//! openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=tls.test -addext subjectAltName=DNS:tls.test
//! openssl s_server -tls1_2 -cipher ECDHE-RSA-AES128-GCM-SHA256 -no_ticket -www
//! printf 'GET / HTTP/1.0\r\n\r\n' | openssl s_client -tls1_2 -servername tls.test \
//!     -sigalgs rsa_pss_rsae_sha256 -groups X25519 -msg -debug -keylogfile keylog.txt
//! ```
//!
//! The handshake constants are each one message (header and body) before record protection.
//! The record constants are whole protected records (header and fragment) as sent after
//! `ChangeCipherSpec`; each direction's sequence numbers start at 0 with its `Finished`.
//! The certificate is a throwaway self-signed one whose private key was discarded, and the
//! master secret is that one finished session's, from the key log.
//! Replace these only with a fresh capture, never by editing bytes to make a test pass.

#![allow(
    dead_code,
    reason = "each test binary uses a different subset of the capture"
)]

/// Decodes a hex string into bytes.
pub(crate) fn hex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

pub(crate) const CLIENT_HELLO: &str = concat!(
    "0100008f030338d5f8bf80d79d0977cee305230c5e74044962b2a0253b27555382760de8cb2a00002ec030cc",
    "a8c02fc027c014c013009dc09d009cc09c003d003c0035002f009fccaac09f009ec09e006b00670039003301",
    "000038ff01000100000b00020100000a00040002001d002300000016000000170000000d0004000208040000",
    "000d000b000008746c732e74657374",
);

pub(crate) const SERVER_HELLO: &str = concat!(
    "0200005703033ffd170ee88750c48367d606764aa89df97a6874b52f6e00ceac8a2fd20b41af20e52c177326",
    "9b222179c43fcf35d7c7a7ca2cd85bd52207dd65266b5b6fc9962fc02f00000fff01000100000b0002010000",
    "170000",
);

pub(crate) const CERTIFICATE: &str = concat!(
    "0b0003050003020002ff308202fb308201e3a00302010202142ce5292699622c18175b1a1786c8dc5998943d",
    "f2300d06092a864886f70d01010b050030133111300f06035504030c08746c732e74657374301e170d323631",
    "3131373031313035385a170d3336313131343031313035385a30133111300f06035504030c08746c732e7465",
    "737430820122300d06092a864886f70d01010105000382010f003082010a0282010100d19ef4a71aa5f6534c",
    "4e2ecb35daf288242b79d563045b0b9b9509bfc6f4a70e748e2e84f68c738369129196fcbd92047576c63449",
    "c4feaeb1b53f2e2f0104f8f6a78a049f628b4d97868bab46a6e42935fe99fa046adc9eba8d83a0055c2b4760",
    "bea0382788387e5e3eacfe9e298305639ea03b9d8b19c269ff7f7324d0ce6723acc09175be946e4cd1ae399e",
    "9a289079a6873e79272054b29d52d94f28e0cc465489846e90bf5d99d0bb7e5058ed5ec3b835a4555444a4e4",
    "3f2e78c50baa1a8124d7f4f8b522b61ba1fcfa0c1f660633022fb75564b719ab867a2c929453a9ac2dfb6a9f",
    "a218bda0b80a068acd397891ca2e61166a67950b346b6012b73fa90203010001a3473045301d0603551d0e04",
    "160414bfed6d90577ca46786f33a405a1549b0a9e87744300f0603551d130101ff040530030101ff30130603",
    "551d11040c300a8208746c732e74657374300d06092a864886f70d01010b05000382010100bef2ca812c8a60",
    "b4e7e3032a4a0cabdf4aac97723da0908264dbc80763f8b0ff9f75d3c6680ab99fe7a2ac8058e5771e436744",
    "4e9b90c2917d564ac23558700386ead6752aad779c7f7a3b953ea9947352bc1d8616bdb964db364fd41486e1",
    "4f843a3adb94596c7a08969aa3f3603f261141fa5219c17b5aa5b19b22ea7ce61f3c8300ed3bdae0ef599cb2",
    "583a38a9283d42c988dad86b0fd73002e0ecee2f285fbec112392c746d4b5bbd3d16b1cf303e1b7e3bf67deb",
    "d561afc89b89b9b1cca7eeba1bad13d5b84a5c6889baf17bc97dfc4e0f31889c98e68d883f8479d053dae014",
    "41191fec51128d4fd9775b50021dc5c8a70302cf1517e8439c27c1e936",
);

pub(crate) const SERVER_KEY_EXCHANGE: &str = concat!(
    "0c00012803001d2072a309e870a4be2798ad55cd67f633760faa8c8394d8d18f4b0af7f5d81d954608040100",
    "59cfc7d06f31a0e6374c0e75165f7d8fb23699f26e51747ed200124d08f40506c781468d08c8188ba940557a",
    "9d9a71d4ab86cd0d49a0e500e2793ec95a4ddc968aa9d5d400c9713671d887532b05eaeae5d0f9e1f648deae",
    "d001102f7343f7c7cb03794d5b82abc66d1cdda23b11eae19d63a92d9b0f8133c71241669866987d6290a720",
    "db5ede62fc746a9140db7093109fb34ffb2bf7fbcc6277cba680142e1ac4ffc1e58a3d60d8472456fc57fabc",
    "a013547fc162aeaec44e0f322e539af51a477ebb48556b482bea80f838e2aefd005e6d11d60c6694127d6dd5",
    "d72fe4d162a6aa7db60b3aa53417615f24184258b097ffd0756dbea1915c54243a318c4c",
);

pub(crate) const SERVER_HELLO_DONE: &str = "0e000000";

pub(crate) const CLIENT_KEY_EXCHANGE: &str =
    "100000212045084ea0f1f3cd998b579c3ff76b833c23ca5382e2cf9e77db4aff1179f73a3a";

pub(crate) const CLIENT_FINISHED: &str = "1400000c52f5a48b7625e4f9293bccb2";

pub(crate) const SERVER_FINISHED: &str = "1400000c829102b0ce40308b9e81af25";

/// The session's master secret, from the `CLIENT_RANDOM` line of the key log.
pub(crate) const MASTER_SECRET: &str = concat!(
    "a95e62134785189b2cc299e4dff7392a9b967c5d38e83e9209adc4996c005f806af569e2dee4cfece072fb61",
    "3d3b31e7",
);

/// The client's first protected record (sequence number 0): its `Finished` message.
pub(crate) const CLIENT_FINISHED_RECORD: &str = concat!(
    "1603030028656dae74cf3fb3933407d8a6aec6f83150dd81ee0a4af728bb9514b540a54e8e55f8bcf3e69894",
    "a9",
);

/// The client's second protected record (sequence number 1): the HTTP request.
pub(crate) const CLIENT_APPLICATION_RECORD: &str = concat!(
    "170303002a656dae74cf3fb39483f736b12556c716353e264906376d65e19471f1ef7cb7603f7b3f7bd9c3c1",
    "6b6928",
);

/// The client's third protected record (sequence number 2): a `close_notify` alert.
pub(crate) const CLIENT_CLOSE_RECORD: &str =
    "150303001a656dae74cf3fb395d21b50a7616e0a25f902ff663971d43c87fc";

/// The server's first protected record (sequence number 0): its `Finished` message.
pub(crate) const SERVER_FINISHED_RECORD: &str = concat!(
    "16030300280a31a0347a7683679cd6bf782090a3be944e63680a5abc18dfe77144b2e7706f756846c91dcca0",
    "8a",
);
