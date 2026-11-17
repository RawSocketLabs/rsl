//! The anti-MITM object of `authConfig`, and the answer to the second-stage request (E3).

// --- Workspace dependencies ---
use http1::Response;
use serde::Deserialize;

// --- Internal modules ---
use super::members::{from_object, or_default};
use super::{Envelope, Error};

/// The `antiMITMAttackData` object of an `authConfig` answer (spec §3.2).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[non_exhaustive]
pub struct AntiMitmData {
    /// The object's JSON text exactly as received: what its signature covers.
    #[serde(skip)]
    pub raw: String,
    /// One when the certificate-identity check is to run.
    #[serde(deserialize_with = "or_default")]
    pub enable: i64,
    /// Text mixed into the sign key and the challenge key.
    #[serde(deserialize_with = "or_default")]
    pub device_pub_key_mod: String,
    /// Text mixed into the sign key and the challenge key.
    #[serde(deserialize_with = "or_default")]
    pub device_pub_key_exp: String,
    /// The expected RSA certificate, base64 of its DER, or empty.
    #[serde(deserialize_with = "or_default")]
    pub rsa_cert: String,
    /// The expected SM2 encryption certificate, base64, or empty.
    #[serde(rename = "sm2encCert", deserialize_with = "or_default")]
    pub sm2_enc_cert: String,
    /// The challenge text.
    #[serde(deserialize_with = "or_default")]
    pub challenge: String,
    /// The challenge encrypted, in uppercase hex.
    #[serde(deserialize_with = "or_default")]
    pub encrypted_challenge: String,
    /// The ticket the second-stage request echoes.
    #[serde(deserialize_with = "or_default")]
    pub ticket: String,
    /// When false, and the check is enabled, the client makes the second-stage request.
    #[serde(rename = "antiMITMRequest", deserialize_with = "or_default")]
    pub anti_mitm_request: bool,
    /// The signature over the object's canonical string, in hex.
    #[serde(deserialize_with = "or_default")]
    pub mitm_sig: String,
}

impl AntiMitmData {
    /// Read the object from its JSON text. `None` when it is null or has no members: the
    /// answer then counts as having no such object (spec §5.3).
    ///
    /// # Errors
    ///
    /// [`Error::Json`] when it is not an object, or a member has the wrong type.
    pub fn parse(raw: &str) -> Result<Option<Self>, Error> {
        if raw.trim() == "null" {
            return Ok(None);
        }
        let data: Self = from_object(raw).map_err(Error::Json)?;
        // Valid JSON by now, so an object with nothing between its braces has no members.
        let inside = raw.trim().trim_start_matches('{').trim_end_matches('}');
        if inside.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            raw: raw.to_owned(),
            ..data
        }))
    }
}

/// The answer to the second-stage anti-MITM request, as far as its check reads it
/// (spec §3.6). The answer's `antiMITMEnable` member plays no part in the check and is not
/// kept.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct AntiMitmReply<'a> {
    /// The gateway's code, when it gave one. The check, not this reader, judges it.
    pub code: Option<i64>,
    /// The nonce the gateway echoes.
    pub nonce: String,
    /// The value of the `X-Response-Sig` header, empty when there is none.
    pub signature: String,
    /// The body exactly as received: what the signature covers.
    pub body: &'a [u8],
}

impl<'a> AntiMitmReply<'a> {
    /// Read the answer. It must have status 200 (spec §4.3 notes); its code is not judged
    /// here.
    ///
    /// # Errors
    ///
    /// [`Error::Status`] for another status, and [`Error::Json`] when the body is not the
    /// JSON this endpoint returns. Either fails the second-stage check.
    pub fn parse(response: &'a Response) -> Result<Self, Error> {
        /// The `data` member.
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Data {
            /// The echoed nonce.
            #[serde(deserialize_with = "or_default")]
            nonce: String,
        }
        let envelope = Envelope::of_ok(response)?;
        let data: Data = envelope.data()?;
        let signature = response
            .head
            .headers
            .get("X-Response-Sig")
            .unwrap_or_default();
        Ok(Self {
            code: envelope.code,
            nonce: data.nonce,
            signature: String::from_utf8_lossy(signature).into_owned(),
            body: &response.body,
        })
    }
}
