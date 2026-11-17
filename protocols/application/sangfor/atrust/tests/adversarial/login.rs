//! Gateways that refuse, stall, lie or break off during a login.

use sangfor_atrust::api;
use sangfor_atrust::login::{AntiMitm, Check, Credentials, Error, Options};

use crate::support::fixtures::{
    ANTI_MITM, AUTH_CHECK, AUTH_CONFIG, CLIENT_RESOURCE, MANIFEST, ONLINE_INFO, PSW, REPORT_ENV,
    V21, auth_config, login, manifest, psw_accepted, session,
};
use crate::support::{Counting, Script, ok, response};

/// A script up to and including an accepted password and its report.
fn authenticated() -> Script {
    Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, auth_config(""))
        .then(PSW, psw_accepted(""))
        .then(REPORT_ENV, ok(r#"{"code":0}"#))
}

#[test]
fn a_manifest_that_cannot_be_had_stops_the_login_at_once() {
    for answer in [response(503, &[], b"{}"), ok(r#"{"code":1}"#), ok("<html>")] {
        let mut script = Script::default().then(MANIFEST, answer);
        assert!(matches!(
            login(&mut session(), &mut script, Options::default()),
            Err(Error::Api(_))
        ));
    }
    let mut script = Script::default().then_fail(MANIFEST);
    assert!(matches!(
        login(&mut session(), &mut script, Options::default()),
        Err(Error::Transport(_))
    ));
}

#[test]
fn an_auth_config_that_refuses_stops_the_login() {
    let answers = [
        (response(401, &[], b"{}"), true),
        (ok(r#"{"code":10000004}"#), true),
        (ok(r#"{"code":75500002}"#), true),
        (response(500, &[], b"{}"), false),
        (ok(r#"{"code":9,"message":"no"}"#), false),
        (ok("not json"), false),
    ];
    for (answer, session_invalid) in answers {
        let mut script = Script::default()
            .then(MANIFEST, manifest())
            .then(AUTH_CONFIG, answer);
        let found = login(&mut session(), &mut script, Options::default());
        let invalid = matches!(found, Err(Error::Api(api::Error::SessionInvalid { .. })));
        assert!(matches!(found, Err(Error::Api(_))));
        assert_eq!(invalid, session_invalid);
    }
}

#[test]
fn no_password_is_sent_when_the_gateway_offers_none_for_the_domain() {
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, auth_config(""));
    let elsewhere = Credentials {
        username: "user01",
        password: "pw",
        domain: "Local",
    };
    let found = session().login(
        &mut script,
        &elsewhere,
        Options::default(),
        &mut Counting::default(),
        0,
    );
    assert!(matches!(found, Err(Error::NoLoginMethod { domain }) if domain == "Local"));
    assert!(script.is_finished());
}

#[test]
fn no_password_is_sent_under_a_key_that_cannot_be_used() {
    for (key, exponent) in [
        ("", "65537"),
        ("not hex", "65537"),
        ("00C0FFEE", "0"),
        ("00C0FFEE", ""),
    ] {
        let body = format!(
            r#"{{"code":0,"data":{{"pubKey":"{key}","pubKeyExp":"{exponent}","authServerInfoList":[{{"loginDomain":"local","authType":"auth/psw"}}]}}}}"#
        );
        let mut script = Script::default()
            .then(MANIFEST, manifest())
            .then(AUTH_CONFIG, ok(&body));
        let found = login(&mut session(), &mut script, Options::default());
        assert!(
            matches!(found, Err(Error::Sig(_))),
            "key {key:?}, exponent {exponent:?}"
        );
        assert!(script.is_finished());
    }
}

#[test]
fn a_refused_password_is_reported_as_what_it_is() {
    let attempt = |answer: &str| {
        let mut script = Script::default()
            .then(MANIFEST, manifest())
            .then(AUTH_CONFIG, auth_config(""))
            .then(PSW, ok(answer));
        let found = login(&mut session(), &mut script, Options::default());
        assert!(script.is_finished(), "nothing follows a refused password");
        found
    };
    assert!(matches!(
        attempt(r#"{"code":75500010,"message":"wrong password"}"#),
        Err(Error::Api(api::Error::Refused { code: 75_500_010, message })) if message == "wrong password"
    ));
    // A demand for a captcha, with a refusing code or with none.
    for answer in [
        r#"{"code":75500001,"data":{"graphCheckCodeEnable":1}}"#,
        r#"{"code":0,"data":{"graphCheckCodeEnable":1}}"#,
    ] {
        assert!(
            matches!(attempt(answer), Err(Error::CaptchaRequired)),
            "{answer}"
        );
    }
    assert!(matches!(
        attempt(r#"{"code":0,"data":{"ticket":""}}"#),
        Err(Error::NoTicket)
    ));
    assert!(matches!(
        attempt("<html>"),
        Err(Error::Api(api::Error::Json(_)))
    ));
}

#[test]
fn a_refused_environment_report_stops_the_login() {
    for answer in [r#"{"code":7,"message":"no"}"#, "not json"] {
        let mut script = Script::default()
            .then(MANIFEST, manifest())
            .then(AUTH_CONFIG, auth_config(""))
            .then(PSW, psw_accepted(""))
            .then(REPORT_ENV, ok(answer));
        assert!(matches!(
            login(&mut session(), &mut script, Options::default()),
            Err(Error::Api(_))
        ));
        assert!(script.is_finished());
    }
}

#[test]
fn a_chain_that_never_ends_is_given_up() {
    let again = r#"{"code":0,"data":{"nextService":"auth/authCheck"}}"#;
    let mut script = authenticated();
    for _ in 0..8 {
        script = script.then(AUTH_CHECK, ok(again));
    }
    let found = login(&mut session(), &mut script, Options::default());
    assert!(matches!(found, Err(Error::TooManySteps)));
    assert!(
        script.is_finished(),
        "exactly the limit's worth of steps were tried"
    );

    let mut short = Options::default();
    short.max_steps = 0;
    let mut script = authenticated();
    assert!(matches!(
        login(&mut session(), &mut script, short),
        Err(Error::TooManySteps)
    ));
}

#[test]
fn a_refused_step_user_or_policy_stops_the_login() {
    let step = ok(r#"{"code":0,"data":{}}"#);
    let user = ok(r#"{"code":0,"data":{"username":"user01"}}"#);
    let refusal = || ok(r#"{"code":7,"message":"no"}"#);

    let mut script = authenticated().then(AUTH_CHECK, refusal());
    assert!(matches!(
        login(&mut session(), &mut script, Options::default()),
        Err(Error::Api(_))
    ));

    let mut script = authenticated()
        .then(AUTH_CHECK, step.clone())
        .then(ONLINE_INFO, refusal());
    assert!(matches!(
        login(&mut session(), &mut script, Options::default()),
        Err(Error::Api(_))
    ));

    // A policy answer that refuses is not an empty policy.
    let mut script = authenticated()
        .then(AUTH_CHECK, step.clone())
        .then(ONLINE_INFO, user.clone())
        .then(CLIENT_RESOURCE, refusal());
    assert!(matches!(
        login(&mut session(), &mut script, Options::default()),
        Err(Error::Api(api::Error::Refused { code: 7, .. }))
    ));

    let mut script = authenticated()
        .then(AUTH_CHECK, step)
        .then(ONLINE_INFO, user)
        .then(CLIENT_RESOURCE, ok("<html>"));
    assert!(matches!(
        login(&mut session(), &mut script, Options::default()),
        Err(Error::Api(_))
    ));
}

#[test]
fn a_login_that_leaves_no_session_identifier_fails() {
    let finish = |script: Script| {
        script.then(AUTH_CHECK, ok(r#"{"code":0,"data":{}}"#)).then(
            ONLINE_INFO,
            ok(r#"{"code":0,"data":{"username":"user01"}}"#),
        )
    };
    // The gateway deletes the cookie with its last answer.
    let deleting = response(
        200,
        &["Set-Cookie: sid=gone; Path=/; Max-Age=0"],
        V21.as_bytes(),
    );
    let mut script = finish(authenticated()).then(CLIENT_RESOURCE, deleting);
    assert!(matches!(
        login(&mut session(), &mut script, Options::default()),
        Err(Error::NoSession)
    ));

    // The cookie is there and empty.
    let emptying = response(200, &["Set-Cookie: sid=; Path=/"], V21.as_bytes());
    let mut script = finish(authenticated()).then(CLIENT_RESOURCE, emptying);
    assert!(matches!(
        login(&mut session(), &mut script, Options::default()),
        Err(Error::NoSession)
    ));

    // No answer ever set one.
    let body = r#"{"code":0,"data":{"isLogin":1}}"#;
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then(AUTH_CONFIG, ok(body))
        .then(ONLINE_INFO, ok(r#"{"code":0,"data":{}}"#))
        .then(CLIENT_RESOURCE, ok(V21));
    assert!(matches!(
        login(&mut session(), &mut script, Options::default()),
        Err(Error::NoSession)
    ));
}

#[test]
fn an_exchange_that_fails_is_a_transport_error_wherever_it_happens() {
    let scripts = [
        Script::default()
            .then(MANIFEST, manifest())
            .then_fail(AUTH_CONFIG),
        Script::default()
            .then(MANIFEST, manifest())
            .then(AUTH_CONFIG, auth_config(""))
            .then_fail(PSW),
        authenticated().then_fail(AUTH_CHECK),
    ];
    for mut script in scripts {
        assert!(matches!(
            login(&mut session(), &mut script, Options::default()),
            Err(Error::Transport(_))
        ));
        assert!(script.is_finished());
    }
}

#[test]
fn a_second_stage_exchange_that_fails_is_a_failed_check_not_a_failed_login() {
    // The object of the contract tests, enabled, with no certificate named: the certificate
    // check fails first, so here the second stage is reached through a named certificate.
    let object = r#"{"enable":1,"antiMITMRequest":false,"devicePubKeyMod":"00C0FFEE5EED","devicePubKeyExp":"65537","rsaCert":"MAMCAQU=","sm2encCert":"","challenge":"example-challenge-0001","encryptedChallenge":"91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED","ticket":"TK-0001","mitmSig":"SIG"}"#;
    let key =
        sangfor_atrust_sig::SignKey::derive("00C0FFEE5EED", "65537", "example-challenge-0001")
            .unwrap();
    let canonical = sangfor_atrust_sig::canonical_string(object).unwrap();
    let object = object.replace("SIG", &key.sign(canonical.as_bytes()).unwrap());

    let config = auth_config(&format!(r#","isLogin":1,"antiMITMAttackData":{object}"#));
    let mut script = Script::default()
        .then(MANIFEST, manifest())
        .then_over(
            AUTH_CONFIG,
            config,
            vec![vec![0x30, 0x03, 0x02, 0x01, 0x05]],
        )
        .then_fail(ANTI_MITM)
        .then(
            ONLINE_INFO,
            ok(r#"{"code":0,"data":{"username":"user01"}}"#),
        )
        .then(CLIENT_RESOURCE, ok(V21));
    let login = login(&mut session(), &mut script, Options::default()).unwrap();
    assert_eq!(login.anti_mitm, AntiMitm::Failed(Check::SecondStage));
    assert!(script.is_finished());
}
