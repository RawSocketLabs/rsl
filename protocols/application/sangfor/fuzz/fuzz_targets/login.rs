//! A password login (spec §5) against a gateway that answers whatever it likes: the login
//! must never panic, must stop within its bound of requests, and must keep what it promises
//! its caller: a session it reports is the one the cookie jar holds, a strict login never
//! succeeds past a failed anti-MITM check, and after such a failure nothing more was sent and
//! the jar is what it was before.
//!
//! The first byte picks what the gateway's `authConfig` carries, whether the login is strict
//! and whether the jar starts with a cookie. The rest is one answer per request, separated by `0xff`: an empty answer is the
//! valid one for the request that came, so the input only has to say where a login goes
//! wrong; any other is a selector byte, an optional header line, a line feed, and a body.
//!
//! Run: `cargo +nightly fuzz run login --fuzz-dir protocols/application/sangfor/fuzz`.
#![no_main]

use std::io;
use std::sync::LazyLock;

use http1::{Request, Response};
use libfuzzer_sys::fuzz_target;
use sangfor_atrust::api::{Gateway, Portal, Reply};
use sangfor_atrust::login::{AntiMitm, Credentials, Device, Error, Options, Session};
use sangfor_atrust_sig::{SignKey, canonical_string};
use sangfor_fuzz::{Counting, response};

const STATUSES: [u16; 4] = [200, 401, 403, 500];
const SEPARATOR: u8 = 0xff;
/// The requests a login makes once each (manifest, `authConfig`, anti-MITM second stage,
/// password, `reportEnv`, `onlineInfo`, `clientResource`); `authCheck` is the only one it
/// repeats, at most `Options::max_steps` times.
const SINGLE_REQUESTS: usize = 7;

/// The time of vector V4, as seconds since the epoch: the seed of its nonce.
const NOW: i64 = 20_261_116;
/// The nonce of vector V4, and the signature of vector V6 over the answer that carries it.
const NONCE: &str = "8A5BF19A11717BF9A74BB450E2EEC26E882AC7339744AA3505344A94E544E7B8";
const V6_SIGNATURE: &[u8] =
    b"X-Response-Sig: EE78499E113699CC1D5326246C2C260268E2F22D1E63A27EC9054A5721FD809A";
/// The stand-in certificate of vector V5; `MAMCAQU=` is its base64.
const CERTIFICATE: [u8; 5] = [0x30, 0x03, 0x02, 0x01, 0x05];
/// A throwaway 1024-bit RSA modulus for the gateway to announce.
const MODULUS: &str = concat!(
    "c27a17863e6058c010396eb0867f93dc51753e09ba1b87bed5d3f76c16c6f5adb5974f49d93b8452",
    "3df495898e5734b6d750ccdd4be725153111265c57c854ff8d679658bb467aeee4ffcb3643219392",
    "f44bc1c80d90528ae7ac11654be448934a49008136c8ccf34625fca525955de6bec05da1a67c24b6",
    "f9164a062be8f431",
);
/// The policy document of vector V21, without its second application.
const POLICY: &str = r#"{"data":{"appList":{"data":{
  "appInfo":[{"apps":[
   {"id":"app-0001","nodeGroupId":"ng-1","accessModel":"L3VPN","enableTCPPrefL3":true,
    "addressList":[{"protocol":"all","port":"1-65535","host":"198.51.100.0/24"}]}]}],
  "config":{"nodeGroupConf":{"majorNodeGroup":{"id":"ng-1"},
   "nodeGroupList":[{"id":"ng-1","addressInfo":[{"address":"{{sdpcHost}}","type":"WAN"}]}]}}}}}}"#;

/// The anti-MITM object of vector V3 naming the certificate of V5, signed with the key the
/// object itself establishes: one whose every check passes over a connection that presents
/// that certificate.
static ANTI_MITM: LazyLock<String> = LazyLock::new(|| {
    let unsigned = concat!(
        r#"{"enable":1,"antiMITMRequest":false,"devicePubKeyMod":"00C0FFEE5EED","#,
        r#""devicePubKeyExp":"65537","rsaCert":"MAMCAQU=","sm2encCert":"","#,
        r#""challenge":"example-challenge-0001","ticket":"TK-0001","encryptedChallenge":"#,
        r#""91F62C3D0672E10A14CF23E7C5F18EAEF19820E405852A2DF51DF478DD90B0ED"}"#,
    );
    let key = SignKey::derive("00C0FFEE5EED", "65537", "example-challenge-0001").unwrap();
    let signature = key
        .sign(canonical_string(unsigned).unwrap().as_bytes())
        .unwrap();
    format!(
        r#"{},"mitmSig":"{signature}"}}"#,
        unsigned.strip_suffix('}').unwrap()
    )
});

/// A gateway that answers each request with the next answer of the input.
struct Scripted<'a> {
    answers: std::slice::Split<'a, u8, fn(&u8) -> bool>,
    /// What the valid `authConfig` answer carries besides a password login.
    config_extra: &'a str,
    /// The path of each request that came, in order.
    paths: Vec<Vec<u8>>,
}

