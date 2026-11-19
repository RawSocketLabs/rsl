//! Answers a gateway might send that are wrong, odd or hostile, and session values that
//! must not escape into the request.

use sangfor_atrust::api::{
    AntiMitmData, AuthConfig, Call, Context, Envelope, Error, Gateway, Manifest, NextStep,
    OnlineInfo, PswReply, Reply,
};

use crate::support::ok;

fn context<'a>(gateway: &'a Gateway, csrf_token: &'a str, trace_id: &'a str) -> Context<'a> {
    Context {
        gateway,
        csrf_token,
        trace_id,
        sign_key: None,
        cookie: None,
    }
}

/// An answer with code zero and `data` as its payload.
fn answer(data: &str) -> http1::Response {
    ok(&format!(r#"{{"code":0,"message":null,"data":{data}}}"#))
}

#[test]
fn a_session_value_cannot_inject_a_header_or_a_request() {
    let gateway = Gateway::new("sdp.example.org", 443);
    // The CSRF token comes from the gateway's last answer.
    for hostile in ["a\r\nX-Injected: 1", "a\nb", "a\rb", "a\0b"] {
        let found = Call::auth_check().request(&context(&gateway, hostile, "0a1b2c3d"));
        assert!(matches!(found, Err(Error::Request(_))), "csrf {hostile:?}");
        let found = Call::auth_check().request(&context(&gateway, "t", hostile));
        assert!(
            matches!(found, Err(Error::Request(_))),
            "trace id {hostile:?}"
        );
    }
    // A cookie restored from storage is not one a response header was checked for.
    for hostile in [
        &b"sid=x\r\nX-Injected: 1\r\n\r\nGET /evil HTTP/1.1"[..],
        b"a=b\nc",
        b"a=b\0c",
    ] {
        let with_cookie = Context {
            cookie: Some(hostile),
            ..context(&gateway, "t", "0a1b2c3d")
        };
        let found = Call::auth_check().request(&with_cookie);
        assert!(
            matches!(found, Err(Error::Request(_))),
            "cookie {hostile:?}"
        );
        // A bare call carries no cookie at all.
        let bare = Call::manifest().request(&with_cookie).unwrap();
        assert_eq!(bare.headers.get("Cookie"), None);
    }
    // A host that could end the Host field and start another.
    for hostile in ["sdp.example.org\r\nX: 1", "sdp.example.org\n"] {
        let gateway = Gateway::new(hostile, 443);
        let found = Call::manifest().request(&context(&gateway, "", ""));
        assert!(matches!(found, Err(Error::Request(_))), "host {hostile:?}");
    }
}

#[test]
fn a_query_value_is_encoded_not_spliced() {
    let gateway = Gateway::new("sdp.example.org", 443);
    let call = Call::anti_mitm("N", "T", "id&x=1 #\r\n");
    let request = call.request(&context(&gateway, "", "t")).unwrap();
    let target = String::from_utf8(request.target).unwrap();
    assert!(
        target.ends_with("mobileId=id%26x%3D1+%23%0D%0A&platform=Linux"),
        "{target}"
    );
}

#[test]
fn a_signed_request_is_refused_without_a_key() {
    let gateway = Gateway::new("sdp.example.org", 443);
    let call = Call::report_env("d", &[], "T");
    assert!(matches!(
        call.request(&context(&gateway, "invalid\nheader", "t")),
        Err(Error::NoSignKey)
    ));
}

#[test]
fn reply_diagnostics_hide_authentication_material() {
    let replies = ["aaa", "bbb"].map(|secret| {
        let mut response = ok(secret);
        response.head.headers.push("Set-Cookie", secret);
        Reply {
            response,
            peer_chain: vec![secret.as_bytes().to_vec()],
        }
    });
    assert_eq!(format!("{:?}", replies[0]), format!("{:?}", replies[1]));

    let passwords = ["aaa", "bbb"].map(|secret| {
        let mut reply = PswReply::default();
        reply.message = secret.into();
        reply.ticket = secret.into();
        reply.next_service = secret.into();
        reply.anti_replay_rand = secret.into();
        reply
    });
    assert_eq!(format!("{:?}", passwords[0]), format!("{:?}", passwords[1]));
}

#[test]
fn bodies_that_are_not_the_envelope_are_errors() {
    let bodies: [&[u8]; 10] = [
        b"",
        b"<html>",
        b"[]",
        br#"[10000004,"m",{}]"#,
        b"7",
        b"null",
        br#"{"code":"0"}"#,
        br#"{"code":0.5}"#,
        br#"{"message":7}"#,
        b"{\"message\":\"\xff\"}",
    ];
    for body in bodies {
        assert!(
            matches!(Envelope::parse(body), Err(Error::Json(_))),
            "{body:?}"
        );
    }
    assert!(matches!(
        Envelope::of_ok(&ok("<html>")),
        Err(Error::Json(_))
    ));
}

#[test]
fn an_error_answer_keeps_its_code_whatever_its_data_is() {
    for data in ["null", "7", "[]", r#""x""#, r#"{"ticket":7}"#] {
        let body = format!(r#"{{"code":75500000,"message":"no","data":{data}}}"#);
        let envelope = Envelope::parse(body.as_bytes()).unwrap();
        assert!(
            matches!(
                envelope.accepted(),
                Err(Error::Refused {
                    code: 75_500_000,
                    ..
                })
            ),
            "{data}"
        );
        // The password answer is read on a refusal too, and must survive any payload.
        let reply = PswReply::parse(&ok(&body)).unwrap();
        assert_eq!(
            (reply.code, reply.wants_captcha),
            (75_500_000, false),
            "{data}"
        );
        assert!(
            matches!(NextStep::parse(&ok(&body)), Err(Error::Refused { .. })),
            "{data}"
        );
        assert!(
            matches!(AuthConfig::parse(&ok(&body)), Err(Error::Refused { .. })),
            "{data}"
        );
    }
}

#[test]
fn null_members_read_as_absent() {
    let nulls = answer(
        r#"{"csrfToken":null,"security":null,"pubKey":null,"pubKeyExp":null,"antiReplayRand":null,
            "isLogin":null,"authServerInfoList":null,"antiMITMAttackData":null}"#,
    );
    assert_eq!(AuthConfig::parse(&nulls).unwrap(), AuthConfig::default());

    let nulls = answer(r#"{"ticket":null,"graphCheckCodeEnable":null,"nextService":null}"#);
    assert_eq!(PswReply::parse(&nulls).unwrap(), PswReply::default());

    let nulls =
        answer(r#"{"nextService":null,"nextServiceList":[{"authId":null}],"username":null}"#);
    assert_eq!(NextStep::parse(&nulls).unwrap().next_service_list.len(), 1);
    assert_eq!(OnlineInfo::parse(&nulls).unwrap().username, "");
    assert_eq!(
        NextStep::parse(&answer(r#"{"nextServiceList":null}"#)).unwrap(),
        NextStep::default()
    );

    let nulls = answer(r#"{"capacities":null,"options":{"tun0rtt":null}}"#);
    assert!(!Manifest::parse(&nulls).unwrap().zero_rtt);
}

#[test]
fn members_of_the_wrong_type_are_errors() {
    let auth_config = [
        r#"{"pubKey":7}"#,
        r#"{"isLogin":"1"}"#,
        r#"{"authServerInfoList":{}}"#,
        r#"{"authServerInfoList":[7]}"#,
        r#"{"antiMITMAttackData":7}"#,
        r#"{"antiMITMAttackData":[]}"#,
        r#"{"antiMITMAttackData":{"enable":"1"}}"#,
        "7",
        "[]",
        // An array where an object belongs is not that object written by position.
        r#"{"security":["tok"]}"#,
        r#"{"authServerInfoList":[["local","1","auth/psw"]]}"#,
        r#"{"authServerInfoList":[[]]}"#,
    ];
    for mistyped in auth_config {
        assert!(
            matches!(AuthConfig::parse(&answer(mistyped)), Err(Error::Json(_))),
            "{mistyped}"
        );
    }
    for mistyped in [
        r#"{"nextServiceList":[["a-1","auth/sms","s"]]}"#,
        r#"["auth/sms"]"#,
    ] {
        assert!(
            matches!(NextStep::parse(&answer(mistyped)), Err(Error::Json(_))),
            "{mistyped}"
        );
    }
    for mistyped in [
        r#"{"capacities":[["2.0"]],"options":[[true]]}"#,
        r#"{"capacities":{"zeroRTT":["2.0"]}}"#,
    ] {
        assert!(
            matches!(Manifest::parse(&answer(mistyped)), Err(Error::Json(_))),
            "{mistyped}"
        );
    }
    assert!(matches!(
        PswReply::parse(&answer(r#"["T",1]"#)),
        Err(Error::Json(_))
    ));
    assert!(matches!(
        OnlineInfo::parse(&answer(r#"["user01"]"#)),
        Err(Error::Json(_))
    ));
}

#[test]
fn a_list_longer_than_any_real_one_is_refused() {
    // Many empty entries would otherwise cost far more memory than their text.
    let entries = |count: usize| vec!["{}"; count].join(",");
    let servers = |count| answer(&format!(r#"{{"authServerInfoList":[{}]}}"#, entries(count)));
    assert_eq!(AuthConfig::parse(&servers(256)).unwrap().servers.len(), 256);
    assert!(matches!(
        AuthConfig::parse(&servers(257)),
        Err(Error::Json(_))
    ));

    let steps = |count| answer(&format!(r#"{{"nextServiceList":[{}]}}"#, entries(count)));
    assert_eq!(
        NextStep::parse(&steps(256))
            .unwrap()
            .next_service_list
            .len(),
        256
    );
    assert!(matches!(NextStep::parse(&steps(257)), Err(Error::Json(_))));
}

#[test]
fn the_anti_mitm_text_is_kept_with_its_number_lexemes() {
    // §2.5: `1.0` and `1e0` must reach the signature check as written, not as `1`.
    let raw = r#"{"enable":1,"extra":1.0,"more":1e0,"nested":{"a":[1,2]},"mitmSig":"x"}"#;
    let data = AntiMitmData::parse(raw).unwrap().unwrap();
    assert_eq!(data.raw, raw);
    assert_eq!(data.enable, 1);

    // An object whose only member holds an empty object is not an empty object.
    assert!(AntiMitmData::parse(r#"{"a":{}}"#).unwrap().is_some());
    assert!(matches!(AntiMitmData::parse("[]"), Err(Error::Json(_))));
    assert!(matches!(AntiMitmData::parse("{"), Err(Error::Json(_))));
}
