//! Public SHA3-256 and SHAKE256 validation harness.
//!
//! Differential oracles: the `sha3` crate (SHA3-256) and the `shake` crate (SHAKE256), both
//! development-only.
//!
//! Provenance: `tests/vectors/sha3/README.md`. The NIST per-step intermediate states are
//! white-box tests beside the implementation.

use rsl_crypto::digest::{
    Digest,
    sha3::{Sha3_256, Shake256},
};
use sha3::{Digest as _, Sha3_256 as ReferenceSha3_256};
use shake::{ExtendableOutput, Shake256 as ReferenceShake256, Update, XofReader};

fn decode(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2)
        .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).expect("fixture is hex"))
        .collect()
}

/// Published evidence: NIST `SHA3-256_Msg0.pdf` and `SHA3-256_1600.pdf` final hashes.
#[test]
fn nist_sha3_256_examples() {
    assert_eq!(
        Sha3_256::digest(b"").unwrap().as_bytes().as_slice(),
        decode("a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a")
    );
    // NIST prints the 1600-bit message as bits, least-significant first; each printed group
    // `1 1 0 0 0 1 0 1` is the byte 0xa3 (FIPS 202 Algorithm 10), so the message is 200 × 0xa3.
    let message = [0xa3_u8; 200];
    assert_eq!(
        Sha3_256::digest(message).unwrap().as_bytes().as_slice(),
        decode("79f38adec5c20307a98ef76e8324afbfd46cfd81b22e3973c65fa1bd9de31787")
    );
}

/// Published evidence: NIST `SHAKE256_Msg0.pdf` output for the empty message (first 512 bytes).
#[test]
fn nist_shake256_empty_message_output() {
    let expected = decode(
        "46b9dd2b0ba88d13233b3feb743eeb243fcd52ea62b81b82b50c27646ed5762fd75dc4ddd8c0f200cb05019d67b592f6fc821c49479ab48640292eacb3b7c4be141e96616fb13957692cc7edd0b45ae3dc07223c8e92937bef84bc0eab862853349ec75546f58fb7c2775c38462c5010d846c185c15111e595522a6bcd16cf86f3d122109e3b1fdd943b6aec468a2d621a7c06c6a957c62b54dafc3be87567d677231395f6147293b68ceab7a9e0c58d864e8efde4e1b9a46cbe854713672f5caaae314ed9083dab4b099f8e300f01b8650f1f4b1d8fcf3f3cb53fb8e9eb2ea203bdc970f50ae55428a91f7f53ac266b28419c3778a15fd248d339ede785fb7f5a1aaa96d313eacc890936c173cdcd0fab882c45755feb3aed96d477ff96390bf9a66d1368b208e21f7c10d04a3dbd4e360633e5db4b602601c14cea737db3dcf722632cc77851cbdde2aaf0a33a07b373445df490cc8fc1e4160ff118378f11f0477de055a81a9eda57a4a2cfb0c83929d310912f729ec6cfa36c6ac6a75837143045d791cc85eff5b21932f23861bcf23a52b5da67eaf7baae0f5fb1369db78f3ac45f8c4ac5671d85735cdddb09d2b1e34a1fc066ff4a162cb263d6541274ae2fcc865f618abe27c124cd8b074ccd516301b91875824d09958f341ef274bdab0bae316339894304e35877b0c28a9b1fd166c796b9cc258a064a8f57e27f2a",
    );
    let mut output = vec![0_u8; expected.len()];
    Shake256::digest_into(b"", &mut output);
    assert_eq!(output, expected);
}

