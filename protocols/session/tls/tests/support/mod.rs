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

/// A deterministic stand-in for a random source: every byte it yields is one more than the
/// last, starting at 1. It makes a client's output reproducible, so a recorded connection can
/// be replayed. It is not random and must never leave the tests.
#[derive(Default)]
pub(crate) struct CountingRandom(u8);

impl crypto::RandomSource for CountingRandom {
    fn fill_bytes(&mut self, output: &mut [u8]) -> crypto::Result<()> {
        for byte in output {
            self.0 = self.0.wrapping_add(1);
            *byte = self.0;
        }
        Ok(())
    }
}

/// What a test playing the server needs to continue a recorded session: its master secret,
/// both hello randoms, and the transcript up to and including the client's `Finished`.
pub(crate) struct ReplaySession {
    pub(crate) master_secret: tls::schedule::MasterSecret,
    pub(crate) transcript: tls::handshake::Transcript,
    /// The client random, then the server random.
    pub(crate) randoms: ([u8; 32], [u8; 32]),
}

impl ReplaySession {
    /// Re-derives a session that a [`CountingRandom`] client took part in, from the records
    /// both sides sent in the clear: the client's hello, the server's first flight, and the
    /// client's second flight (of which only the records before its `ChangeCipherSpec` are
    /// read). The client's private key is known because its randomness is reproducible; the
    /// server's public key is in its `ServerKeyExchange`.
    pub(crate) fn derive(
        client_hello: &[u8],
        first_flight: &[Vec<u8>],
        client_flight: &[Vec<u8>],
    ) -> Self {
        use crypto::agreement::x25519::{X25519, X25519PrivateKey, X25519PublicKey};
        use tls::{
            handshake::{HandshakeJoiner, HandshakeMessage, Transcript},
            schedule::{ConnectionEnd, MasterSecret},
            wire::{HandshakeType, ServerHello, ServerKeyExchange},
        };

        // CountingRandom yields 1..=32 for the client random, then 33..=64 for the key.
        let (mut client_random, mut scalar) = ([0; 32], [0; 32]);
        for (byte, value) in client_random.iter_mut().chain(&mut scalar).zip(1..) {
            *byte = value;
        }

        let mut joiner = HandshakeJoiner::new();
        let mut transcript = Transcript::new();
        let (mut server_random, mut server_public) = (None, None);
        let sent_in_clear = client_flight.iter().take_while(|record| record[0] == 22);
        let client_hello = client_hello.to_vec();
        for record in [&client_hello]
            .into_iter()
            .chain(first_flight)
            .chain(sent_in_clear)
        {
            joiner.push(&record[5..]);
            while let Some(message) = joiner.next_message().unwrap() {
                match message.msg_type() {
                    HandshakeType::ServerHello => {
                        let hello = ServerHello::decode_exact(message.body()).unwrap();
                        server_random = Some(hello.random);
                    }
                    HandshakeType::ServerKeyExchange => {
                        let exchange = ServerKeyExchange::decode_exact(message.body()).unwrap();
                        server_public = Some(exchange.params.public);
                    }
                    _ => {}
                }
                transcript.add(&message);
            }
        }
        let server_public = X25519PublicKey::new(server_public.unwrap().try_into().unwrap());
        let shared = X25519::agree(&X25519PrivateKey::new(scalar), &server_public).unwrap();
        let master_secret =
            MasterSecret::derive_extended(shared.expose_secret(), &transcript.hash()).unwrap();
        let client_verify = master_secret
            .finished_verify_data(ConnectionEnd::Client, &transcript.hash())
            .unwrap();
        transcript.add(&HandshakeMessage::new(
            HandshakeType::Finished,
            &client_verify,
        ));
        Self {
            master_secret,
            transcript,
            randoms: (client_random, server_random.unwrap()),
        }
    }

    /// Both directions' record keys.
    pub(crate) fn keys(&self) -> tls::record::TrafficKeys {
        self.master_secret
            .aes128_gcm_keys(&self.randoms.0, &self.randoms.1)
            .unwrap()
    }
}

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

// --- A recorded connection made by this crate's client ---
//
// Captured 2026-11-16 by `tests/interop.rs` with `TLS_RECORD_DIR` set: the client, seeded by
// `CountingRandom`, against `openssl s_server -tls1_2 -rev` (OpenSSL 4.0.3) with a fresh
// throwaway certificate. The client sent `hello tls\n`, the server answered `slt olleh\n`,
// the client closed and the server closed in reply. Everything the server sent was produced
// by OpenSSL. Because the client's randomness is reproducible, replaying the server's bytes
// must make the client emit exactly the client's bytes again.

