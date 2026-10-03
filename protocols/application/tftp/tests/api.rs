//! Public-API surface: the documented types are reachable and keep their shape,
//! independent of any network I/O.

use std::time::Duration;

use tftp::client::{Client, RawClient};
use tftp::error::TftpError;
use tftp::server::{Handler, HandlerResult, MemoryStore, Reject, Server};
use tftp::wire::{Ack, Data, ErrorCode, ErrorPacket, Opcode, Packet, Request, TransferMode};
use tftp::{BLOCK_SIZE, DEFAULT_MAX_RETRIES, DEFAULT_TIMEOUT, MAX_BLOCK_SIZE};

#[test]
fn client_builder_chain_is_available() {
    let server = "127.0.0.1:69".parse().unwrap();
    let _client = Client::new(server)
        .mode(TransferMode::NetAscii)
        .with_timeout(Duration::from_secs(2))
        .with_max_retries(3);
    let _raw = RawClient::bind(server);
}

#[cfg(feature = "options")]
#[test]
fn option_setters_are_available() {
    let server = "127.0.0.1:69".parse().unwrap();
    let _client = Client::new(server)
        .block_size(1432)
        .timeout_option(5)
        .request_tsize(true);
}

#[test]
fn server_and_handler_surface_is_reachable() {
    use std::sync::Arc;

    let store = Arc::new(MemoryStore::new().with_file("a", b"x".to_vec()));
    let server = Server::bind("127.0.0.1:0", store)
        .unwrap()
        .with_max_connections(8);
    assert!(server.local_addr().is_ok());

    // A bespoke handler implementing the trait compiles and the helpers exist.
    struct H;
    impl Handler for H {
        fn read(&self, _f: &str) -> HandlerResult<Vec<u8>> {
            Err(Reject::not_found())
        }
        fn write(&self, _f: &str, _c: Vec<u8>) -> HandlerResult<()> {
            Ok(())
        }
    }
    let _h = H;
    let _r = Reject::new(ErrorCode::DiskFull, "full");
}

#[test]
fn wire_types_encode_and_decode() {
    assert_eq!(
        Packet::decode(b"\x00\x01f\0octet\0").unwrap().opcode(),
        Opcode::Read
    );
    let packets = [
        Packet::Request(Request::read("f", TransferMode::Octet)),
        Packet::Data(Data::new(1, b"x".to_vec())),
        Packet::Ack(Ack::new(1)),
        Packet::Error(ErrorPacket::new(ErrorCode::NotDefined, "m")),
    ];
    for p in packets {
        assert_eq!(Packet::decode(&p.encode()).unwrap(), p);
    }
}

#[test]
fn constants_and_error_type_are_public() {
    assert_eq!(BLOCK_SIZE, 512);
    assert_eq!(MAX_BLOCK_SIZE, 65464);
    assert_eq!(DEFAULT_TIMEOUT, std::time::Duration::from_secs(3));
    assert_eq!(DEFAULT_MAX_RETRIES, 5);
    let err = TftpError::Validation("x".into());
    assert!(err.to_string().contains("validation"));
}