/// Published evidence: CAVP `SHA3_256ShortMsg.rsp` lengths around the 136-byte rate.
#[test]
fn cavp_sha3_256_boundary_lengths() {
    let cases = [
        (
            0,
            "",
            "a7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a",
        ),
        (
            8,
            "e9",
            "f0d04dd1e6cfc29a4460d521796852f25d9ef8d28b44ee91ff5b759d72c1e6d6",
        ),
        (
            1080,
            "b1f6076509938432145bb15dbe1a7b2e007934be5f753908b50fd24333455970a7429f2ffbd28bd6fe1804c4688311f318fe3fcd9f6744410243e115bcb00d7e039a4fee4c326c2d119c42abd2e8f4155a44472643704cc0bc72403b8a8ab0fd4d68e04a059d6e5ed45033b906326abb4eb4147052779bad6a03b55ca5bd8b140e131bed2dfada",
            "f82d9602b231d332d902cb6436b15aef89acc591cb8626233ced20c0a6e80d7a",
        ),
        (
            1088,
            "56ea14d7fcb0db748ff649aaa5d0afdc2357528a9aad6076d73b2805b53d89e73681abfad26bee6c0f3d20215295f354f538ae80990d2281be6de0f6919aa9eb048c26b524f4d91ca87b54c0c54aa9b54ad02171e8bf31e8d158a9f586e92ffce994ecce9a5185cc80364d50a6f7b94849a914242fcb73f33a86ecc83c3403630d20650ddb8cd9c4",
            "4beae3515ba35ec8cbd1d94567e22b0d7809c466abfbafe9610349597ba15b45",
        ),
    ];
    for (bits, message, expected) in cases {
        let message = decode(message);
        assert_eq!(message.len() * 8, bits);
        assert_eq!(
            Sha3_256::digest(&message).unwrap().as_bytes().as_slice(),
            decode(expected),
            "Len = {bits}"
        );
    }
}

/// Published evidence: CAVP `SHAKE256ShortMsg.rsp` lengths around the rate (256-bit outputs).
#[test]
fn cavp_shake256_boundary_lengths() {
    let cases = [
        (
            0,
            "",
            "46b9dd2b0ba88d13233b3feb743eeb243fcd52ea62b81b82b50c27646ed5762f",
        ),
        (
            8,
            "0f",
            "aabb07488ff9edd05d6a603b7791b60a16d45093608f1badc0c9cc9a9154f215",
        ),
        (
            1080,
            "362f1eb00b37a9613b1ae82b90452579d42f8b1f9ede95f86badc6cdf04c9b79af08be4bc94d7cac136979026b92a2d44d2b642ea1431b47d75fce61367919f171486a007cc271d19de0d1c4c6a11c7a2251fe3aee0bb8938a7dd043d0eb0758a4768c95cc9f6f1703075839487879b47c29c10b2c3e5326ac8f363c65aa4ef76f1b8bd363eb60",
            "c6ce60c1852ea780ed845aac4ca6a30e09f5c0064c9675865178717cfeb1dc97",
        ),
        (
            1088,
            "d8f12b97f81d47aebbfb7314ff04172cf2be71c3778e238bcccdeecb691fbd542b00e5b7b1a0abb507f107f781fea700ea7e375fdea9e029754a0ea62216774bda3c59e8783d022360fe9625621c0d93e27f7bc03632942150716f019d048a752ccc0f93139c55df0f4aaa066a0550cf22e8c54e47d0475ba56b9842a392ffbc6bd98f1e4b64abd1",
            "e2e1c432dd07c2ee89a78f31211c92eeb5306c4fa4db93c4e5cd43080d6079e4",
        ),
        (
            1096,
            "a10d05d7e51e75dc150f640ec4722837220b86df2a3580ca1c826ec22ea250977e8663634cc4f212663e6f22e3ffc2a81465e194b885a1356fcbcc0072e1738d80d285e21c70a1f4f5f3296ba6e298a69f3715ff63be4850f5be6cb68cdba5948e3b94dbbce82989aa75b97073e55139aac849a894a71c2294a2776ce6588fb59007b8d796f434da6e",
            "02f17bf86dc7b7f9c3fb96e4b3a10ca574cd0f8dedda50f3dda8008ce9e8fec9",
        ),
        (
            2168,
            "a1ce406d09c02ca1c3cc97f25c9b96eeb9d3480c053b67edee201ce111f718adb243e08cb1b96873b9a2e499bb82db379bf77d8c29e3362552cd835b9885051dbf07d4f0c9a21533255acfa8248afa30acced9d6301f8a0fdf40dc401c5a60812fe3c3a13ac35a6a6ebaff267efc1c62541f05f104378b34fe40ce9987bc52626a9c55a8ea64609ffc8b1d1bb3206853fcb4a8e58b73601b1594016dc0f40347d8fd330cc3cd4f5a3794d090aa3e078d0a536bbbaf1b324d8e051ec4af499ab8e1cd05d5ac464e24879fc18b4b9e2098e8c5f67a56762850cf5bcda73c053f3dedd3720b8c168583547297177e88dcfafcd1f02a6acd6bae425eb51c4f6e1c5f64d823b74d861d0106d7fb392b4363",
            "a7835b81740824ebfc4a0dda40da7a70a66f9f3a8ea77cf857664ff955c5c5fe",
        ),
        (
            2176,
            "104fefe89f08d15d36a2233f42a7defa917c5ad2642e06cac56d5cc51ad914ecfb7d984f4199b9cf5fa5a03bf69207b9a353a9681c9cf6437bea0c49d9c3e3db1f3fc76519c70c40cc1dfdd70a9c150943c272cf9eeb861f485f10100c8f4a3e259c6470501932782512225ba64d70b219cf9d5013a21d25d6d65062dcc6b3deb49d58b90d18933f118df70ff42c807ccc851233a34a221eca56b38971ef858475488988794a975d3894633a19c1ae2f05e9b9c0756affd3cfe823ccf29228f60fa7e025bc39a79943325126409460926b057a3fb28a1b098b938872883804fd2bc245d7fd6d29bcda6ca6198f2eff6ea7e03ef78133de8ba65fc8c45a688160719fa1e7646d878ea44c4b5c2e16f48b",
            "46293a63c235750d58a24edca5ba637b96cae74325c6c8122c4155c0d15805e6",
        ),
    ];
    for (bits, message, expected) in cases {
        let message = decode(message);
        assert_eq!(message.len() * 8, bits);
        let expected = decode(expected);
        let mut output = vec![0_u8; expected.len()];
        Shake256::digest_into(&message, &mut output);
        assert_eq!(output, expected, "Len = {bits}");
    }
}

