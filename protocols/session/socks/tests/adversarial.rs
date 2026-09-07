//! Rejection, permissive-decoding, length-boundary, and no-panic evidence.

use bnb::{BuilderError, bitstream::ErrorKind};
use socks::v5::{
    AuthMethod, Command, Endpoint, MethodRequest, MethodSelection, Reply, ReplyCode, Request,
    UsernamePasswordRequest, UsernamePasswordResponse,
};
use std::net::Ipv4Addr;

#[test]
fn every_truncated_request_is_rejected() {
    let wire = [0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x01, 0xbb];
    for end in 0..wire.len() {
        assert!(
            Request::decode_exact(&wire[..end]).is_err(),
            "accepted request truncated at byte {end}"
        );
    }
}

#[test]
fn every_truncated_negotiation_message_is_rejected() {
    let request = [0x05, 0x02, 0x00, 0x02];
    for end in 0..request.len() {
        assert!(
            MethodRequest::decode_exact(&request[..end]).is_err(),
            "accepted method request truncated at byte {end}"
        );
    }

    let selection = [0x05, 0x00];
    for end in 0..selection.len() {
        assert!(
            MethodSelection::decode_exact(&selection[..end]).is_err(),
            "accepted method selection truncated at byte {end}"
        );
    }
}

#[test]
fn every_truncated_username_password_message_is_rejected() {
    let mut request = vec![0x01, 0x04];
    request.extend_from_slice(b"user");
    request.push(0x07);
    request.extend_from_slice(b"hunter2");
    for end in 0..request.len() {
        assert!(
            UsernamePasswordRequest::decode_exact(&request[..end]).is_err(),
            "accepted username/password request truncated at byte {end}"
        );
    }

    let response = [0x01, 0x00];
    for end in 0..response.len() {
        assert!(
            UsernamePasswordResponse::decode_exact(&response[..end]).is_err(),
            "accepted username/password response truncated at byte {end}"
        );
    }
}

#[test]
fn every_truncated_reply_is_rejected() {
    let wire = [0x05, 0x00, 0x00, 0x01, 127, 0, 0, 1, 0x01, 0xbb];
    for end in 0..wire.len() {
        assert!(
            Reply::decode_exact(&wire[..end]).is_err(),
            "accepted reply truncated at byte {end}"
        );
    }
}

#[test]
fn unsupported_address_type_is_rejected_without_guessing_its_width() {
    let error = Request::decode_exact(&[0x05, 0x01, 0x00, 0x02, 0x00, 0x50]).unwrap_err();
    assert!(
        matches!(&error.kind, ErrorKind::Convert { message } if message.contains("unrecognized Endpoint discriminant")),
        "unexpected error: {error:?}"
    );
}

#[test]
fn decoded_noncanonical_constants_round_trip_verbatim_and_normalize_explicitly() {
    let wire = [0x04, 0x01, 0xff, 0x01, 127, 0, 0, 1, 0x00, 0x50];
    let request = Request::decode_exact(&wire).unwrap();
    assert_eq!(request.version, 4);
    assert_eq!(request.reserved, 0xff);
    assert_eq!(request.to_bytes().unwrap(), wire);
    assert_eq!(
        request.to_canonical_bytes().unwrap(),
        [0x05, 0x01, 0x00, 0x01, 127, 0, 0, 1, 0x00, 0x50]
    );
}

#[test]
fn decoded_noncanonical_auth_versions_round_trip_verbatim_and_normalize_explicitly() {
    let request_wire = [0xff, 0x01, b'u', 0x01, b'p'];
    let request = UsernamePasswordRequest::decode_exact(&request_wire).unwrap();
    assert_eq!(request.version, 0xff);
    assert_eq!(request.to_bytes().unwrap(), request_wire);
    assert_eq!(
        request.to_canonical_bytes().unwrap(),
        [0x01, 0x01, b'u', 0x01, b'p']
    );

    let response_wire = [0xff, 0x7f];
    let response = UsernamePasswordResponse::decode_exact(&response_wire).unwrap();
    assert_eq!(response.version, 0xff);
    assert_eq!(response.status.code(), 0x7f);
    assert!(response.status.is_failure());
    assert_eq!(response.to_bytes().unwrap(), response_wire);
    assert_eq!(response.to_canonical_bytes().unwrap(), [0x01, 0x7f]);
}

