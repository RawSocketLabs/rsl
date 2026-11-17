//! The control-plane API (spec §4): requests byte for byte, answers field for field.

use std::fmt::Write as _;

use crypto::digest::sha2::sha256::Sha256;
use http1::Request;
use sangfor_atrust::api::{
    AntiMitmReply, AuthConfig, Call, Context, Envelope, Error, Gateway, Manifest, NextStep,
    OnlineInfo, PswReply,
};
use sangfor_atrust_sig::SignKey;

use crate::support::{ok, response};

/// The shared device id of the specification's vectors.
const DEVICE_ID: &str = "00112233445566778899aabbccddeeff";
/// The common query string, sorted (§2.4).
const COMMON: &str = "clientType=SDPClient&lang=en-US&platform=Linux";
/// The `User-Agent` of §4.2, whole.
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) aTrustTray/2.4.10.50 Chrome/83.0.4103.94 Electron/9.0.2 Safari/537.36 aTrustTray-Linux-Plat-Ubuntu-x64 SPCClientType";

/// The key K0 of the specification's vectors.
fn k0() -> SignKey {
    let hex = "6DCC2BEE653BAAB458A7CEE13821BD4A3D0B379603BD84AEF5E8070EAC5A3FD7";
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[2 * index..2 * index + 2], 16).unwrap();
    }
    SignKey::from_bytes(bytes)
}

/// Build `call` for a gateway on port 8443 with a CSRF token, a trace id and K0.
fn request(call: &Call) -> Request {
    let gateway = Gateway::new("sdp.example.org", 8443);
    let key = k0();
    call.request(&Context {
        gateway: &gateway,
        csrf_token: "csrf-1",
        trace_id: "0a1b2c3d",
        sign_key: Some(&key),
        cookie: None,
    })
    .unwrap()
}

fn header<'a>(request: &'a Request, name: &str) -> Option<&'a str> {
    request
        .headers
        .get(name)
        .map(|value| std::str::from_utf8(value).unwrap())
}

fn target(request: &Request) -> &str {
    std::str::from_utf8(&request.target).unwrap()
}

/// The names of the headers a request carries beyond those every request may have.
fn particular_headers(request: &Request) -> Vec<String> {
    let common = ["host", "user-agent", "cookie", "content-length"];
    let mut names: Vec<String> = request
        .headers
        .fields
        .iter()
        .map(|field| {
            String::from_utf8(field.name.clone())
                .unwrap()
                .to_ascii_lowercase()
        })
        .filter(|name| !common.contains(&name.as_str()))
        .collect();
    names.sort();
    names
}

// §4.1, §2.4: the query of each request, sorted, the endpoint's parameters merged in.
#[test]
fn each_request_has_its_method_path_and_sorted_query() {
    let rows = [
        (Call::manifest(), "GET", "/public/manifest".to_owned()),
        (
            Call::auth_config(),
            "GET",
            "/passport/v1/public/authConfig?clientType=SDPClient&lang=en-US&needTicket=1&platform=Linux".to_owned(),
        ),
        (
            Call::anti_mitm("N", "T", DEVICE_ID),
            "POST",
            format!("/controller/v1/public/antiMITMRequest?clientType=SDPClient&lang=en-US&mobileId={DEVICE_ID}&platform=Linux"),
        ),
        (
            Call::anti_mitm("N", "T", ""),
            "POST",
            format!("/controller/v1/public/antiMITMRequest?{COMMON}"),
        ),
        (Call::psw("u@d", "00", DEVICE_ID), "POST", format!("/passport/v1/auth/psw?{COMMON}")),
        (
            Call::report_env(DEVICE_ID, &[], "T"),
            "POST",
            format!("/controller/v1/public/reportEnv?{COMMON}"),
        ),
        (Call::auth_check(), "GET", format!("/passport/v1/auth/authCheck?{COMMON}")),
        (Call::online_info(), "GET", format!("/passport/v1/user/onlineInfo?{COMMON}")),
        (
            Call::client_resource(),
            "POST",
            format!("/controller/v1/user/clientResource?{COMMON}"),
        ),
    ];
    for (call, method, expected) in rows {
        let request = request(&call);
        assert_eq!(request.method, method.as_bytes(), "{expected}");
        assert_eq!(target(&request), expected);
        assert_eq!(call.path(), expected.split('?').next().unwrap());
    }
}

