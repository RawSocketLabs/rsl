//! Feed the same record stream whole and in pieces, checking the client's observable state.
#![no_main]

use libfuzzer_sys::fuzz_target;
use sangfor_fuzz::Counting;
use tls::{
    Client, ClientConfig, Error, ServerVerification,
    handshake::HandshakeMessage,
    record::WriteState,
    wire::{CipherSuite, ContentType, Extension, HandshakeType, ProtocolVersion, ServerHello},
};

fn handshake(kind: HandshakeType, body: &[u8]) -> Vec<u8> {
    let message = HandshakeMessage::new(kind, body);
    WriteState::plain()
        .seal(
            ContentType::Handshake,
            ProtocolVersion::TLS_1_2,
            message.bytes(),
        )
        .unwrap()
}

fn receive(wire: &[u8], stride: usize) -> (Result<(), Error>, Vec<u8>, Vec<u8>, bool) {
    let config = ClientConfig {
        server_name: Some("tls.test"),
        verification: ServerVerification::Unverified,
    };
    let mut client = Client::new(config, &mut Counting::default()).unwrap();
    client.sent(client.transmit().len());
    let result = wire.chunks(stride).try_for_each(|part| {
        // The entire input fits below the plaintext backlog threshold.
        assert_eq!(client.receive(part)?, part.len());
        Ok(())
    });
    let closed = client.peer_closed();
    let result = result.and_then(|()| client.receive_eof());
    (
        result,
        client.transmit().to_vec(),
        client.plaintext().to_vec(),
        closed,
    )
}

fuzz_target!(|input: &[u8]| {
    let Some((&mode, data)) = input.split_first() else {
        return;
    };
    let data = &data[..data.len().min(4096)];
    let wire = match mode % 3 {
        0 => data.to_vec(),
        1 => handshake(HandshakeType::ServerHello, data),
        _ => {
            let hello = ServerHello {
                server_version: ProtocolVersion::TLS_1_2,
                random: [0x55; 32],
                session_id: Vec::new(),
                cipher_suite: CipherSuite::EcdheRsaWithAes128GcmSha256,
                compression_method: 0,
                extensions: Some(vec![Extension::extended_master_secret()]),
            };
            let mut wire = handshake(HandshakeType::ServerHello, &hello.to_bytes().unwrap());
            wire.extend(handshake(HandshakeType::Certificate, data));
            wire
        }
    };
    assert_eq!(
        receive(&wire, 1 + usize::from(mode >> 3)),
        receive(&wire, wire.len().max(1))
    );
});