/// Published evidence: CAVP `SHAKE256VariableOut.rsp` cases including outputs at and across
/// the 1088-bit rate.
#[test]
#[allow(clippy::too_many_lines)] // The published output strings are long.
fn cavp_shake256_variable_output_lengths() {
    let cases = [
        (
            0,
            16,
            "c61a9188812ae73994bc0d6d4021e31bf124dc72669749111232da7ac29e61c4",
            "23ce",
        ),
        (
            1,
            16,
            "74d7980949c1dc759a4a10acc3ab994b771ae6d8b5ef0005f8046233af610c36",
            "77cd",
        ),
        (
            2,
            16,
            "4f865b9ff82cc68705fbb6decb84cbd48f880e5b49b0d77ea77eeef45584f0f5",
            "6ade",
        ),
        (
            3,
            16,
            "5e30de9794d269e22aead3ce26f4f6dfceb1e3eb6ad5cb744b0020350cf0f7fb",
            "d436",
        ),
        (
            4,
            16,
            "3fa5f3b5dfbff118e07eb21d339a5a6bb60d52d8b67feb7eb102441160ff6d70",
            "28bc",
        ),
        (
            70,
            128,
            "dc886df3f69c49513de3627e9481db5871e8ee88eb9f99611541930a8bc885e0",
            "00648afbc5e651649db1fd82936b00db",
        ),
        (
            71,
            128,
            "e3ef127eadfafaf40408cebb28705df30b68d99dfa1893507ef3062d85461715",
            "7314002948c057006d4fc21e3e19c258",
        ),
        (
            72,
            128,
            "76891a7bcc6c04490035b743152f64a8dd2ea18ab472b8d36ecf45858d0b0046",
            "e8447df87d01beeb724c9a2a38ab00fc",
        ),
        (
            73,
            128,
            "445b17ce13727ae842b877c4750611a9eb79823bc5752da0a5e9d4e27bd40b94",
            "e7708cdc22f03b0bfaca03e5d11d46ca",
        ),
        (
            74,
            128,
            "6ae23f058f0f2264a18cd609acc26dd4dbc00f5c3ee9e13ecaea2bb5a2f0bb6b",
            "b9b92544fb25cfe4ec6fe437d8da2bbe",
        ),
        (
            670,
            1088,
            "dc886df3f69c49513de3627e9481db5871e8ee88eb9f99611541930a8bc885e0",
            "00648afbc5e651649db1fd82936b00dbbc122fb4c877860d385c4950d56de7e096d613d7a3f27ed8f26334b0ccc1407b41dccb23dfaa529818d1125cd5348092524366b85fabb97c6cd1e6066f459bcc566da87ec9b7ba36792d118ac39a4ccef6192bbf3a54af18e57b0c146101f6aeaa822bc4b4c9708b09f0b3bab41bcce964d999d1107bd7c2",
        ),
        (
            671,
            1088,
            "e3ef127eadfafaf40408cebb28705df30b68d99dfa1893507ef3062d85461715",
            "7314002948c057006d4fc21e3e19c258fb5bdd57728fe93c9c6ef265b6d9f559ca73da32c427e135ba0db900d9003b19c9cf116f542a760418b1a435ac75ed5ab4ef151808c3849c3bce11c3cd285dd75e5c9fd0a0b32a89640a68e6e5b270f966f33911cfdffd03488b52b4c7fd1b2219de133e77519c426a63b9d8afac2ccab273ebd23765616b",
        ),
        (
            672,
            1088,
            "76891a7bcc6c04490035b743152f64a8dd2ea18ab472b8d36ecf45858d0b0046",
            "e8447df87d01beeb724c9a2a38ab00fcc24e9bd17860e673b021222d621a7810e5d3dcead3f6b72810ff1ad242bf79074d2fd63503cbe7a2ffe81b1c57566568b01dda7b440ad27aee54d2f8696615008efee01c682dae7d875aa21ab3914d063d21f1d97fa9d57709ebbab376a88b1da805f0fc5ab8370cd3b714613fd8e5939f972d72fd5dff9e",
        ),
        (
            673,
            1088,
            "445b17ce13727ae842b877c4750611a9eb79823bc5752da0a5e9d4e27bd40b94",
            "e7708cdc22f03b0bfaca03e5d11d46cac118fded60b64bf4acffb35b0b474fbe85d270e625b95d54157d6597eb4fbdfa482e636d4a44c9de13c71387654c1a254a85063dd7720ffd5c6fc50ab97914c67ce6f0da5ae14ec0f2c5cdad79c4d85415279d21e236519dc1422c5b6dd156ffe432c14f40eb458f21e20527b23c03e299736adc12620303",
        ),
        (
            674,
            1088,
            "6ae23f058f0f2264a18cd609acc26dd4dbc00f5c3ee9e13ecaea2bb5a2f0bb6b",
            "b9b92544fb25cfe4ec6fe437d8da2bbe00f7bdaface3de97b8775a44d753c3adca3f7c6f183cc8647e229070439aa9539ae1f8f13470c9d3527fffdeef6c94f9f0520ff0c1ba8b16e16014e1af43ac6d94cb7929188cce9d7b02f81a2746f52ba16988e5f6d93298d778dfe05ea0ef256ae3728643ce3e29c794a0370e9ca6a8bf3e7a41e8677067",
        ),
        (
            675,
            1096,
            "8d8001e2c096f1b88e7c9224a086efd4797fbf74a8033a2d422a2b6b8f6747e4",
            "2e975f6a8a14f0704d51b13667d8195c219f71e6345696c49fa4b9d08e9225d3d39393425152c97e71dd24601c11abcfa0f12f53c680bd3ae757b8134a9c10d429615869217fdd5885c4db174985703a6d6de94a667eac3023443a8337ae1bc601b76d7d38ec3c34463105f0d3949d78e562a039e4469548b609395de5a4fd43c46ca9fd6ee29ada5e",
        ),
        (
            676,
            1096,
            "afc9ef4e2e46c719120b68a65aa872273d0873fc6ea353859ff6f034443005e6",
            "45c65255731e3679b4662f55b02bc5d1c8038a1d778fe91144a5c7d3a286c78c54f52135134a3c6a19a9e6e546de21b2e8a7e280290709f0e482a51bffa95137a381268d10195862818309b2a4954c656d1725c7ad1a29973162832d62afd538cf74e1b70d1775a9f77dc7c7380ea034f5b1869af46c1c26bce29e1980f0de9e55543e7eda19a56453",
        ),
        (
            677,
            1096,
            "7935b68bb334f35ddc157a8c473349eb03ad0e41530d3c045e2c5f642850ad8c",
            "b44d25998e5cf77a83a4c0b2aae3061785adc7507d76fe07f4dcf299e04c991c922b51570fb843ab04cce25de258fda0560454c0e17be715d8051f388c48351e72ce0f8df8daa7643d3659e0e7be600a584039a14f85ad695ce143b923295e2d00c9a4394d4973302706bbbc8ddf01da7154740577c5de11b7938ece4eceb169c896d5d52ce3fe715f",
        ),
        (
            678,
            1096,
            "3e20cf32669fa3fd6e94e519b52a1dba33cd1f3a6947975e9829e4db326d2a18",
            "3389aea66244b91428f0896be26a9c3cfc5c1be2f07514f5d4718a1ed31855e148c4aa19b9f50f7619b04a1338b58fcb9b953d214f8218faa0e4b2daf467300283c96192d32d48b5e6801cf1560b72b5e8a418ee534e2e9ede69071403782ae08b128f236040d64f926db52af6aa532543ec211e90fdb72c9ed0efac1c8cd72357ba08310887c32618",
        ),
        (
            679,
            1096,
            "7d9312ffe94845ac51056c63eb3bff4a94626aafb7470ff86fa88fd8f0fe45c9",
            "de489392796fd3b530c506e482936afcfe6b72dcf7e9def054953842ff19076908c8a1d6a4e7639e0fdbfa1b5201095051aac3e3997779e588377eac979313e39c3721dc9f912cf7fdf1a9038cbaba8e9f3d95951a5d819bffd0b080319fcd12da0516baf54b779e79e437d3ec565c64eb5752825f54050f93b0a0f990dc8747aae6d67d5ca8d00c98",
        ),
    ];
    for (count, bits, message, expected) in cases {
        let expected = decode(expected);
        assert_eq!(expected.len() * 8, bits);
        let mut output = vec![0_u8; expected.len()];
        Shake256::digest_into(decode(message), &mut output);
        assert_eq!(output, expected, "COUNT = {count}");
    }
}

