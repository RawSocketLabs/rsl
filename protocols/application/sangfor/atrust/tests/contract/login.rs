//! The login sequence (spec §5): which request follows which, and what each carries from
//! the ones before.

use crypto_legacy::rsa::{
    RsaPrivateKey,
    pkcs1v15::{Pkcs1v15PrivateOperations, RsaPkcs1v15Ciphertext},
};
use http1::Response;
use sangfor_atrust::api::NextStep;
use sangfor_atrust::login::{AntiMitm, Check, Credentials, Error, Login, Options, Step};
use sangfor_atrust_sig::{SignKey, canonical_string};

use crate::support::fixtures::{
    ANTI_MITM, AUTH_CHECK, AUTH_CONFIG, CLIENT_RESOURCE, CREDENTIALS, DEVICE_ID, MANIFEST, MODULUS,
    NOW, ONLINE_INFO, PSW, REPORT_ENV, V21, auth_config, login, manifest, psw_accepted, session,
};
use crate::support::{Script, header, ok, response};

const PRIVATE_EXPONENT: &str = concat!(
    "a8d6988fca9d98cf7e066e8bc0e907b491dfc8b1672ab98a3b57e009ae28a368612bf1013b2089bb",
    "60e073e6cd75f664fabfc26c29968e423cac17f2b99f49f264164c1c1ebae333fa4d2b266d0e07fd",
    "42757df17607eb4b05c28a9bcdc5d665d46b6064d9d991c91f870961a6283da7451471d7502c29cf",
    "5c4abce16fbfa225",
);
/// The nonce of vector V4.
const NONCE: &str = "8A5BF19A11717BF9A74BB450E2EEC26E882AC7339744AA3505344A94E544E7B8";
/// The stand-in certificate of vector V5, and its base64.
const CERTIFICATE: [u8; 5] = [0x30, 0x03, 0x02, 0x01, 0x05];
const CERTIFICATE_BASE64: &str = "MAMCAQU=";

fn unhex(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect()
}