// §4.2: the per-request header table of the password path, and each content type.
#[test]
fn each_request_carries_exactly_the_headers_of_the_table() {
    let json = Some("application/json;charset=utf-8");
    let rows: [(Call, &[&str], Option<&str>); 8] = [
        (Call::manifest(), &[], None),
        (
            Call::auth_config(),
            &["x-csrf-token", "x-sdp-rid", "x-sdp-traceid"],
            None,
        ),
        (
            Call::anti_mitm("N", "T", ""),
            &["content-type", "x-csrf-token", "x-sdp-rid", "x-sdp-traceid"],
            Some("application/json"),
        ),
        (
            Call::psw("u@d", "00", DEVICE_ID),
            &["content-type", "x-csrf-token", "x-sdp-env", "x-sdp-traceid"],
            json,
        ),
        (
            Call::report_env(DEVICE_ID, &[], "T"),
            &[
                "content-type",
                "x-csrf-token",
                "x-request-sig",
                "x-sdp-traceid",
            ],
            json,
        ),
        (Call::auth_check(), &["x-csrf-token", "x-sdp-traceid"], None),
        (
            Call::online_info(),
            &["x-csrf-token", "x-sdp-traceid"],
            None,
        ),
        (
            Call::client_resource(),
            &["content-type", "x-csrf-token", "x-sdp-traceid"],
            json,
        ),
    ];
    for (call, expected, content_type) in rows {
        let request = request(&call);
        assert_eq!(particular_headers(&request), expected, "{}", call.path());
        assert_eq!(
            header(&request, "Content-Type"),
            content_type,
            "{}",
            call.path()
        );
        assert_eq!(header(&request, "User-Agent"), Some(USER_AGENT));
        assert_eq!(header(&request, "Host"), Some("sdp.example.org:8443"));
        assert_eq!(
            header(&request, "Accept-Encoding"),
            None,
            "§2.2: no compressed answers"
        );
    }
}

// §4: the base host leaves out the default port. V8: the RID and env header encodings.
#[test]
fn session_values_reach_their_headers() {
    let request = request(&Call::auth_config());
    assert_eq!(header(&request, "x-csrf-token"), Some("csrf-1"));
    assert_eq!(header(&request, "x-sdp-traceid"), Some("0a1b2c3d"));
    assert_eq!(
        header(&request, "x-sdp-rid"),
        Some("c2RwLmV4YW1wbGUub3JnOjg0NDM=")
    );
    assert_eq!(
        header(
            &self::request(&Call::psw("u@d", "00", DEVICE_ID)),
            "x-sdp-env"
        ),
        Some("eyJkZXZpY2VJZCI6IjAwMTEyMjMzNDQ1NTY2Nzc4ODk5YWFiYmNjZGRlZWZmIn0=")
    );

    let default_port = Gateway::new("sdp.example.org", 443);
    assert_eq!(default_port.base_host(), "sdp.example.org");
    assert_eq!(
        Gateway::new("sdp.example.org", 8443).base_host(),
        "sdp.example.org:8443"
    );
    let context = Context {
        gateway: &default_port,
        // §4.2: the header is sent even while the token is empty.
        csrf_token: "",
        trace_id: "0a1b2c3d",
        sign_key: None,
        cookie: Some(b"sid=S1D-example; lang=en"),
    };
    let request = Call::auth_check().request(&context).unwrap();
    assert_eq!(header(&request, "Host"), Some("sdp.example.org"));
    assert_eq!(header(&request, "x-csrf-token"), Some(""));
    assert_eq!(header(&request, "Cookie"), Some("sid=S1D-example; lang=en"));
}