#[test]
fn unassigned_command_and_reply_codes_are_preserved() {
    let request_wire = [0x05, 0x09, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
    let request = Request::decode_exact(&request_wire).unwrap();
    assert_eq!(request.command, Command::Other(0x09));
    assert_eq!(request.to_bytes().unwrap(), request_wire);

    let reply_wire = [0x05, 0x09, 0x00, 0x01, 0, 0, 0, 0, 0, 0];
    let reply = Reply::decode_exact(&reply_wire).unwrap();
    assert_eq!(reply.code, ReplyCode::Other(0x09));
    assert_eq!(reply.to_bytes().unwrap(), reply_wire);
}

#[test]
fn empty_method_list_decodes_but_compliant_builder_rejects_it() {
    let decoded = MethodRequest::decode_exact(&[0x05, 0x00]).unwrap();
    assert!(decoded.methods.is_empty());

    let error = MethodRequest::builder()
        .methods(Vec::new())
        .build()
        .unwrap_err();
    assert!(matches!(error, BuilderError::Invalid(_)));
}

#[test]
fn empty_username_decodes_but_compliant_builder_rejects_it() {
    let decoded = UsernamePasswordRequest::decode_exact(&[0x01, 0x00, 0x01, b'p']).unwrap();
    assert!(decoded.username.is_empty());
    assert_eq!(decoded.password, b"p");

    let result = UsernamePasswordRequest::builder()
        .username(Vec::new())
        .password(b"p".to_vec())
        .build();
    assert!(matches!(result, Err(BuilderError::Invalid(_))));
}

#[test]
fn empty_password_decodes_but_compliant_builder_rejects_it() {
    let decoded = UsernamePasswordRequest::decode_exact(&[0x01, 0x01, b'u', 0x00]).unwrap();
    assert_eq!(decoded.username, b"u");
    assert!(decoded.password.is_empty());

    let result = UsernamePasswordRequest::builder()
        .username(b"u".to_vec())
        .password(Vec::new())
        .build();
    assert!(matches!(result, Err(BuilderError::Invalid(_))));
}

#[test]
fn no_acceptable_methods_decodes_but_compliant_builder_rejects_it() {
    let decoded = MethodRequest::decode_exact(&[0x05, 0x01, 0xff]).unwrap();
    assert_eq!(decoded.methods, [AuthMethod::NoAcceptable]);

    let error = MethodRequest::builder()
        .methods(vec![AuthMethod::NoAcceptable])
        .build()
        .unwrap_err();
    assert!(matches!(error, BuilderError::Invalid(_)));

    let disguised = MethodRequest::builder()
        .methods(vec![AuthMethod::Other(0xff)])
        .build()
        .unwrap_err();
    assert!(matches!(disguised, BuilderError::Invalid(_)));

    let mut raw = decoded;
    raw.methods[0] = AuthMethod::Other(0xff);
    assert!(raw.validate().is_err());
    assert_eq!(raw.methods, [AuthMethod::Other(0xff)]);
    assert_eq!(raw.to_bytes().unwrap(), [5, 1, 0xff]);
}

#[test]
fn method_validation_accepts_selectable_aliases_without_mutating_them() {
    let methods = vec![
        AuthMethod::Other(0),
        AuthMethod::Other(2),
        AuthMethod::Other(0x80),
    ];
    let request = MethodRequest {
        version: 5,
        methods: methods.clone(),
    };

    assert!(request.validate().is_ok());
    assert_eq!(request.methods, methods);
    assert_eq!(request.to_bytes().unwrap(), [5, 3, 0, 2, 0x80]);
}

#[test]
fn method_count_overflow_is_rejected() {
    let request = MethodRequest {
        version: 5,
        methods: vec![AuthMethod::NoAuthentication; 256],
    };
    let error = request.to_bytes().unwrap_err();
    assert!(
        matches!(&error.kind, ErrorKind::Convert { message } if message.contains("256")),
        "unexpected error: {error:?}"
    );
}

#[test]
fn domain_length_overflow_is_rejected() {
    let request = Request {
        version: 5,
        command: Command::Connect,
        reserved: 0,
        destination: Endpoint::domain(vec![b'x'; 256], 443),
    };
    let error = request.to_bytes().unwrap_err();
    assert!(
        matches!(&error.kind, ErrorKind::Convert { message } if message.contains("256")),
        "unexpected error: {error:?}"
    );
}

#[test]
fn username_length_overflow_is_rejected() {
    let request = UsernamePasswordRequest {
        version: 1,
        username: vec![b'u'; 256],
        password: vec![b'p'],
    };
    let error = request.to_bytes().unwrap_err();
    assert!(
        matches!(&error.kind, ErrorKind::Convert { message } if message.contains("256")),
        "unexpected error: {error:?}"
    );

    let result = UsernamePasswordRequest::builder()
        .username(vec![b'u'; 256])
        .password(vec![b'p'])
        .build();
    assert!(matches!(result, Err(BuilderError::Invalid(_))));
}

#[test]
fn password_length_overflow_is_rejected() {
    let request = UsernamePasswordRequest {
        version: 1,
        username: vec![b'u'],
        password: vec![b'p'; 256],
    };
    let error = request.to_bytes().unwrap_err();
    assert!(
        matches!(&error.kind, ErrorKind::Convert { message } if message.contains("256")),
        "unexpected error: {error:?}"
    );

    let result = UsernamePasswordRequest::builder()
        .username(vec![b'u'])
        .password(vec![b'p'; 256])
        .build();
    assert!(matches!(result, Err(BuilderError::Invalid(_))));
}

#[test]
fn maximum_domain_length_round_trips() {
    let request = Request {
        version: 5,
        command: Command::Connect,
        reserved: 0,
        destination: Endpoint::domain(vec![b'x'; 255], u16::MAX),
    };
    let wire = request.to_bytes().unwrap();
    assert_eq!(wire[4], u8::MAX);
    assert_eq!(Request::decode_exact(&wire).unwrap(), request);
}

#[test]
fn maximum_username_and_password_lengths_round_trip() {
    let request = UsernamePasswordRequest::builder()
        .username(vec![b'u'; 255])
        .password(vec![b'p'; 255])
        .build()
        .unwrap();
    let wire = request.to_bytes().unwrap();
    assert_eq!(wire[1], u8::MAX);
    assert_eq!(wire[257], u8::MAX);

    let decoded = UsernamePasswordRequest::decode_exact(&wire).unwrap();
    assert_eq!(decoded.username, vec![b'u'; 255]);
    assert_eq!(decoded.password, vec![b'p'; 255]);
}

#[test]
fn every_username_password_status_value_round_trips() {
    for status in 0u8..=u8::MAX {
        let wire = [0x01, status];
        let response = UsernamePasswordResponse::decode_exact(&wire).unwrap();
        assert_eq!(response.to_bytes().unwrap(), wire, "status {status:#04x}");
    }
}

#[test]
fn bounded_uniform_byte_inputs_do_not_panic() {
    for byte in 0u8..=u8::MAX {
        let bytes = [byte; 24];
        let _ = MethodRequest::decode_exact(&bytes);
        let _ = Request::decode_exact(&bytes);
        let _ = Reply::decode_exact(&bytes);
        let _ = UsernamePasswordRequest::decode_exact(&bytes);
        let _ = UsernamePasswordResponse::decode_exact(&bytes);
    }

    let valid = Request {
        version: 5,
        command: Command::Connect,
        reserved: 0,
        destination: Endpoint::Ipv4 {
            address: Ipv4Addr::LOCALHOST,
            port: 80,
        },
    };
    assert!(valid.to_bytes().is_ok());
}
