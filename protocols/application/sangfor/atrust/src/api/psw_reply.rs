//! The answer to a password login (E5).

// --- Workspace dependencies ---
use http1::Response;
use serde::Deserialize;

// --- Internal modules ---
use super::members::or_default;
use super::{Envelope, Error};

/// An `auth/psw` answer (spec §4.3 E5, §5.4). Its payload is read whatever the code, because
/// a refusal may be a demand for a captcha.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct PswReply {
    /// The gateway's code: zero when it accepted the password, and when it gave none.
    pub code: i64,
    /// The gateway's message.
    pub message: String,
    /// The ticket of the login. Empty on a refusal.
    pub ticket: String,
    /// Whether the gateway wants a click captcha solved before it acts on the request.
    pub wants_captcha: bool,
    /// The service the secondary chain starts with, taken literally; empty for the default.
    pub next_service: String,
    /// A fresh anti-replay value, when the gateway issued one.
    pub anti_replay_rand: String,
}

impl PswReply {
    /// Read an `auth/psw` answer. The HTTP status is not looked at (spec §4.3 notes).
    ///
    /// # Errors
    ///
    /// [`Error::Json`] when the body is not the JSON this endpoint returns.
    pub fn parse(response: &Response) -> Result<Self, Error> {
        /// The `data` member.
        #[derive(Default, Deserialize)]
        #[serde(default, rename_all = "camelCase")]
        struct Data {
            /// The ticket.
            #[serde(deserialize_with = "or_default")]
            ticket: String,
            /// One when a captcha is demanded.
            #[serde(deserialize_with = "or_default")]
            graph_check_code_enable: i64,
            /// The first service of the secondary chain.
            #[serde(deserialize_with = "or_default")]
            next_service: String,
            /// A fresh anti-replay value.
            #[serde(deserialize_with = "or_default")]
            anti_replay_rand: String,
        }
        let envelope = Envelope::parse(&response.body)?;
        // A refusal's payload may have any shape; only a demand for a captcha is read from it.
        let data: Data = match envelope.accepted() {
            Ok(()) => envelope.data()?,
            Err(_) => envelope.data().unwrap_or_default(),
        };
        Ok(Self {
            code: envelope.code.unwrap_or(0),
            message: envelope.message,
            ticket: data.ticket,
            wants_captcha: data.graph_check_code_enable == 1,
            next_service: data.next_service,
            anti_replay_rand: data.anti_replay_rand,
        })
    }
}
