//! One request to the control plane, before the session's values are added.

// --- Standard library ---
use std::collections::BTreeMap;

// --- Workspace dependencies ---
use http1::Request;
use sangfor_atrust_sig::identifiers;
use serde::Serialize;
use uri::form::Encoding;

// --- Internal modules ---
use super::{Context, Error};

/// The `User-Agent` every request carries (spec §4.2).
const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
     aTrustTray/2.4.10.50 Chrome/83.0.4103.94 Electron/9.0.2 Safari/537.36 \
     aTrustTray-Linux-Plat-Ubuntu-x64 SPCClientType";
/// The query parameters every `/passport/` and `/controller/` request carries (spec §4.1).
const COMMON_QUERY: [(&str, &str); 3] = [
    ("clientType", "SDPClient"),
    ("lang", "en-US"),
    ("platform", "Linux"),
];
/// The content type of a JSON body.
const JSON: &str = "application/json;charset=utf-8";
/// The content type of the one JSON body that is sent without a character set.
const BARE_JSON: &str = "application/json";
/// The body that asks for every part of the client's resources (spec §4.3, E23).
const CLIENT_RESOURCE_BODY: &str = r#"{"resourceType":{"appList":{},"favoriteAppList":{},"featureCenter":{},"sdpPolicy":{},"uemSpace":{"params":{"action":"login"}}}}"#;

/// One request of the endpoint catalogue (spec §4.3), holding what is particular to it.
/// [`request`](Self::request) adds what the session supplies and yields the HTTP request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The path, without a query.
    path: &'static str,
    /// The endpoint's own query parameters.
    query: Vec<(&'static str, String)>,
    /// Whether the common query parameters are added.
    common_query: bool,
    /// The content type and bytes of a `POST` body. Without one the request is a `GET`.
    body: Option<(&'static str, Vec<u8>)>,
    /// Which optional headers the request carries.
    headers: Headers,
}

/// The headers a request carries beyond those every request has (spec §4.2).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Headers {
    /// None: the request carries nothing of the session.
    Bare,
    /// `x-csrf-token` and `x-sdp-traceid`, which every other kind carries too.
    Session,
    /// Those and `x-sdp-rid`.
    Rid,
    /// Those and `x-sdp-env`, with this value.
    Env(String),
    /// Those and `X-Request-Sig`.
    Signed,
}

impl Call {
    /// E1: the gateway's capabilities. It carries nothing of the session.
    #[must_use]
    pub const fn manifest() -> Self {
        Self {
            path: "/public/manifest",
            query: Vec::new(),
            common_query: false,
            body: None,
            headers: Headers::Bare,
        }
    }

    /// E2 as a login sends it: the login configuration, with a ticket asked for.
    #[must_use]
    pub fn auth_config() -> Self {
        Self {
            query: vec![("needTicket", "1".to_owned())],
            headers: Headers::Rid,
            ..Self::get("/passport/v1/public/authConfig")
        }
    }

    /// E3: the second-stage anti-MITM request, echoing `nonce` and the `ticket` of the
    /// `authConfig` answer under validation. `device_id` is sent when it is not empty.
    ///
    /// The body's white space is part of the protocol and is reproduced exactly. `nonce` and
    /// `ticket` go into it verbatim, unescaped, as the specification's byte template has
    /// them: a ticket holding a quote makes a body that is not the JSON it looks like. Only
    /// the gateway that issued the ticket receives it.
    #[must_use]
    pub fn anti_mitm(nonce: &str, ticket: &str, device_id: &str) -> Self {
        let body = format!(
            "\n            {{\n                \"nonce\": \"{nonce}\",\n                \"ticket\": \"{ticket}\"\n            }}\n        "
        );
        let query = match device_id {
            "" => Vec::new(),
            id => vec![("mobileId", id.to_owned())],
        };
        Self {
            query,
            body: Some((BARE_JSON, body.into_bytes())),
            headers: Headers::Rid,
            ..Self::get("/controller/v1/public/antiMITMRequest")
        }
    }

