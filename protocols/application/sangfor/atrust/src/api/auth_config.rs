//! The answer to `authConfig` (E2).

// --- Workspace dependencies ---
use http1::Response;
use serde::Deserialize;
use serde_json::value::RawValue;

// --- Internal modules ---
use super::members::{object, objects, or_default};
use super::{AntiMitmData, Envelope, Error};

/// What an `authConfig` answer holds (spec §4.3 E2, §5.2). Every string is stored as
/// received; a missing member is empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct AuthConfig {
    /// The CSRF token for the requests that follow: `data.csrfToken`, or
    /// `data.security.csrfToken` when that is empty.
    pub csrf_token: String,
    /// The modulus of the RSA key secrets are encrypted under, in hex.
    pub pub_key: String,
    /// That key's exponent, in decimal.
    pub pub_key_exp: String,
    /// The value mixed into the next encrypted secret.
    pub anti_replay_rand: String,
    /// Whether the cookies sent already belong to a logged-in session.
    pub is_login: bool,
    /// The ways to log in that the gateway offers.
    pub servers: Vec<AuthServer>,
    /// The anti-MITM object, with its text exactly as received, when the answer has one that
    /// is not empty.
    pub anti_mitm: Option<AntiMitmData>,
}

/// One way to log in: an entry of `authServerInfoList`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct AuthServer {
    /// The login domain the entry serves.
    #[serde(deserialize_with = "or_default")]
    pub login_domain: String,
    /// The entry's id.
    #[serde(deserialize_with = "or_default")]
    pub auth_id: String,
    /// The method, such as `auth/psw`.
    #[serde(deserialize_with = "or_default")]
    pub auth_type: String,
    /// The name shown to a user.
    #[serde(deserialize_with = "or_default")]
    pub auth_name: String,
    /// A refinement of the method.
    #[serde(deserialize_with = "or_default")]
    pub sub_type: String,
    /// Where an external login starts.
    #[serde(deserialize_with = "or_default")]
    pub login_url: String,
}

/// The `data` member of an `authConfig` answer, as it is on the wire, but for the anti-MITM
/// object, which is read from the payload's text separately.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Data {
    /// The CSRF token, in its first place.
    #[serde(deserialize_with = "or_default")]
    csrf_token: String,
    /// The object holding the CSRF token's second place.
    #[serde(deserialize_with = "object")]
    security: Security,
    /// The RSA modulus.
    #[serde(deserialize_with = "or_default")]
    pub_key: String,
    /// The RSA exponent.
    #[serde(deserialize_with = "or_default")]
    pub_key_exp: String,
    /// The anti-replay value.
    #[serde(deserialize_with = "or_default")]
    anti_replay_rand: String,
    /// One when a session already exists.
    #[serde(deserialize_with = "or_default")]
    is_login: i64,
    /// The login methods.
    #[serde(deserialize_with = "objects")]
    auth_server_info_list: Vec<AuthServer>,
}

/// The `security` member.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Security {
    /// The CSRF token, in its second place.
    #[serde(deserialize_with = "or_default")]
    csrf_token: String,
}

/// The one member of the payload whose text must survive.
#[derive(Default, Deserialize)]
struct Raw<'a> {
    /// The anti-MITM object, unparsed.
    #[serde(default, borrow, rename = "antiMITMAttackData")]
    anti_mitm: Option<&'a RawValue>,
}

impl AuthConfig {
    /// Read an `authConfig` answer under the rules of a login (spec §4.3 notes, §5.2): the
    /// status must be 200, and a code that is present must be zero. An answer with no code is
    /// accepted.
    ///
    /// # Errors
    ///
    /// [`Error::SessionInvalid`] for status 401 or 403 and for the codes that mean the
    /// session is gone; [`Error::Status`] for any other status than 200; [`Error::Refused`]
    /// for any other code; [`Error::Json`] when the body is not the JSON this endpoint
    /// returns.
    pub fn parse(response: &Response) -> Result<Self, Error> {
        let envelope = match Envelope::of_ok(response) {
            Err(Error::Status(status @ (401 | 403))) => {
                return Err(Error::SessionInvalid {
                    code: status.into(),
                });
            }
            envelope => envelope?,
        };
        envelope.accepted()?;

        let data: Data = envelope.data()?;
        let raw: Raw<'_> = envelope
            .raw_data()
            .map(serde_json::from_str)
            .transpose()
            .map_err(Error::Json)?
            .unwrap_or_default();
        let anti_mitm = raw
            .anti_mitm
            .map(|raw| AntiMitmData::parse(raw.get()))
            .transpose()?
            .flatten();
        let csrf_token = if data.csrf_token.is_empty() {
            data.security.csrf_token
        } else {
            data.csrf_token
        };
        Ok(Self {
            csrf_token,
            pub_key: data.pub_key,
            pub_key_exp: data.pub_key_exp,
            anti_replay_rand: data.anti_replay_rand,
            is_login: data.is_login == 1,
            servers: data.auth_server_info_list,
            anti_mitm,
        })
    }

    /// The first entry that offers `auth_type` for `login_domain`, both compared exactly
    /// (spec §5.2).
    #[must_use]
    pub fn server(&self, auth_type: &str, login_domain: &str) -> Option<&AuthServer> {
        self.servers
            .iter()
            .find(|server| server.auth_type == auth_type && server.login_domain == login_domain)
    }
}