/// The script of a whole password login that needs no second factor.
fn whole_login() -> Script {
    Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, auth_config(""))
        .then(PSW, psw_accepted(""))
        .then(REPORT_ENV, ok(r#"{"code":0}"#))
        .then(
            AUTH_CHECK,
            ok(r#"{"code":0,"data":{"nextService":"","nextServiceList":[]}}"#),
        )
        .then(
            ONLINE_INFO,
            ok(r#"{"code":0,"data":{"username":"user01"}}"#),
        )
        .then(CLIENT_RESOURCE, ok(V21))
}

// §5.2: the sequence, and what it yields.
#[test]
fn a_password_login_runs_the_sequence_and_yields_the_session() {
    let (mut session, mut script) = (session(), whole_login());
    let login = login(&mut session, &mut script, Options::default()).unwrap();

    assert!(script.is_finished());
    assert_eq!(
        script.paths(),
        [
            MANIFEST,
            AUTH_CONFIG,
            PSW,
            REPORT_ENV,
            AUTH_CHECK,
            ONLINE_INFO,
            CLIENT_RESOURCE
        ]
    );
    // The sid is read last: the password answer replaced the one authConfig set.
    assert_eq!(login.sid, "S1D-example");
    assert_eq!(session.sid(NOW), Some("S1D-example"));
    assert_eq!(login.user_name, "user01");
    assert!(login.zero_rtt, "V18");
    assert_eq!(login.anti_mitm, AntiMitm::Absent);
    // The policy is V21's, with the gateway's host standing in for the placeholder.
    assert_eq!(login.policy.node("ng-1"), Some("sdp.example.org:441"));
    assert_eq!(login.policy.ip_rules.len(), 2);
}

// §4.2, §5.2: what each request carries from the answers before it.
#[test]
fn each_request_carries_what_the_answers_before_it_established() {
    let (mut session, mut script) = (session(), whole_login());
    let _ = login(&mut session, &mut script, Options::default()).unwrap();

    // Nothing of the session goes with the manifest, and no token exists before authConfig.
    assert_eq!(header(script.request(MANIFEST), "Cookie"), None);
    assert_eq!(
        header(script.request(AUTH_CONFIG), "x-csrf-token"),
        Some("")
    );
    assert_eq!(header(script.request(AUTH_CONFIG), "Cookie"), None);

    let psw = script.request(PSW);
    assert_eq!(header(psw, "x-csrf-token"), Some("csrf-1"));
    assert_eq!(header(psw, "Cookie"), Some("sid=S1D-early"));
    for path in [REPORT_ENV, AUTH_CHECK, ONLINE_INFO, CLIENT_RESOURCE] {
        assert_eq!(
            header(script.request(path), "Cookie"),
            Some("sid=S1D-example"),
            "{path}"
        );
        assert_eq!(
            header(script.request(path), "x-csrf-token"),
            Some("csrf-1"),
            "{path}"
        );
    }

    // §3.8: a trace id is eight lowercase hex digits, fresh for each request.
    let trace_ids: Vec<&str> = script.seen[1..]
        .iter()
        .map(|request| header(request, "x-sdp-traceid").unwrap())
        .collect();
    for id in &trace_ids {
        assert!(
            id.len() == 8
                && id
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
            "{id}"
        );
    }
    let mut distinct = trace_ids.clone();
    distinct.sort_unstable();
    distinct.dedup();
    assert_eq!(distinct.len(), trace_ids.len());
}

// V28, §3.4: the password request names `user@domain` and encrypts `password_rand`.
#[test]
fn the_password_request_matches_v28() {
    let (mut session, mut script) = (session(), whole_login());
    let _ = login(&mut session, &mut script, Options::default()).unwrap();

    let body: serde_json::Value = serde_json::from_slice(&script.request(PSW).body).unwrap();
    assert_eq!(body["username"], "user01@local");
    let private = RsaPrivateKey::from_components(unhex(MODULUS), unhex(PRIVATE_EXPONENT)).unwrap();
    let block = RsaPkcs1v15Ciphertext::from_bytes(unhex(body["password"].as_str().unwrap()));
    assert_eq!(private.decrypt_pkcs1v15(&block).unwrap(), b"pw_r1");

    assert_eq!(CREDENTIALS.qualified_username(), "user01@local");
    let odd = Credentials {
        username: "a@b",
        ..CREDENTIALS
    };
    assert_eq!(odd.qualified_username(), "a@b@local");
    assert!(
        !format!("{CREDENTIALS:?}").contains("pw"),
        "a password is never printed"
    );
}

// §3.7, §5.2: reportEnv carries the ticket and is signed with the key of the latest
// authConfig, which exists even when the answer had no anti-MITM object (V2's K0).
#[test]
fn the_environment_report_is_signed_with_the_session_key() {
    let (mut session, mut script) = (session(), whole_login());
    let _ = login(&mut session, &mut script, Options::default()).unwrap();

    let report = script.request(REPORT_ENV);
    let body: serde_json::Value = serde_json::from_slice(&report.body).unwrap();
    assert_eq!(body["ticket"], "TK-0001");
    assert_eq!(body["deviceId"], DEVICE_ID);
    assert_eq!(
        body["env"]["endpoint"]["mac_addresses"],
        serde_json::json!(["02:00:5e:00:53:01"])
    );

    // The inputs are V7's (device, MAC address, ticket, and K0 for want of an object), so
    // the signature is V7's.
    assert_eq!(
        header(report, "X-Request-Sig"),
        Some("E2FDAB67D324A0A6B4A64EC447DC93F782F451DC09EAE08395CF228AD198E140")
    );
}

// §5.2: a gateway that already knows the cookies asks for no password.
#[test]
fn a_session_the_gateway_still_knows_skips_the_password() {
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, auth_config(r#","isLogin":1"#))
        .then(ONLINE_INFO, ok(r#"{"code":0,"data":{"username":""}}"#))
        .then(CLIENT_RESOURCE, ok(V21));
    let mut session = session();
    let login = login(&mut session, &mut script, Options::default()).unwrap();
    assert!(script.is_finished());
    // V28: an empty user name is accepted.
    assert_eq!(
        (login.sid.as_str(), login.user_name.as_str()),
        ("S1D-early", "")
    );
}

// V28, §5.2: the primary answer's service is taken literally and starts the chain.
#[test]
fn a_named_first_step_replaces_the_default_one() {
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, auth_config(""))
        .then(PSW, psw_accepted("auth/totp"))
        .then(REPORT_ENV, ok(r#"{"code":0}"#));
    let found = login(&mut session(), &mut script, Options::default());
    assert!(
        matches!(found, Err(Error::UnsupportedStep(step)) if step.service == "auth/totp" && step.auth_id.is_empty())
    );
    assert!(
        script.is_finished(),
        "the report is sent, and no authCheck precedes the step"
    );
}

// §5.5: an authCheck answer is resolved, and the step it names is the next one.
#[test]
fn the_step_an_auth_check_names_is_followed() {
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, auth_config(""))
        .then(PSW, psw_accepted(""))
        .then(REPORT_ENV, ok(r#"{"code":0}"#))
        .then(AUTH_CHECK, ok(r#"{"code":0,"data":{"nextService":"auth/token","nextServiceList":[{"authId":"a-17","authType":"auth/radius"}]}}"#));
    let found = login(&mut session(), &mut script, Options::default());
    assert!(
        matches!(found, Err(Error::UnsupportedStep(step)) if step.service == "auth/radius" && step.auth_id == "a-17")
    );

    // A chain of authChecks is followed until none is named.
    let again = r#"{"code":0,"data":{"nextService":"auth/authCheck"}}"#;
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, auth_config(""))
        .then(PSW, psw_accepted("auth/authCheck"))
        .then(REPORT_ENV, ok(r#"{"code":0}"#))
        .then(AUTH_CHECK, ok(again))
        .then(AUTH_CHECK, ok(again))
        .then(AUTH_CHECK, ok(r#"{"code":0,"data":{}}"#))
        .then(
            ONLINE_INFO,
            ok(r#"{"code":0,"data":{"username":"user01"}}"#),
        )
        .then(CLIENT_RESOURCE, ok(V21));
    let _ = login(&mut session(), &mut script, Options::default()).unwrap();
    assert!(script.is_finished());
}

// V19 and the table of §5.5.
#[test]
fn next_step_resolution_matches_the_table() {
    let resolve = |data: &str| {
        let fields = NextStep::parse(&ok(&format!(r#"{{"code":0,"data":{data}}}"#))).unwrap();
        Step::resolve(&fields).map(|step| (step.service, step.auth_id))
    };
    let step = |service: &str, auth_id: &str| Some((service.to_owned(), auth_id.to_owned()));

    // V19.
    assert_eq!(
        resolve(
            r#"{"nextService":"auth/token","nextServiceList":[{"authId":"a-17","authType":"auth/radius"}]}"#
        ),
        step("auth/radius", "a-17")
    );
    assert_eq!(
        resolve(r#"{"nextService":"","nextServiceList":[{"authId":"a-18"}]}"#),
        step("auth/sms", "a-18")
    );
    assert_eq!(
        resolve(r#"{"nextService":"auth/sendSms"}"#),
        step("auth/sms", "")
    );

    // No service and no usable entry: the chain is complete.
    assert_eq!(resolve("{}"), None);
    assert_eq!(resolve(r#"{"nextService":"","nextServiceList":[]}"#), None);
    assert_eq!(
        resolve(r#"{"nextService":"","nextServiceList":[{"subType":"x"}]}"#),
        None
    );
    // No service, an entry with a type.
    assert_eq!(
        resolve(r#"{"nextServiceList":[{"authId":"a-1","authType":"auth/customSms"}]}"#),
        step("auth/customSms", "a-1")
    );
    // auth/token narrowed by its entry, or left undispatchable.
    for kind in ["auth/totp", "auth/radius", "auth/challenge"] {
        let data = format!(
            r#"{{"nextService":"auth/token","nextServiceList":[{{"authId":"a","authType":"{kind}"}}]}}"#
        );
        assert_eq!(resolve(&data), step(kind, "a"));
    }
    assert_eq!(
        resolve(
            r#"{"nextService":"auth/token","nextServiceList":[{"authId":"a","authType":"auth/token","subType":"totp"}]}"#
        ),
        step("auth/totp", "a")
    );
    assert_eq!(
        resolve(
            r#"{"nextService":"auth/token","nextServiceList":[{"authId":"a","authType":"auth/token","subType":"x"}]}"#
        ),
        step("auth/token", "a")
    );
    assert_eq!(
        resolve(r#"{"nextService":"auth/token"}"#),
        step("auth/token", "")
    );
    // Any other service stands; the entry of that type gives the id, else the first entry.
    assert_eq!(
        resolve(
            r#"{"nextService":"auth/sms","nextServiceList":[{"authId":"first","authType":"auth/totp"},{"authId":"second","authType":"auth/sms"}]}"#
        ),
        step("auth/sms", "second")
    );
    assert_eq!(
        resolve(
            r#"{"nextService":"auth/accessCheck","nextServiceList":[{"authId":"first","authType":"auth/totp"}]}"#
        ),
        step("auth/accessCheck", "first")
    );
}

/// The anti-MITM object of vector V3 with `changes` applied, re-signed with the key the
/// object itself establishes unless a change sets `mitmSig`.
fn anti_mitm_object(changes: &[(&str, serde_json::Value)]) -> String {
    let mut object = serde_json::json!({
        "enable": 1, "antiMITMRequest": false,
        "devicePubKeyMod": "00C0FFEE5EED", "devicePubKeyExp": "65537",
        "rsaCert": "", "sm2encCert": "",
        "challenge": "example-challenge-0001",
        "encryptedChallenge": "91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED",
        "ticket": "TK-0001",
    });
    for (name, value) in changes {
        object[*name] = value.clone();
    }
    if object.get("mitmSig").is_none() {
        let key = SignKey::derive("00C0FFEE5EED", "65537", "example-challenge-0001").unwrap();
        let canonical = canonical_string(&object.to_string()).unwrap();
        object["mitmSig"] = key.sign(canonical.as_bytes()).unwrap().into();
    }
    object.to_string()
}

/// Log in to a gateway that still knows the session and whose `authConfig` answer carries
/// `object`, over a connection presenting `chain`; `second_stage` scripts what follows.
fn anti_mitm_outcome(
    object: &str,
    chain: Vec<Vec<u8>>,
    second_stage: impl FnOnce(Script) -> Script,
    options: Options,
) -> (Result<Login, Error>, Script) {
    let config = auth_config(&format!(r#","isLogin":1,"antiMITMAttackData":{object}"#));
    let script = Script::default()
        .then(MANIFEST, manifest())
        .then_over(AUTH_CONFIG, config, chain);
    let mut script = second_stage(script)
        .then(
            ONLINE_INFO,
            ok(r#"{"code":0,"data":{"username":"user01"}}"#),
        )
        .then(CLIENT_RESOURCE, ok(V21));
    let found = login(&mut session(), &mut script, options);
    (found, script)
}

/// The second-stage answer of vector V6.
fn v6_answer() -> Response {
    let body = format!(r#"{{"code":0,"data":{{"nonce":"{NONCE}","antiMITMEnable":1}}}}"#);
    let signature =
        "X-Response-Sig: EE78499E113699CC1D5326246C2C260268E2F22D1E63A27EC9054A5721FD809A";
    response(200, &[signature], body.as_bytes())
}

// §5.3 with V1 and V3: the checks in order, each able to fail on its own.
#[test]
fn the_anti_mitm_checks_run_in_order_and_stop_at_the_first_failure() {
    let outcome = |object: &str, chain: Vec<Vec<u8>>| {
        let (login, script) = anti_mitm_outcome(object, chain, |script| script, Options::default());
        assert!(
            script.is_finished(),
            "a failed check is reported and the login goes on"
        );
        login.unwrap().anti_mitm
    };
    let named = (
        ("rsaCert", CERTIFICATE_BASE64.into()),
        ("antiMITMRequest", true.into()),
    );

    // V3's object as it stands: challenge and signature hold, and no certificate is named.
    let v3 = r#"{"enable":1,"antiMITMRequest":false,"devicePubKeyMod":"00C0FFEE5EED","devicePubKeyExp":"65537","rsaCert":"","sm2encCert":"","challenge":"example-challenge-0001","encryptedChallenge":"91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED","ticket":"TK-0001","mitmSig":"0e89931e17ea6ed910f9fd0e0491eb77b0f08f615ed4a7e97e696da8a04cf35f"}"#;
    assert_eq!(
        outcome(v3, vec![CERTIFICATE.to_vec()]),
        AntiMitm::Failed(Check::Certificate)
    );

    // V1: the same digits in lowercase are not the encrypted challenge.
    let lowercase = "91f62c3d0672e10a14cf23e7c5f18eaef19820e405852a2df51df478dd90b0ed";
    let object = anti_mitm_object(&[("encryptedChallenge", lowercase.into())]);
    assert_eq!(
        outcome(&object, Vec::new()),
        AntiMitm::Failed(Check::Challenge)
    );

    let object = anti_mitm_object(&[("mitmSig", "00".repeat(32).into())]);
    assert_eq!(
        outcome(&object, Vec::new()),
        AntiMitm::Failed(Check::Signature)
    );
    let object = anti_mitm_object(&[("mitmSig", "".into())]);
    assert_eq!(
        outcome(&object, Vec::new()),
        AntiMitm::Failed(Check::Signature)
    );

    // Not enabled: the certificate is not looked at.
    let object = anti_mitm_object(&[("enable", 0.into())]);
    assert_eq!(outcome(&object, Vec::new()), AntiMitm::Passed);

    // Enabled: some certificate of the chain must be the one named.
    let object = anti_mitm_object(&[named.0.clone(), named.1.clone()]);
    assert_eq!(
        outcome(&object, vec![vec![0x30, 0x00], CERTIFICATE.to_vec()]),
        AntiMitm::Passed
    );
    assert_eq!(
        outcome(&object, vec![vec![0x30, 0x00]]),
        AntiMitm::Failed(Check::Certificate)
    );
    assert_eq!(
        outcome(&object, Vec::new()),
        AntiMitm::Failed(Check::Certificate)
    );
    let by_sm2 = anti_mitm_object(&[("sm2encCert", CERTIFICATE_BASE64.into()), named.1]);
    assert_eq!(
        outcome(&by_sm2, vec![CERTIFICATE.to_vec()]),
        AntiMitm::Passed
    );
}

// §3.6, §5.3 with V4 and V6: the second-stage request and its answer.
#[test]
fn the_second_stage_request_matches_v6_and_its_answer_is_checked() {
    let object = anti_mitm_object(&[("rsaCert", CERTIFICATE_BASE64.into())]);
    let chain = || vec![CERTIFICATE.to_vec()];
    let with = |answer: Response| {
        anti_mitm_outcome(
            &object,
            chain(),
            |script| script.then(ANTI_MITM, answer),
            Options::default(),
        )
    };

    let (login, script) = with(v6_answer());
    assert_eq!(login.unwrap().anti_mitm, AntiMitm::Passed);
    assert!(script.is_finished());
    let request = script.request(ANTI_MITM);
    // V6: the body with V4's nonce and the object's ticket.
    assert_eq!(request.body.len(), 166);
    let body = String::from_utf8(request.body.clone()).unwrap();
    assert!(
        body.contains(&format!(r#""nonce": "{NONCE}""#)) && body.contains(r#""ticket": "TK-0001""#)
    );
    // §4.3: the token of the answer being validated, and the device id.
    assert_eq!(header(request, "x-csrf-token"), Some("csrf-1"));
    assert!(
        String::from_utf8(request.target.clone())
            .unwrap()
            .contains(&format!("mobileId={DEVICE_ID}"))
    );

    let failed = |answer: Response| {
        with(answer).0.unwrap().anti_mitm == AntiMitm::Failed(Check::SecondStage)
    };
    let body = format!(r#"{{"code":0,"data":{{"nonce":"{NONCE}","antiMITMEnable":1}}}}"#);
    let signed = |body: &str| {
        let key = SignKey::derive("00C0FFEE5EED", "65537", "example-challenge-0001").unwrap();
        format!("X-Response-Sig: {}", key.sign(body.as_bytes()).unwrap())
    };
    // No signature, a signature over other bytes, a lowercase one is still accepted.
    assert!(failed(ok(&body)));
    assert!(failed(response(200, &[&signed("other")], body.as_bytes())));
    let lowercase = signed(&body).replace("X-Response-Sig: ", "").to_lowercase();
    let (login, _) = with(response(
        200,
        &[&format!("X-Response-Sig: {lowercase}")],
        body.as_bytes(),
    ));
    assert_eq!(login.unwrap().anti_mitm, AntiMitm::Passed);
    // Another nonce, correctly signed.
    let other = body.replace(NONCE, &"0".repeat(64));
    assert!(failed(response(200, &[&signed(&other)], other.as_bytes())));
    // The two refusing codes, correctly signed, and a status other than 200.
    for code in [10_000_004, 10_000_008] {
        let refused = body.replace(r#""code":0"#, &format!(r#""code":{code}"#));
        assert!(
            failed(response(200, &[&signed(&refused)], refused.as_bytes())),
            "code {code}"
        );
    }
    assert!(failed(response(500, &[&signed(&body)], body.as_bytes())));
}

// §5.3: no second-stage request when the object says it is not wanted.
#[test]
fn no_second_stage_request_is_made_when_the_object_says_so() {
    let object = anti_mitm_object(&[
        ("rsaCert", CERTIFICATE_BASE64.into()),
        ("antiMITMRequest", true.into()),
    ]);
    let (login, script) = anti_mitm_outcome(
        &object,
        vec![CERTIFICATE.to_vec()],
        |script| script,
        Options::default(),
    );
    assert_eq!(login.unwrap().anti_mitm, AntiMitm::Passed);
    assert!(!script.paths().contains(&ANTI_MITM.to_owned()));
}

/// The object of V3, re-signed, naming V5's certificate: the certificate check passes over
/// a connection presenting it, and a second-stage request follows.
fn object_reaching_the_second_stage() -> String {
    anti_mitm_object(&[("rsaCert", CERTIFICATE_BASE64.into())])
}

// §5.3, failure policy: fatal on request. Each check, on a path where the password would
// otherwise be sent next.
#[test]
fn a_strict_login_sends_nothing_after_a_failed_check() {
    let mut strict = Options::default();
    strict.strict_anti_mitm = true;
    let lowercase = "91f62c3d0672e10a14cf23e7c5f18eaef19820e405852a2df51df478dd90b0ed";
    let cases = [
        (
            Check::Challenge,
            anti_mitm_object(&[("encryptedChallenge", lowercase.into())]),
            None,
        ),
        (
            Check::Signature,
            anti_mitm_object(&[("mitmSig", "00".repeat(32).into())]),
            None,
        ),
        (
            Check::Certificate,
            anti_mitm_object(&[("rsaCert", "QUJD".into())]),
            None,
        ),
        (
            Check::SecondStage,
            object_reaching_the_second_stage(),
            Some(ok(r#"{"code":0}"#)),
        ),
    ];
    for (check, object, second_stage) in cases {
        let config = auth_config(&format!(r#","antiMITMAttackData":{object}"#));
        let mut script = Script::default().then(MANIFEST, manifest()).then_over(
            AUTH_CONFIG,
            config,
            vec![CERTIFICATE.to_vec()],
        );
        let mut expected = vec![MANIFEST, AUTH_CONFIG];
        if let Some(answer) = second_stage {
            script = script.then(ANTI_MITM, answer);
            expected.push(ANTI_MITM);
        }
        let mut session = session();
        let found = login(&mut session, &mut script, strict);
        assert!(
            matches!(found, Err(Error::AntiMitm(failed)) if failed == check),
            "{check:?}"
        );
        assert_eq!(
            script.paths(),
            expected,
            "{check:?}: no password after the failure"
        );
        // The cookies the failing gateway set do not carry into the next attempt.
        assert_eq!(session.sid(NOW), None, "{check:?}");
        assert_eq!(session.cookies().cookies().len(), 0, "{check:?}");
    }
}

#[test]
fn strict_login_preserves_prior_cookies_after_a_malformed_answer() {
    let mut session = session();
    session
        .cookies_mut()
        .store(b"sid=original; Path=/", b"/", NOW);
    let mut script = Script::default().then(
        MANIFEST,
        response(200, &["Set-Cookie: sid=replaced; Path=/"], b"invalid"),
    );
    let mut strict = Options::default();
    strict.strict_anti_mitm = true;
    assert!(login(&mut session, &mut script, strict).is_err());
    assert_eq!(session.sid(NOW), Some("original"));
}

// §5.3: by default the same failures are reported and the password is sent.
#[test]
fn a_default_login_reports_a_failed_check_and_goes_on() {
    let object = anti_mitm_object(&[("mitmSig", "00".repeat(32).into())]);
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(
            AUTH_CONFIG,
            auth_config(&format!(r#","antiMITMAttackData":{object}"#)),
        )
        .then(PSW, psw_accepted(""))
        .then(REPORT_ENV, ok(r#"{"code":0}"#))
        .then(AUTH_CHECK, ok(r#"{"code":0,"data":{}}"#))
        .then(
            ONLINE_INFO,
            ok(r#"{"code":0,"data":{"username":"user01"}}"#),
        )
        .then(CLIENT_RESOURCE, ok(V21));
    let login = login(&mut session(), &mut script, Options::default()).unwrap();
    assert_eq!(login.anti_mitm, AntiMitm::Failed(Check::Signature));
    assert!(script.is_finished());

    // A strict login is not stopped by checks that pass or are not made.
    let mut strict = Options::default();
    strict.strict_anti_mitm = true;
    let passing = anti_mitm_object(&[("enable", 0.into())]);
    let (found, _) = anti_mitm_outcome(&passing, Vec::new(), |script| script, strict);
    assert_eq!(found.unwrap().anti_mitm, AntiMitm::Passed);
}

// §5.3: the challenge is checked before the signature, and an object whose signed text
// cannot be built has no valid signature.
#[test]
fn the_challenge_comes_first_and_an_unsignable_object_fails_its_signature() {
    let outcome = |object: &str| {
        anti_mitm_outcome(object, Vec::new(), |script| script, Options::default())
            .0
            .unwrap()
            .anti_mitm
    };
    let both_wrong = anti_mitm_object(&[
        ("encryptedChallenge", "00".into()),
        ("mitmSig", "00".repeat(32).into()),
    ]);
    assert_eq!(outcome(&both_wrong), AntiMitm::Failed(Check::Challenge));

    // Nested past what the canonical string allows; no signature can cover it.
    let deep = format!("{}1{}", r#"{"a":"#.repeat(20), "}".repeat(20));
    let object = anti_mitm_object(&[("enable", 0.into()), ("mitmSig", "00".repeat(32).into())])
        .replacen('{', &format!(r#"{{"deep":{deep},"#), 1);
    assert_eq!(outcome(&object), AntiMitm::Failed(Check::Signature));
}

// §3.6: only the two named codes refuse; any other code leaves the nonce and signature to
// decide.
#[test]
fn a_second_stage_answer_with_another_code_is_judged_by_nonce_and_signature() {
    let body = format!(r#"{{"code":7,"data":{{"nonce":"{NONCE}"}}}}"#);
    let key = SignKey::derive("00C0FFEE5EED", "65537", "example-challenge-0001").unwrap();
    let signature = format!("X-Response-Sig: {}", key.sign(body.as_bytes()).unwrap());
    let answer = response(200, &[&signature], body.as_bytes());
    let (login, _) = anti_mitm_outcome(
        &object_reaching_the_second_stage(),
        vec![CERTIFICATE.to_vec()],
        |script| script.then(ANTI_MITM, answer),
        Options::default(),
    );
    assert_eq!(login.unwrap().anti_mitm, AntiMitm::Passed);
}

// §2.3: a cookie goes to the paths it is scoped to, and the session identifier is the `sid`
// the gateway's root would get (§5.2).
#[test]
fn cookies_keep_their_scope_and_the_sid_is_the_one_for_the_root() {
    // A cookie set without a path belongs to the directory of the request that set it.
    let psw = response(
        200,
        &[
            "Set-Cookie: sid=S1D-example; Path=/",
            "Set-Cookie: scoped=1",
            "Set-Cookie: sid=NARROW; Path=/passport",
        ],
        br#"{"code":0,"data":{"ticket":"TK-0001","nextService":""}}"#,
    );
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, auth_config(""))
        .then(PSW, psw)
        .then(REPORT_ENV, ok(r#"{"code":0}"#))
        .then(AUTH_CHECK, ok(r#"{"code":0,"data":{}}"#))
        .then(
            ONLINE_INFO,
            ok(r#"{"code":0,"data":{"username":"user01"}}"#),
        )
        .then(CLIENT_RESOURCE, ok(V21));
    let login = login(&mut session(), &mut script, Options::default()).unwrap();

    assert!(
        header(script.request(AUTH_CHECK), "Cookie")
            .unwrap()
            .contains("scoped=1")
    );
    assert!(
        !header(script.request(REPORT_ENV), "Cookie")
            .unwrap()
            .contains("scoped=1")
    );
    assert_eq!(login.sid, "S1D-example", "not the sid of a narrower path");

    // A sid that the root would not get is no session identifier.
    let narrow_only = response(
        200,
        &["Set-Cookie: sid=NARROW; Path=/passport"],
        br#"{"code":0,"data":{"isLogin":1}}"#,
    );
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, narrow_only)
        .then(ONLINE_INFO, ok(r#"{"code":0,"data":{}}"#))
        .then(CLIENT_RESOURCE, ok(V21));
    assert!(matches!(
        self::login(&mut session(), &mut script, Options::default()),
        Err(Error::NoSession)
    ));
}

// §5.2: stored cookies are in the jar before authConfig and not before the manifest.
#[test]
fn restored_cookies_reach_auth_config_and_not_the_manifest() {
    let mut session = session();
    session
        .cookies_mut()
        .store(b"sid=S1D-stored; Path=/", b"/", NOW);
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, ok(r#"{"code":0,"data":{"isLogin":1}}"#))
        .then(
            ONLINE_INFO,
            ok(r#"{"code":0,"data":{"username":"user01"}}"#),
        )
        .then(CLIENT_RESOURCE, ok(V21));
    let login = login(&mut session, &mut script, Options::default()).unwrap();
    assert_eq!(header(script.request(MANIFEST), "Cookie"), None);
    assert_eq!(
        header(script.request(AUTH_CONFIG), "Cookie"),
        Some("sid=S1D-stored")
    );
    assert_eq!(login.sid, "S1D-stored");
}

// §3.3, §5.2: every authConfig answer replaces the token and the key, empty or not.
#[test]
fn each_auth_config_answer_replaces_the_token_and_the_sign_key() {
    let rest = |script: Script| {
        script
            .then(PSW, psw_accepted(""))
            .then(REPORT_ENV, ok(r#"{"code":0}"#))
            .then(AUTH_CHECK, ok(r#"{"code":0,"data":{}}"#))
            .then(
                ONLINE_INFO,
                ok(r#"{"code":0,"data":{"username":"user01"}}"#),
            )
            .then(CLIENT_RESOURCE, ok(V21))
    };
    let mut session = session();

    // First: a token, and an object that establishes V2's K.
    let object = anti_mitm_object(&[("enable", 0.into())]);
    let config = auth_config(&format!(r#","antiMITMAttackData":{object}"#));
    let mut script = rest(
        Script::default()
            .then(MANIFEST, manifest())
            .then(AUTH_CONFIG, config),
    );
    let _ = login(&mut session, &mut script, Options::default()).unwrap();
    let report = script.request(REPORT_ENV);
    let k = SignKey::from_bytes(
        unhex("E7B4C6CFA59121FDCB33ECC990676E9F0ACA451320B7DEB06A25D8EF1D980DE7")
            .try_into()
            .unwrap(),
    );
    let signed = [&report.target[..], &report.body[..]].concat();
    assert!(
        k.verify(&signed, header(report, "X-Request-Sig").unwrap())
            .unwrap(),
        "signed with V2's K"
    );

    // Second, same session: no token and no object. The token is empty again and the key is
    // V2's K0, so the report's signature is V7's.
    let bare = format!(
        r#"{{"code":0,"data":{{"pubKey":"{MODULUS}","pubKeyExp":"65537","antiReplayRand":"r1","authServerInfoList":[{{"loginDomain":"local","authType":"auth/psw"}}]}}}}"#
    );
    let mut script = rest(
        Script::default()
            .then(MANIFEST, manifest())
            .then(AUTH_CONFIG, ok(&bare)),
    );
    let _ = login(&mut session, &mut script, Options::default()).unwrap();
    assert_eq!(header(script.request(PSW), "x-csrf-token"), Some(""));
    assert_eq!(
        header(script.request(REPORT_ENV), "X-Request-Sig"),
        Some("E2FDAB67D324A0A6B4A64EC447DC93F782F451DC09EAE08395CF228AD198E140")
    );
}

// §5.5, read literally: with no service named, the entry is the first whose type is empty
// too, else the first. Whether a gateway sends such a list is not known.
#[test]
fn with_no_service_named_an_entry_with_no_type_is_the_one_picked() {
    let resolve = |list: &str| {
        let body = format!(r#"{{"code":0,"data":{{"nextService":"","nextServiceList":{list}}}}}"#);
        Step::resolve(&NextStep::parse(&ok(&body)).unwrap())
            .map(|step| (step.service, step.auth_id))
    };
    assert_eq!(
        resolve(r#"[{"authId":"a","authType":"auth/totp"},{}]"#),
        None
    );
    assert_eq!(
        resolve(r#"[{"authId":"a","authType":"auth/totp"},{"authId":"b"}]"#),
        Some(("auth/sms".to_owned(), "b".to_owned()))
    );
}

// What identifies the session to a tunnel is never printed.
#[test]
fn the_session_identifier_is_never_printed() {
    let (mut session, mut script) = (session(), whole_login());
    let login = login(&mut session, &mut script, Options::default()).unwrap();
    for shown in [format!("{session:?}"), format!("{login:?}")] {
        assert!(
            !shown.contains("S1D-example") && !shown.contains("csrf-1"),
            "{shown}"
        );
    }
    assert!(format!("{login:?}").contains("user01"));
}

// §2.3: a session identifier that has expired is none.
#[test]
fn an_expired_sid_is_no_session_identifier() {
    let mut session = session();
    session
        .cookies_mut()
        .store(b"sid=S1D-brief; Path=/; Max-Age=10", b"/", NOW);
    assert_eq!(session.sid(NOW + 5), Some("S1D-brief"));
    assert_eq!(session.sid(NOW + 11), None);
}
