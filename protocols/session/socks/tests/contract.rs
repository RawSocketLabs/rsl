//! RFC-derived, byte-exact SOCKS5 wire contracts.

use socks::v5::{
    AuthMethod, Command, Endpoint, MethodRequest, MethodSelection, Reply, ReplyCode, Request,
    UsernamePasswordRequest, UsernamePasswordResponse, UsernamePasswordStatus,
};
use std::net::{Ipv4Addr, Ipv6Addr};

#[test]
fn method_builders_normalize_known_aliases_without_changing_wire_codes() {
    let request = MethodRequest::builder()
        .methods(vec![
            AuthMethod::Other(0),
            AuthMethod::Other(2),
            AuthMethod::Other(0x80),
        ])
        .build()
        .unwrap();
    assert_eq!(
        request.methods,
        [
            AuthMethod::NoAuthentication,
            AuthMethod::UsernamePassword,
            AuthMethod::Other(0x80)
        ]
    );
    assert_eq!(request.to_bytes().unwrap(), [5, 3, 0, 2, 0x80]);
    let selection = MethodSelection::builder()
        .method(AuthMethod::Other(0xff))
        .build()
        .unwrap();
    assert_eq!(selection.method, AuthMethod::NoAcceptable);
    assert_eq!(selection.to_bytes().unwrap(), [5, 0xff]);
}

#[test]
fn method_negotiation_matches_rfc_1928_section_3() {
    let request_wire = [0x05, 0x02, 0x00, 0x02];
    let request = MethodRequest::decode_exact(&request_wire).unwrap();
    assert_eq!(
        request.methods,
        [AuthMethod::NoAuthentication, AuthMethod::UsernamePassword]
    );
    assert_eq!(request.to_bytes().unwrap(), request_wire);

    let selection_wire = [0x05, 0x00];
    let selection = MethodSelection::decode_exact(&selection_wire).unwrap();
    assert_eq!(selection.method, AuthMethod::NoAuthentication);
    assert_eq!(selection.to_bytes().unwrap(), selection_wire);
}

#[test]
fn username_password_request_matches_rfc_1929_section_2() {
    let mut wire = vec![0x01, 0x04];
    wire.extend_from_slice(b"user");
    wire.push(0x07);
    wire.extend_from_slice(b"hunter2");

    let request = UsernamePasswordRequest::decode_exact(&wire).unwrap();
    assert_eq!(request.version, 1);
    assert_eq!(request.username, b"user");
    assert_eq!(request.password, b"hunter2");
    assert_eq!(request.to_bytes().unwrap(), wire);
}

#[test]
fn username_password_responses_match_rfc_1929_section_2() {
    let success_wire = [0x01, 0x00];
    let success = UsernamePasswordResponse::decode_exact(&success_wire).unwrap();
    assert_eq!(success.status, UsernamePasswordStatus::SUCCESS);
    assert_eq!(success.to_bytes().unwrap(), success_wire);

    let failure_wire = [0x01, 0x01];
    let failure = UsernamePasswordResponse::decode_exact(&failure_wire).unwrap();
    assert_eq!(failure.status.code(), 0x01);
    assert!(failure.status.is_failure());
    assert_eq!(failure.to_bytes().unwrap(), failure_wire);

    let built_failure = UsernamePasswordResponse::builder()
        .status(UsernamePasswordStatus::from(0x01))
        .build()
        .unwrap();
    assert_eq!(built_failure.to_bytes().unwrap(), failure_wire);
}

#[test]
fn connect_request_encodes_ipv4_endpoint() {
    let wire = [0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x01, 0xbb];
    let request = Request::decode_exact(&wire).unwrap();
    assert_eq!(request.command, Command::Connect);
    assert_eq!(
        request.destination,
        Endpoint::Ipv4 {
            address: Ipv4Addr::LOCALHOST,
            port: 443,
        }
    );
    assert_eq!(request.to_bytes().unwrap(), wire);
}

#[test]
fn connect_request_encodes_domain_endpoint() {
    let mut wire = vec![0x05, 0x01, 0x00, 0x03, 11];
    wire.extend_from_slice(b"example.com");
    wire.extend_from_slice(&80u16.to_be_bytes());

    let request = Request::decode_exact(&wire).unwrap();
    assert_eq!(request.destination, Endpoint::domain(b"example.com", 80));
    assert_eq!(request.to_bytes().unwrap(), wire);
}

#[test]
fn connect_request_encodes_ipv6_endpoint() {
    let mut wire = vec![0x05, 0x01, 0x00, 0x04];
    wire.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());
    wire.extend_from_slice(&443u16.to_be_bytes());

    let request = Request::decode_exact(&wire).unwrap();
    assert_eq!(
        request.destination,
        Endpoint::Ipv6 {
            address: Ipv6Addr::LOCALHOST,
            port: 443,
        }
    );
    assert_eq!(request.to_bytes().unwrap(), wire);
}

#[test]
fn successful_reply_matches_rfc_1928_section_6() {
    let wire = [0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0x04, 0x38];
    let reply = Reply::decode_exact(&wire).unwrap();
    assert_eq!(reply.code, ReplyCode::Succeeded);
    assert_eq!(reply.bound.port(), 1080);
    assert_eq!(reply.to_bytes().unwrap(), wire);
}

#[test]
fn builders_supply_protocol_constants() {
    let request = Request::builder()
        .command(Command::Connect)
        .destination(Endpoint::Ipv4 {
            address: Ipv4Addr::UNSPECIFIED,
            port: 0,
        })
        .build()
        .unwrap();
    assert_eq!(request.version, 5);
    assert_eq!(request.reserved, 0);

    let selection = MethodSelection::builder()
        .method(AuthMethod::NoAuthentication)
        .build()
        .unwrap();
    assert_eq!(selection.version, 5);

    let credentials = UsernamePasswordRequest::builder()
        .username(b"user".to_vec())
        .password(b"password".to_vec())
        .build()
        .unwrap();
    assert_eq!(credentials.version, 1);

    let auth_response = UsernamePasswordResponse::builder()
        .status(UsernamePasswordStatus::SUCCESS)
        .build()
        .unwrap();
    assert_eq!(auth_response.version, 1);
}
