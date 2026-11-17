//! What several tests script a gateway with: the documents and answers of the
//! specification's vectors, and a session to log in with.

use http1::Response;
use sangfor_atrust::api::Gateway;
use sangfor_atrust::login::{Credentials, Device, Error, Login, Options, Session};

use super::{Counting, Script, ok, response};

pub(crate) const MANIFEST: &str = "/public/manifest";
pub(crate) const AUTH_CONFIG: &str = "/passport/v1/public/authConfig";
pub(crate) const ANTI_MITM: &str = "/controller/v1/public/antiMITMRequest";
pub(crate) const PSW: &str = "/passport/v1/auth/psw";
pub(crate) const REPORT_ENV: &str = "/controller/v1/public/reportEnv";
pub(crate) const AUTH_CHECK: &str = "/passport/v1/auth/authCheck";
pub(crate) const ONLINE_INFO: &str = "/passport/v1/user/onlineInfo";
pub(crate) const CLIENT_RESOURCE: &str = "/controller/v1/user/clientResource";

/// The shared device id of the specification's vectors.
pub(crate) const DEVICE_ID: &str = "00112233445566778899aabbccddeeff";
/// A throwaway 1024-bit RSA key: the modulus a scripted gateway announces, and the private
/// exponent with which a test reads what was encrypted for it.
pub(crate) const MODULUS: &str = concat!(
    "c27a17863e6058c010396eb0867f93dc51753e09ba1b87bed5d3f76c16c6f5adb5974f49d93b8452",
    "3df495898e5734b6d750ccdd4be725153111265c57c854ff8d679658bb467aeee4ffcb3643219392",
    "f44bc1c80d90528ae7ac11654be448934a49008136c8ccf34625fca525955de6bec05da1a67c24b6",
    "f9164a062be8f431",
);
/// The time of vector V4, as seconds since the epoch: the seed of its nonce.
pub(crate) const NOW: i64 = 20_261_116;
/// The policy document of vector V21.
pub(crate) const V21: &str = r#"{"data":{
 "appList":{"data":{
  "appInfo":[{"apps":[
   {"id":"app-0001","nodeGroupId":"ng-1","accessModel":"L3VPN","enableTCPPrefL3":true,
    "addressList":[{"protocol":"all","port":"1-65535","host":"198.51.100.0/24"}]},
   {"id":"app-0002","nodeGroupId":"ng-2","accessModel":"L3VPN","addrPretend":false,
    "addressList":[{"protocol":"tcp","port":"443","host":"app.example.net","ip":["203.0.113.5"]},
                   {"protocol":"udp","port":"5000-5010","host":"*.example.net"}]},
   {"id":"app-0003","nodeGroupId":"ng-1","accessModel":"WEB",
    "addressList":[{"protocol":"tcp","port":"80","host":"203.0.113.9"}]}]}],
  "config":{"nodeGroupConf":{
   "majorNodeGroup":{"id":"ng-1"},
   "nodeGroupList":[
    {"id":"ng-1","addressInfo":[{"address":"{{sdpcHost}}","type":"WAN"},
                                {"address":"198.51.100.200:4441","type":"lan"}]},
    {"id":"ng-2","addressInfo":[{"address":"node2.example.org:441","type":"wan"}]}]}}}},
 "sdpPolicy":{"data":{"clientOption":{
  "dnsOption":{"firstDNS":"","secondDNS":""},
  "dnsOptionV2":{"firstDNS":"192.0.2.53","secondDNS":"192.0.2.54"}}}}}}"#;

pub(crate) fn session() -> Session {
    let device = Device {
        id: DEVICE_ID.to_owned(),
        mac_addresses: vec!["02:00:5e:00:53:01".to_owned()],
    };
    Session::new(Gateway::new("sdp.example.org", 8443), device)
}

pub(crate) const CREDENTIALS: Credentials<'static> = Credentials {
    username: "user01",
    password: "pw",
    domain: "local",
};

pub(crate) fn login(
    session: &mut Session,
    script: &mut Script,
    options: Options,
) -> Result<Login, Error> {
    session.login(script, &CREDENTIALS, options, &mut Counting::default(), NOW)
}

/// The manifest of vector V18.
pub(crate) fn manifest() -> Response {
    ok(
        r#"{"code":0,"data":{"capacities":{"zeroRTT":{"version":"2.0"}},"options":{"tun0rtt":{"enable":true}}}}"#,
    )
}

/// An `authConfig` answer offering a password login for the domain `local`, with `extra`
/// members in its payload.
pub(crate) fn auth_config(extra: &str) -> Response {
    let body = format!(
        r#"{{"code":0,"data":{{"csrfToken":"csrf-1","pubKey":"{MODULUS}","pubKeyExp":"65537","antiReplayRand":"r1","authServerInfoList":[{{"loginDomain":"local","authId":"1","authType":"auth/psw"}}]{extra}}}}}"#
    );
    response(200, &["Set-Cookie: sid=S1D-early; Path=/"], body.as_bytes())
}

/// The answer to a password the gateway accepts, naming `next_service`.
pub(crate) fn psw_accepted(next_service: &str) -> Response {
    let body =
        format!(r#"{{"code":0,"data":{{"ticket":"TK-0001","nextService":"{next_service}"}}}}"#);
    response(
        200,
        &["Set-Cookie: sid=S1D-example; Path=/"],
        body.as_bytes(),
    )
}