impl Scripted<'_> {
    /// The answer a well-behaved gateway gives to a request for `path`.
    fn valid(&self, path: &[u8]) -> Option<Response> {
        const EARLY: &[u8] = b"Set-Cookie: sid=S1D-early; Path=/";
        const SID: &[u8] = b"Set-Cookie: sid=S1D-example; Path=/";
        let ok = |body: &str| response(200, &[], body.as_bytes());
        match path {
            b"/public/manifest" => ok(
                r#"{"code":0,"data":{"capacities":{"zeroRTT":{"version":"2.0"}},"options":{"tun0rtt":{"enable":true}}}}"#,
            ),
            b"/passport/v1/public/authConfig" => {
                let body = format!(
                    concat!(
                        r#"{{"code":0,"data":{{"csrfToken":"csrf-1","pubKey":"{}","pubKeyExp":"65537","#,
                        r#""antiReplayRand":"r1","authServerInfoList":[{{"loginDomain":"local","#,
                        r#""authId":"1","authType":"auth/psw"}}]{}}}}}"#,
                    ),
                    MODULUS, self.config_extra
                );
                response(200, &[EARLY], body.as_bytes())
            }
            b"/controller/v1/public/antiMITMRequest" => {
                let body =
                    format!(r#"{{"code":0,"data":{{"nonce":"{NONCE}","antiMITMEnable":1}}}}"#);
                response(200, &[V6_SIGNATURE], body.as_bytes())
            }
            b"/passport/v1/auth/psw" => response(
                200,
                &[SID],
                br#"{"code":0,"data":{"ticket":"TK-0001","nextService":""}}"#,
            ),
            b"/passport/v1/auth/authCheck" => {
                ok(r#"{"code":0,"data":{"nextService":"","nextServiceList":[]}}"#)
            }
            b"/passport/v1/user/onlineInfo" => ok(r#"{"code":0,"data":{"username":"user01"}}"#),
            b"/controller/v1/user/clientResource" => ok(POLICY),
            _ => ok(r#"{"code":0}"#),
        }
    }
}

impl Portal for Scripted<'_> {
    fn exchange(&mut self, request: &Request) -> io::Result<Reply> {
        let path = request.target.split(|&byte| byte == b'?').next().unwrap();
        self.paths.push(path.to_vec());
        let response = match self.answers.next().unwrap_or_default().split_first() {
            None => self.valid(path),
            Some((selector, _)) if selector & 4 != 0 => {
                return Err(io::ErrorKind::ConnectionReset.into());
            }
            Some((selector, answer)) => {
                let cut = answer.iter().position(|&byte| byte == b'\n');
                let (header, body) = match cut {
                    Some(cut) => (&answer[..cut], &answer[cut + 1..]),
                    None => (&[][..], answer),
                };
                let headers = if header.is_empty() {
                    vec![]
                } else {
                    vec![header]
                };
                response(STATUSES[usize::from(selector & 3)], &headers, body)
            }
        };
        // An answer HTTP cannot carry is a connection that failed.
        let response = response.ok_or(io::ErrorKind::InvalidData)?;
        Ok(Reply {
            response,
            peer_chain: vec![CERTIFICATE.to_vec()],
        })
    }
}

fuzz_target!(|data: &[u8]| {
    let (selector, answers) = data
        .split_first()
        .map_or((0, data), |(byte, rest)| (*byte, rest));
    let with_session = format!(r#","isLogin":1,"antiMITMAttackData":{}"#, *ANTI_MITM);
    let without_session = format!(r#","antiMITMAttackData":{}"#, *ANTI_MITM);
    let config_extra = ["", &with_session, &without_session, ""][usize::from(selector & 3)];
    let mut options = Options::default();
    options.strict_anti_mitm = selector & 4 != 0;

    let device = Device {
        id: "00112233445566778899aabbccddeeff".to_owned(),
        mac_addresses: vec!["02:00:5e:00:53:01".to_owned()],
    };
    let mut session = Session::new(Gateway::new("sdp.example.org", 8443), device);
    if selector & 8 != 0 {
        // A cookie from before the login, so that an emptied jar is not a restored one.
        session.cookies_mut().store(b"kept=1; Path=/", b"/", NOW);
    }
    let cookies_before = session.cookies().clone();
    let credentials = Credentials {
        username: "user01",
        password: "pw",
        domain: "local",
    };
    let mut gateway = Scripted {
        answers: answers.split(|&byte| byte == SEPARATOR),
        config_extra,
        paths: Vec::new(),
    };
    let result = session.login(
        &mut gateway,
        &credentials,
        options,
        &mut Counting::default(),
        NOW,
    );

    assert!(
        gateway.paths.len() <= SINGLE_REQUESTS + options.max_steps,
        "a login made more requests than its bound"
    );
    match &result {
        Ok(login) => {
            assert_eq!(
                session.sid(NOW),
                Some(login.sid.as_str()),
                "the session reported is not the one the jar holds"
            );
            assert!(
                !(options.strict_anti_mitm && matches!(login.anti_mitm, AntiMitm::Failed(_))),
                "a strict login succeeded past a failed check"
            );
        }
        Err(Error::AntiMitm(_)) => {
            assert!(
                options.strict_anti_mitm,
                "only a strict login fails on a check"
            );
            assert_eq!(
                session.cookies(),
                &cookies_before,
                "a failing gateway's cookies were kept"
            );
            // The check is made on `authConfig` and its second stage; nothing follows it.
            let before_the_check: [&[u8]; 3] = [
                b"/public/manifest",
                b"/passport/v1/public/authConfig",
                b"/controller/v1/public/antiMITMRequest",
            ];
            assert!(
                gateway
                    .paths
                    .iter()
                    .all(|path| before_the_check.contains(&path.as_slice())),
                "a strict login sent a request after a failed check"
            );
        }
        Err(_) => {}
    }
    // A gateway that answers every request properly is logged in to, with every check passed.
    if answers.iter().all(|&byte| byte == SEPARATOR) {
        let expected = if config_extra.is_empty() {
            AntiMitm::Absent
        } else {
            AntiMitm::Passed
        };
        assert_eq!(result.unwrap().anti_mitm, expected);
    }
});