// V6: the second-stage request body, white space included.
#[test]
fn the_anti_mitm_body_matches_v6() {
    let nonce = "8A5BF19A11717BF9A74BB450E2EEC26E882AC7339744AA3505344A94E544E7B8";
    let request = request(&Call::anti_mitm(nonce, "TK-0001", DEVICE_ID));
    assert_eq!(request.body.len(), 166);
    assert_eq!(header(&request, "Content-Length"), Some("166"));

    let mut hash = Sha256::new();
    hash.update(&request.body).unwrap();
    let digest = hash
        .finalize()
        .as_ref()
        .iter()
        .fold(String::new(), |mut hex, byte| {
            write!(hex, "{byte:02X}").unwrap();
            hex
        });
    assert_eq!(
        digest,
        "24002AC288FCC0B3A38E87EB4657AB2430167DFB04E656D28DAEE2185B660DC6"
    );
}

// V7: the reportEnv body and its signature under K0.
#[test]
fn report_env_matches_v7() {
    let macs = ["02:00:5e:00:53:01".to_owned()];
    let request = request(&Call::report_env(DEVICE_ID, &macs, "TK-0001"));
    assert_eq!(
        std::str::from_utf8(&request.body).unwrap(),
        r#"{"deviceId":"00112233445566778899aabbccddeeff","env":{"endpoint":{"device":{"type":"browser"},"device_id":"00112233445566778899aabbccddeeff","mac_addresses":["02:00:5e:00:53:01"]}},"ticket":"TK-0001"}"#
    );
    assert_eq!(
        header(&request, "X-Request-Sig"),
        Some("E2FDAB67D324A0A6B4A64EC447DC93F782F451DC09EAE08395CF228AD198E140")
    );
}

// §4.3 E5 and E23: the bodies.
#[test]
fn the_psw_and_client_resource_bodies_are_as_specified() {
    let psw = request(&Call::psw("user01@local", "0badc0de", DEVICE_ID));
    let body: serde_json::Value = serde_json::from_slice(&psw.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"password": "0badc0de", "rememberPwd": "0", "username": "user01@local"}),
        "no graphCheckCode member unless a captcha was answered"
    );

    let resource = request(&Call::client_resource());
    assert_eq!(
        std::str::from_utf8(&resource.body).unwrap(),
        r#"{"resourceType":{"appList":{},"favoriteAppList":{},"featureCenter":{},"sdpPolicy":{},"uemSpace":{"params":{"action":"login"}}}}"#
    );
}