    /// E5: a password login for `username` (already qualified, `user@domain`), with the
    /// password encrypted as the protocol requires (spec §3.4). `device_id` goes in the
    /// `x-sdp-env` header.
    #[must_use]
    pub fn psw(username: &str, encrypted_password: &str, device_id: &str) -> Self {
        /// The body of E5, without a captcha answer.
        #[derive(Serialize)]
        struct Body<'a> {
            /// The encrypted password, lowercase hex.
            password: &'a str,
            /// Always `"0"`.
            #[serde(rename = "rememberPwd")]
            remember_pwd: &'a str,
            /// The qualified user name.
            username: &'a str,
        }
        let body = Body {
            password: encrypted_password,
            remember_pwd: "0",
            username,
        };
        Self {
            headers: Headers::Env(identifiers::env_header(device_id)),
            ..Self::post_json("/passport/v1/auth/psw", &body)
        }
    }

    /// E10: the environment report that follows a primary login, for the `ticket` it issued.
    /// The request is signed with the session's sign key.
    #[must_use]
    pub fn report_env(device_id: &str, mac_addresses: &[String], ticket: &str) -> Self {
        /// The body of E10. The order of members is the order signed and sent.
        #[derive(Serialize)]
        struct Body<'a> {
            /// The device id.
            #[serde(rename = "deviceId")]
            device_id: &'a str,
            /// The environment report.
            env: Env<'a>,
            /// The ticket of the primary login.
            ticket: &'a str,
        }
        /// The `env` member.
        #[derive(Serialize)]
        struct Env<'a> {
            /// The endpoint description.
            endpoint: Endpoint<'a>,
        }
        /// The `endpoint` member.
        #[derive(Serialize)]
        struct Endpoint<'a> {
            /// The kind of device: always a browser.
            device: Device,
            /// The device id, again.
            device_id: &'a str,
            /// The device's MAC addresses.
            mac_addresses: &'a [String],
        }
        /// The `device` member.
        #[derive(Serialize)]
        struct Device {
            /// Always `browser`.
            r#type: &'static str,
        }
        let body = Body {
            device_id,
            env: Env {
                endpoint: Endpoint {
                    device: Device { r#type: "browser" },
                    device_id,
                    mac_addresses,
                },
            },
            ticket,
        };
        Self {
            headers: Headers::Signed,
            ..Self::post_json("/controller/v1/public/reportEnv", &body)
        }
    }

    /// E11: asks which authentication step, if any, comes next.
    #[must_use]
    pub fn auth_check() -> Self {
        Self::get("/passport/v1/auth/authCheck")
    }

    /// E22: who the session belongs to.
    #[must_use]
    pub fn online_info() -> Self {
        Self::get("/passport/v1/user/onlineInfo")
    }

    /// E23: the client's resources, whose body is the policy document (spec §6).
    #[must_use]
    pub fn client_resource() -> Self {
        Self {
            body: Some((JSON, CLIENT_RESOURCE_BODY.into())),
            ..Self::get("/controller/v1/user/clientResource")
        }
    }

    /// The path the request goes to, without its query: what a cookie jar matches against.
    #[must_use]
    pub const fn path(&self) -> &'static str {
        self.path
    }

    /// The HTTP request for this call in the session `context` describes.
    ///
    /// # Errors
    ///
    /// [`Error::Request`] when a value cannot appear in a request line or header,
    /// [`Error::NoSignKey`] when the call is signed and `context` has no key, and
    /// [`Error::Sig`] when signing fails.
    pub fn request(&self, context: &Context<'_>) -> Result<Request, Error> {
        let query = self.query_string();
        let target = match query.as_str() {
            "" => self.path.to_owned(),
            query => format!("{}?{query}", self.path),
        };
        let host = context.gateway.base_host();
        let (request, body) = match &self.body {
            Some((content_type, body)) => (
                Request::post(&host, &target, content_type, body.clone()),
                body.as_slice(),
            ),
            None => (Request::get(&host, &target), &[][..]),
        };
        let mut request = request
            .and_then(|request| request.header("User-Agent", USER_AGENT))
            .map_err(Error::Request)?;

        let mut headers: Vec<(&str, String)> = Vec::new();
        if self.headers != Headers::Bare {
            headers.push(("x-csrf-token", context.csrf_token.to_owned()));
            headers.push(("x-sdp-traceid", context.trace_id.to_owned()));
        }
        match &self.headers {
            Headers::Bare | Headers::Session => {}
            Headers::Rid => headers.push(("x-sdp-rid", identifiers::rid(&host))),
            Headers::Env(value) => headers.push(("x-sdp-env", value.clone())),
            Headers::Signed => {
                // The signature covers the path, the query exactly as sent, and the body.
                let key = context.sign_key.ok_or(Error::NoSignKey)?;
                let signed = [self.path.as_bytes(), b"?", query.as_bytes(), body].concat();
                headers.push(("X-Request-Sig", key.sign(&signed)?));
            }
        }
        // `Request::header` refuses a value that could end its own line.
        for (name, value) in headers {
            request = request.header(name, &value).map_err(Error::Request)?;
        }
        // A bare call carries nothing of the session, cookies included: a login sends the
        // manifest request before it has any (spec §5.2).
        let cookie = context.cookie.filter(|_| self.headers != Headers::Bare);
        if let Some(cookie) = cookie {
            // Cookies may hold bytes that are not text, so they bypass `Request::header` and
            // its check is repeated here: a restored cookie is not a checked one.
            if cookie.iter().any(|byte| matches!(byte, b'\r' | b'\n' | 0)) {
                return Err(Error::Request(http1::Error::InvalidRequest(
                    "a cookie contains a carriage return, line feed or NUL",
                )));
            }
            request.headers.push("Cookie", cookie);
        }
        Ok(request)
    }

    /// A `GET` of `path` with the common query and the headers of a request in a session.
    const fn get(path: &'static str) -> Self {
        Self {
            path,
            query: Vec::new(),
            common_query: true,
            body: None,
            headers: Headers::Session,
        }
    }

    /// A `POST` of `body` as JSON to `path`, otherwise like [`get`](Self::get).
    fn post_json(path: &'static str, body: &impl Serialize) -> Self {
        let body = serde_json::to_vec(body)
            .expect("invariant: the bodies are structs of strings, which always serialise");
        Self {
            body: Some((JSON, body)),
            ..Self::get(path)
        }
    }

    /// The query string: the common parameters and the endpoint's own in one set, sorted by
    /// name, an endpoint's value replacing a common one of the same name (spec §2.4).
    fn query_string(&self) -> String {
        let common = COMMON_QUERY.iter().filter(|_| self.common_query);
        let mut parameters: BTreeMap<&str, &str> = common.copied().collect();
        parameters.extend(
            self.query
                .iter()
                .map(|(name, value)| (*name, value.as_str())),
        );
        let encoded = Encoding::QUERY.pairs(
            parameters
                .iter()
                .map(|(name, value)| (name.as_bytes(), value.as_bytes())),
        );
        String::from_utf8(encoded).expect("invariant: the form encoder emits ASCII only")
    }
}