/// Standard-derived evidence: squeezing in pieces equals one squeeze; fragmentation of input
/// does not matter; the generic `Digest` contract matches.
#[test]
fn incremental_squeeze_and_fragmented_absorb_are_consistent() {
    let message: Vec<u8> = (0..1000).map(|i| u8::try_from(i % 251).unwrap()).collect();
    let mut whole = vec![0_u8; 300];
    Shake256::digest_into(&message, &mut whole);
    let mut xof = Shake256::new();
    for part in message.chunks(37) {
        xof.update(part);
    }
    let mut pieces = vec![0_u8; 300];
    let mut position = 0;
    for length in [1, 135, 1, 100, 63] {
        xof.squeeze(&mut pieces[position..position + length]);
        position += length;
    }
    assert_eq!(pieces, whole);

    let mut generic = <Sha3_256 as Digest>::new();
    for part in message.chunks(17) {
        Digest::update(&mut generic, part).unwrap();
    }
    assert_eq!(
        Digest::finalize(generic),
        Sha3_256::digest(&message).unwrap()
    );
}

/// Differential evidence against the `sha3` crate over rate and padding boundaries.
#[test]
fn varied_lengths_match_rustcrypto() {
    for length in [0_usize, 1, 7, 135, 136, 137, 271, 272, 273, 1024, 4096] {
        let message: Vec<u8> = (0..length)
            .map(|i| (i * 73 + 19).to_le_bytes()[0])
            .collect();
        assert_eq!(
            Sha3_256::digest(&message).unwrap().as_bytes().as_slice(),
            ReferenceSha3_256::digest(&message).as_slice(),
            "SHA3-256 length {length}"
        );
        let mut reference = ReferenceShake256::default();
        reference.update(&message);
        let mut expected = vec![0_u8; 300];
        reference.finalize_xof().read(&mut expected);
        let mut ours = vec![0_u8; 300];
        Shake256::digest_into(&message, &mut ours);
        assert_eq!(ours, expected, "SHAKE256 length {length}");
    }
}