// §4.3, §4.4: the envelope and its codes.
#[test]
fn the_envelope_tells_success_refusal_and_a_lost_session_apart() {
    let accepted = |body: &str| Envelope::parse(body.as_bytes()).unwrap().accepted();
    assert!(accepted(r#"{"code":0,"message":"ok","data":{}}"#).is_ok());
    assert!(
        accepted(r#"{"data":{}}"#).is_ok(),
        "an answer with no code is accepted"
    );
    for code in [10_000_004, 75_500_002] {
        let result = accepted(&format!(r#"{{"code":{code},"message":"gone"}}"#));
        assert!(matches!(result, Err(Error::SessionInvalid { code: found }) if found == code));
    }
    let refused = accepted(r#"{"code":75500000,"message":"stale"}"#);
    assert!(matches!(
        refused,
        Err(Error::Refused { code: 75_500_000, message }) if message == "stale"
    ));
}

// §4.3 notes: E1, E2 and E3 must answer 200; only for E2 do 401 and 403 mean a lost session.
#[test]
fn a_status_other_than_200_is_not_an_answer() {
    for status in [201, 302, 401, 403, 404, 500] {
        let answer = response(status, &[], br#"{"code":0}"#);
        assert!(matches!(Envelope::of_ok(&answer), Err(Error::Status(found)) if found == status));
        assert!(matches!(Manifest::parse(&answer), Err(Error::Status(found)) if found == status));
        assert!(
            matches!(AntiMitmReply::parse(&answer), Err(Error::Status(found)) if found == status)
        );
        match (status, AuthConfig::parse(&answer)) {
            (401 | 403, Err(Error::SessionInvalid { code })) => assert_eq!(code, i64::from(status)),
            (401 | 403, other) => {
                panic!("status {status} on E2 must be a lost session, not {other:?}")
            }
            (_, Err(Error::Status(found))) => assert_eq!(found, status),
            (_, other) => panic!("status {status} on E2 must be a failure, not {other:?}"),
        }
    }
    // The other endpoints do not look at the status: only the body counts.
    let answer = response(500, &[], br#"{"code":0,"data":{"username":"user01"}}"#);
    assert_eq!(OnlineInfo::parse(&answer).unwrap().username, "user01");
    assert!(NextStep::parse(&answer).is_ok());
    assert_eq!(PswReply::parse(&answer).unwrap().code, 0);
}

// §4.3 E2, §5.2: the fields of authConfig.
#[test]
fn auth_config_fields_are_stored_as_received() {
    let body = r#"{"code":0,"data":{
        "csrfToken":"","security":{"csrfToken":"csrf-2"},
        "pubKey":"00C0FFEE","pubKeyExp":"65537","antiReplayRand":"r1","isLogin":0,
        "authServerInfoList":[
          {"loginDomain":"Local","authId":"1","authType":"auth/psw"},
          {"loginDomain":"local","authId":"2","authType":"auth/smsCheckCode"},
          {"loginDomain":"local","authId":"3","authType":"auth/psw","authName":"Password","subType":"x","loginUrl":"https://sdp.example.org/l"},
          {"loginDomain":"local","authId":"4","authType":"auth/psw"}],
        "antiMITMAttackData":{ "enable":1,"devicePubKeyMod":"00C0FFEE5EED", "devicePubKeyExp":"65537",
          "rsaCert":"QUJD","sm2encCert":"REVG","challenge":"example-challenge-0001","encryptedChallenge":"AB",
          "ticket":"TK-0001","antiMITMRequest":true,"mitmSig":"cd" }}}"#;
    let config = AuthConfig::parse(&ok(body)).unwrap();

    assert_eq!(
        config.csrf_token, "csrf-2",
        "the second place is used when the first is empty"
    );
    assert_eq!(
        (
            config.pub_key.as_str(),
            config.pub_key_exp.as_str(),
            config.anti_replay_rand.as_str()
        ),
        ("00C0FFEE", "65537", "r1")
    );
    assert!(!config.is_login);
    assert_eq!(config.servers.len(), 4);

    // The first entry whose type and domain both match exactly; `Local` is not `local`.
    let server = config.server("auth/psw", "local").unwrap();
    assert_eq!(
        (server.auth_id.as_str(), server.auth_name.as_str()),
        ("3", "Password")
    );
    assert_eq!(
        (server.sub_type.as_str(), server.login_url.as_str()),
        ("x", "https://sdp.example.org/l")
    );
    assert!(config.server("auth/psw", "LOCAL").is_none());
    assert!(config.server("auth/cas", "local").is_none());

    let anti_mitm = config.anti_mitm.unwrap();
    assert_eq!(anti_mitm.enable, 1);
    assert_eq!(anti_mitm.device_pub_key_mod, "00C0FFEE5EED");
    assert_eq!(anti_mitm.device_pub_key_exp, "65537");
    assert_eq!(
        (anti_mitm.rsa_cert.as_str(), anti_mitm.sm2_enc_cert.as_str()),
        ("QUJD", "REVG")
    );
    assert_eq!(anti_mitm.challenge, "example-challenge-0001");
    assert_eq!(anti_mitm.encrypted_challenge, "AB");
    assert_eq!(anti_mitm.ticket, "TK-0001");
    assert!(anti_mitm.anti_mitm_request);
    assert_eq!(anti_mitm.mitm_sig, "cd");
    // §2.5: the object's text survives exactly, white space and all.
    assert!(
        anti_mitm
            .raw
            .starts_with(r#"{ "enable":1,"devicePubKeyMod":"00C0FFEE5EED", "#)
    );
    assert!(anti_mitm.raw.ends_with(r#""mitmSig":"cd" }"#));
}

// §4.3 E2, §5.2: the first CSRF place wins when set; isLogin is 1 or it is not; a code that
// is present must be zero, and an answer with none is accepted.
#[test]
fn auth_config_applies_the_login_rules() {
    let parse = |body: &str| AuthConfig::parse(&ok(body));
    let data = |data: &str| parse(&format!(r#"{{"code":0,"data":{data}}}"#)).unwrap();

    let config = data(r#"{"csrfToken":"a","security":{"csrfToken":"b"},"isLogin":1}"#);
    assert_eq!(config.csrf_token, "a");
    assert!(config.is_login);
    assert!(!data(r#"{"isLogin":2}"#).is_login);
    assert!(!data("{}").is_login, "a missing isLogin counts as 0");
    // §5.3: an absent, null or empty anti-MITM object is no object.
    for payload in [
        "{}",
        r#"{"antiMITMAttackData":null}"#,
        r#"{"antiMITMAttackData":{}}"#,
        r#"{"antiMITMAttackData": { } }"#,
    ] {
        assert_eq!(data(payload).anti_mitm, None, "{payload}");
    }
    assert_eq!(data("null"), AuthConfig::default());

    assert_eq!(parse(r#"{"data":{"pubKey":"AB"}}"#).unwrap().pub_key, "AB");
    assert!(matches!(
        parse(r#"{"code":10000004,"data":null}"#),
        Err(Error::SessionInvalid { code: 10_000_004 })
    ));
    assert!(matches!(
        parse(r#"{"code":7,"message":"no","data":{"pubKey":"AB"}}"#),
        Err(Error::Refused { code: 7, .. })
    ));
}

// §5.4, V28: the answer to a password login, read whatever its code.
#[test]
fn the_psw_reply_is_read_on_success_and_on_refusal() {
    let read = |body: &str| PswReply::parse(&ok(body)).unwrap();

    let reply = read(r#"{"code":0,"data":{"ticket":"TK-0001","nextService":""}}"#);
    assert_eq!(
        (
            reply.code,
            reply.ticket.as_str(),
            reply.next_service.as_str()
        ),
        (0, "TK-0001", "")
    );
    assert!(!reply.wants_captcha);

    let reply =
        read(r#"{"code":0,"data":{"ticket":"T","nextService":"auth/totp","antiReplayRand":"r2"}}"#);
    assert_eq!(
        (reply.next_service.as_str(), reply.anti_replay_rand.as_str()),
        ("auth/totp", "r2")
    );

    let refused =
        read(r#"{"code":75500001,"message":"captcha","data":{"graphCheckCodeEnable":1}}"#);
    assert_eq!(
        (refused.code, refused.message.as_str()),
        (75_500_001, "captcha")
    );
    assert!(refused.wants_captcha);
    assert_eq!(refused.ticket, "");
    // Only the value 1 is a demand for a captcha.
    assert!(!read(r#"{"code":1,"data":{"graphCheckCodeEnable":2}}"#).wants_captcha);
}

// V19: the next-step fields, as they arrive. E22: the user name.
#[test]
fn next_step_fields_are_read_as_they_arrive() {
    let read =
        |data: &str| NextStep::parse(&ok(&format!(r#"{{"code":0,"data":{data}}}"#))).unwrap();

    let step = read(
        r#"{"nextService":"auth/token","nextServiceList":[{"authId":"a-17","authType":"auth/radius","subType":"s"}]}"#,
    );
    assert_eq!(step.next_service, "auth/token");
    assert_eq!(step.next_service_list.len(), 1);
    let entry = &step.next_service_list[0];
    assert_eq!(
        (
            entry.auth_id.as_str(),
            entry.auth_type.as_str(),
            entry.sub_type.as_str()
        ),
        ("a-17", "auth/radius", "s")
    );
    let step = read(r#"{"nextService":"","nextServiceList":[{"authId":"a-18"}]}"#);
    assert_eq!(
        (
            step.next_service.as_str(),
            step.next_service_list[0].auth_type.as_str()
        ),
        ("", "")
    );
    assert_eq!(
        read(r#"{"nextService":"auth/sendSms"}"#)
            .next_service_list
            .len(),
        0
    );

    // §4.3 notes: a code other than zero fails the step.
    assert!(matches!(
        NextStep::parse(&ok(r#"{"code":9,"message":"no"}"#)),
        Err(Error::Refused { code: 9, .. })
    ));

    let info = |body: &str| OnlineInfo::parse(&ok(body));
    assert_eq!(
        info(r#"{"code":0,"data":{"username":"user01"}}"#)
            .unwrap()
            .username,
        "user01"
    );
    // V28: an empty name is accepted.
    assert_eq!(
        info(r#"{"code":0,"data":{"username":""}}"#)
            .unwrap()
            .username,
        ""
    );
    assert!(matches!(
        info(r#"{"code":10000004}"#),
        Err(Error::SessionInvalid { .. })
    ));
}

// V18: the manifest.
#[test]
fn the_manifest_matches_v18() {
    let manifest = |version: &str, enable: bool| {
        ok(&format!(
            r#"{{"code":0,"data":{{"capacities":{{"zeroRTT":{{"version":"{version}"}}}},"options":{{"tun0rtt":{{"enable":{enable}}}}}}}}}"#
        ))
    };
    assert!(Manifest::parse(&manifest("2.0", true)).unwrap().zero_rtt);
    assert!(!Manifest::parse(&manifest("", true)).unwrap().zero_rtt);
    assert!(!Manifest::parse(&manifest("2.0", false)).unwrap().zero_rtt);
    assert!(!Manifest::parse(&ok(r#"{"code":0}"#)).unwrap().zero_rtt);

    // Not obtained: a code other than zero.
    assert!(matches!(
        Manifest::parse(&ok(r#"{"code":1,"message":"no"}"#)),
        Err(Error::Refused { code: 1, .. })
    ));
}

// V6: the second-stage answer: its nonce, its signature header and its exact body.
#[test]
fn the_anti_mitm_reply_matches_v6() {
    let body = r#"{"code":0,"data":{"nonce":"8A5BF19A11717BF9A74BB450E2EEC26E882AC7339744AA3505344A94E544E7B8","antiMITMEnable":1}}"#;
    let signature = "EE78499E113699CC1D5326246C2C260268E2F22D1E63A27EC9054A5721FD809A";
    let response = response(
        200,
        &[&format!("x-response-sig: {signature}")],
        body.as_bytes(),
    );
    let reply = AntiMitmReply::parse(&response).unwrap();
    assert_eq!(reply.code, Some(0));
    assert_eq!(
        reply.nonce,
        "8A5BF19A11717BF9A74BB450E2EEC26E882AC7339744AA3505344A94E544E7B8"
    );
    assert_eq!(reply.signature, signature);
    assert_eq!(reply.body, body.as_bytes());

    let unsigned = ok(body);
    assert_eq!(AntiMitmReply::parse(&unsigned).unwrap().signature, "");
    // §3.6: the code is the check's to judge, so a refusal still reads.
    let refused = ok(r#"{"code":10000008,"data":{"nonce":"N"}}"#);
    assert_eq!(
        AntiMitmReply::parse(&refused).unwrap().code,
        Some(10_000_008)
    );
}