/// Every byte the client sent, in order: `ClientHello`; `ClientKeyExchange`,
/// `ChangeCipherSpec` and `Finished`; one application-data record; `close_notify`.
pub(crate) const REPLAY_CLIENT: &str = concat!(
    "160301005f0100005b03030102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f2000",
    "0002c02f010000300000000d000b000008746c732e74657374000a00040002001d000b00020100000d000400",
    "02080400170000ff01000100160303002510000021205869aff450549732cbaaed5e5df9b30a6da31cb0e574",
    "2bad5ad4a1a768f1a67b14030300010116030300280000000000000000e2a4010d9412a81c20fc7d31af4082",
    "44f2fa6058158389ad8aa91b25016f553617030300220000000000000001cc9bad857041c58ae8b54562f65e",
    "d6d75004b91c4673bead9b66150303001a0000000000000002e8c56ecbdf4acc7fd70ed142b7256f00936f",
);

/// Every byte the server sent, in order: `ServerHello`, `Certificate`, `ServerKeyExchange`
/// and `ServerHelloDone`; `ChangeCipherSpec` and `Finished`; one application-data record;
/// `close_notify`.
pub(crate) const REPLAY_SERVER: &str = concat!(
    "160303005b02000057030399f0f4721ca2b1b12d46e74681aa4495a628393259109fff91485952d6707ab420",
    "a9f1bce0cab46f5af40982992cee90535d8d3003923da191a1e07261c0e2f6d5c02f00000fff01000100000b",
    "000201000017000016030302f40b0002f00002ed0002ea308202e6308201cea003020102021469c27f66c73d",
    "7eb69cf2b2723b7811b987aed8ea300d06092a864886f70d01010b050030133111300f06035504030c08746c",
    "732e74657374301e170d3236313131373031333831375a170d3236313131393031333831375a30133111300f",
    "06035504030c08746c732e7465737430820122300d06092a864886f70d01010105000382010f003082010a02",
    "82010100a8ad5f3c8909bde2cf5a0eec060b2ea03c3170664e6bf31bb9358b6e0bae62563c513ee067a88240",
    "8f891d1ba918d89dfb69f669f6d36557da6513b5b215f30a524cc78963ee1b7f96c22204c02846cfb08c0761",
    "bb3e2590d4b8aac7088e19817a4715f24892464d5b21d329728926f52f9cf2f045fa702cc6e30034542334ad",
    "31a4be87c5a040781c37015dfb9ccefd9200f28b4200cc802328604757f2bbb5a961b14b4e1cdaf91586776a",
    "40e7746b34e0083a0e2b815b62c4e1ee51bcedbebe694012bc998d6b8dad1ff2531e1b6798fe29f0b3fd82df",
    "a8dd7e9e0bdca9cc6900c9db5dc5470b0a5c8413a9fbe9457ca5c790869738cbc5dd625902fb1d9302030100",
    "01a3323030301d0603551d0e04160414d83cc4078e7638b77eb980ae22b968a2f4d88bb0300f0603551d1301",
    "01ff040530030101ff300d06092a864886f70d01010b05000382010100195227a72179d851828aca39a135de",
    "cc8b010999b0f05be4a506bebc9215e3a415c9d43df5c9e9cc340a847b796bce2568976ba9f3b76696cabeb7",
    "fea4b8bdeea092baee08a13dab00a8b0b3a7697de9b2f7c6b5a7bd12fd62705d23b103de36b5c91a74f2eda1",
    "2afb65487bfae03a2681246d4c578e769b2f14234a6acdcab29090f364a38f4b7f8ed8e113589acbe871eef2",
    "305017bab75b9ad55034a59d8a863f16fb322d02ebd944f52acd34fdc1164447339b66bbd7e8be5ff87f8ac3",
    "c33a9512f81cac79615989bbcf322c091ecd642d857030fc7f1986cf184518a04e46f737ce775a1c66d57471",
    "a93f29eeefea8028ea0fdaa0e9019bd3603fa3a060160303012c0c00012803001d20b0fb9a4b8666f44132c4",
    "dd3aa8b9a37fbe20a4dd365c4d1200086806d6ad8c1a080401001cd17310032cc4bb82768b67f52ed2f619d2",
    "9ec5e4f6df8747d9d285fc08782c387aab1bfeab5398d128b11b7a81fb8211774123abf0fbae5f89adbe71a8",
    "43615b6f9798312d37cfa7b1e4453bd4d2327c5d26352cbd25a1fc99053c9df831da11d6432c6fe29b7af957",
    "c4d55e4e7e9962a4a54bace9bbb93d07aba94f7b061e3d2ca982d90ddb24f50ccf99d384c0e3fec63b5da773",
    "c4f44713e6886e8afaa90afd2b6dadd74d5b91b5c3eb1b45a2f40431757098adde3f67b3c8fc8fa4a5cf2cc8",
    "d48e9b89f4677d41121164485b3baeea325dce2311c4284a112b009ce10d99d5907d043d6850282efdbef108",
    "d9c2cc92d4f180428f0827d95d27ebd7be6516030300040e0000001403030001011603030028b55c4c920ff2",
    "5b6035a933a395bb7d9bf16dd2e3a5ce61f3f4ab4278f052f74df2f311f3bc3ca72b1703030022b55c4c920f",
    "f25b61091b10321117f4dd08af0ca592008d557dd8adcda79a8c004fd2150303001ab55c4c920ff25b62393f",
    "e6c79739e47697a18c87d5eb2ed757f5",
);

