//! RFC-derived, byte-exact SOCKS5 wire contracts.

use socks::v5::{
    AuthMethod, Command, Domain, Endpoint, MethodRequest, MethodSelection, Reply, ReplyCode,
    Request, UsernamePasswordRequest, UsernamePasswordResponse, UsernamePasswordStatus,
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
    assert_eq!(
        request.destination,
        Endpoint::domain(b"example.com", 80).unwrap()
    );
    assert_eq!(request.to_bytes().unwrap(), wire);
}

#[test]
fn checked_domain_constructor_preserves_bytes_and_allows_zero_port() {
    for name in [vec![b'x'], vec![b'x'; 255], vec![b'A', 0, 0xff]] {
        let checked = Endpoint::domain(name.clone(), 0).unwrap();
        assert_eq!(checked, Endpoint::domain_raw(name.clone(), 0));
        let mut wire = vec![3, u8::try_from(name.len()).unwrap()];
        wire.extend_from_slice(&name);
        wire.extend_from_slice(&[0, 0]);
        assert_eq!(checked.to_bytes().unwrap(), wire);
        assert_eq!(Endpoint::decode_exact(&wire).unwrap(), checked);
    }
}

#[test]
fn zero_ports_remain_representable_in_wire_requests_and_replies() {
    for address in [
        vec![1, 127, 0, 0, 1, 0, 0],
        vec![3, 1, b'x', 0, 0],
        vec![4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0],
    ] {
        let endpoint = Endpoint::decode_exact(&address).unwrap();
        assert!(endpoint.validate().is_ok());
        let request = Request::builder()
            .command(Command::Connect)
            .destination(endpoint.clone())
            .build()
            .unwrap();
        let wire = [vec![5, 1, 0], address.clone()].concat();
        assert_eq!(request.to_bytes().unwrap(), wire);
        assert_eq!(request.to_canonical_bytes().unwrap(), wire);
        assert_eq!(Request::decode_exact(&wire).unwrap(), request);
        for code in [ReplyCode::Succeeded, ReplyCode::GeneralFailure] {
            let reply = Reply::builder()
                .code(code)
                .bound(endpoint.clone())
                .build()
                .unwrap();
            let wire = [vec![5, u8::from(code), 0], address.clone()].concat();
            assert_eq!(reply.to_bytes().unwrap(), wire);
            assert_eq!(reply.to_canonical_bytes().unwrap(), wire);
            assert_eq!(Reply::decode_exact(&wire).unwrap(), reply);
        }
    }
}

#[test]
fn domain_payload_keeps_opaque_bytes_and_endpoint_adds_only_atyp() {
    let domain = Domain::builder()
        .name(vec![b'A', 0, 0xff])
        .port(0x1234)
        .build()
        .unwrap();
    // RFC 1928 address framing, with deliberately opaque/non-DNS name bytes.
    let payload = [3, b'A', 0, 0xff, 0x12, 0x34];
    assert_eq!(domain.to_bytes().unwrap(), payload);
    assert_eq!(Domain::decode_exact(&payload).unwrap(), domain);
    let endpoint = Endpoint::from(domain);
    let wire = [3, 3, b'A', 0, 0xff, 0x12, 0x34];
    assert_eq!(endpoint.to_bytes().unwrap(), wire);
    assert_eq!(Endpoint::decode_exact(&wire).unwrap(), endpoint);
    assert!(endpoint.validate().is_ok());
}

#[test]
fn domain_builders_accept_both_length_boundaries_without_restricting_command_or_reply() {
    for length in [1u8, 255] {
        let name = vec![b'x'; usize::from(length)];
        let domain = Domain::builder()
            .name(name.clone())
            .port(443)
            .build()
            .unwrap();
        let payload = [vec![length], name, vec![1, 187]].concat();
        assert_eq!(domain.to_bytes().unwrap(), payload, "length {length}");

        // Construction checks address length, not the guided CONNECT-only capability.
        let request = Request::builder()
            .command(Command::Bind)
            .destination(domain.clone().into())
            .build()
            .unwrap();
        let request_wire = [vec![5, 2, 0, 3], payload.clone()].concat();
        assert_eq!(request.to_bytes().unwrap(), request_wire, "length {length}");
        assert_eq!(Request::decode_exact(&request_wire).unwrap(), request);

        let reply = Reply::builder()
            .code(ReplyCode::ConnectionRefused)
            .bound(domain.into())
            .build()
            .unwrap();
        let reply_wire = [vec![5, 5, 0, 3], payload].concat();
        assert_eq!(reply.to_bytes().unwrap(), reply_wire, "length {length}");
        assert_eq!(Reply::decode_exact(&reply_wire).unwrap(), reply);
    }
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