/// A certificate whose RSA public key has an 8200-bit modulus, just over what the client
/// will verify with. Made 2026-11-16 with Python `cryptography` from a random odd number,
/// not a real key, and signed by a discarded Ed25519 key; nothing can sign for it.
pub(crate) const OVERSIZED_RSA_CERTIFICATE: &str = concat!(
    "308204ce30820480a003020102020101300506032b657030133111300f06035504030c08746c732e74657374",
    "301e170d3236313131363030303030305a170d3336313131333030303030305a30133111300f06035504030c",
    "08746c732e7465737430820423300d06092a864886f70d010101050003820410003082040b0282040200ada2",
    "691b761dcc7aa21f8f0902d3dee34996c583ff18bb99d83daeca23119f264d75e496c4aa1e74b39c45201cf6",
    "44d3499ea75a836dd98eda295c884f35066b6bad3b174f1347ea620e23b180ee32ace073ee28ec56c9944e35",
    "69076a5f2605992ffab89d82a9fa73ba133a819e65c464dfb6c2dd212dca2000c3c504a432b3e6045fe5f0f5",
    "12a2a1d42125d87ed5cd1048926427fd86e952b9ff9fd29810fa969fa6598aac00eaab166d7ee8ec20673d97",
    "019cb612efec3d2c5b74b455f11b36b4967a5d42e865d41eaface72391b51055d9023b09a088e6be827b8126",
    "5fe708a826f0c0ad5b03d8aa02a16f788bd17d90cb4dc2b7415c1a0513f95d2d5ff36a834cba0f0f3474529e",
    "4678f76164f93885210d20a8a8aa6a80bf23bc10ee97a8108ecb22b9a9adfcab74909b7295b84a16854687ae",
    "1c8be3bc9419c6c8314a9ff6c097ddb2acb481aac0d8b7c2458bf1d5f03add1d56023e531882f419b33102a6",
    "256f70a5a4e4f9eb0a5915ed94488715baa345bfbdea1ad196bdbe640a08b5c93d19963d15e969a796f223ea",
    "68b5fb9668a8bd5cd410280bae3ace93f8779907f03837255383215ad6b9695c90a91c86d26787f0e8cd28e5",
    "706868d3fa385ac35c9201427b9d7d685afccba41fd65c03479d93cdddf6d4e86986720eadacc35b239a11a0",
    "6a21388fc35f1429366f03395e18fd85b653452ad2bd7ce317c5e93b5212fdeb9863b3a498adf74b11338b9f",
    "c54a308fd773fa80741753915c97b80b970d29ac06f4e42b46cfc8d3922f80712f42b5dcdb22a8c8f3b57579",
    "e2e18ce69aecd33388b609d0346e3236f21c8063f8c33a6132d6c9aa2522a81c3cecafa4bb843b85703c21f9",
    "ac40be2254bcc66f87a6f6c7c9f12c5aab0fa2c83947aeb8f9cf0fe4b3ac8a47e4f6dbc8ca0a7cd4b09a33a6",
    "3e2e9d7a0f8cf001e36f3157e0cc229d75ff85b82b2c964e59a09d9e67a6d6cb3080d5190278582a6afbc877",
    "69a054703b4a12e54165d10428c07b546cd33b884b76662076117e439a9ff59edcd08e105aa44eea1617f88f",
    "e42590af7ea40dbcb7990aa7b24ad0d7462564236769fbe17050fb6f1aa3edb54c3335994ec03ffdac3a97df",
    "4e8d4c6aa3aa561e54affdf1a86529204d381e1715de2d07321f6aebe9bf7ef1c9fda9d5c9172932ebcb866d",
    "30cb75d0b246a4c9f4817249cac7a8ac98856c9c178b1b031895770bc416fd535ca8bac1123b53124d9846e8",
    "3cbd40f3d7a6779574f5410a4fb98d26f97a9ac51f2bd6610a1f8e9aff3d424030691a597bad7a5dfdd5cfed",
    "008d7122f790e2b933bf9f326d3b36d2b22552d476378ebb685aa70854e2c3ed0fc613fc1f490553c57f91ac",
    "a998ca7fd7e571ace6e06157f2eca901d4215263fe254d7e8a165ef583d7ad57df6b21b35f7fec7db6c8ab43",
    "5fb2486d87bf114edf7bcb0203010001300506032b6570034100a4cc444447eaadf0d2d7e542d8c329971383",
    "f1d1f16cebb7a92feabd036d99dd2221ac9a9972be7fb53307e84abc5b2c7fa30b41677831d5e03d54b24869",
    "1f05",
);
