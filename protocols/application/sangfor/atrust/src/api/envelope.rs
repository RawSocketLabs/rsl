//! The wrapper every JSON answer comes in.

// --- Standard library ---
use std::str;

// --- Workspace dependencies ---
use http1::Response;
use serde::Deserialize;
use serde::de::{DeserializeOwned, Error as _};
use serde_json::value::RawValue;

// --- Internal modules ---
use super::Error;
use super::members::{from_object, or_default};

/// The codes with which a gateway says the session is gone (spec §4.4).
const SESSION_INVALID: [i64; 2] = [10_000_004, 75_500_002];

/// The envelope of a JSON answer: `{"code":…,"message":…,"data":{…}}` (spec §4.3).
///
/// The envelope is read first and its payload only by the reader of the endpoint's own type,
/// because an error answer's `data` may be null or of another shape, and that must not hide
/// its code and message. Member names are matched exactly: `Code` is not `code`.
#[derive(Clone, Debug)]
pub struct Envelope<'a> {
    /// The result code: zero for success. Absent when the answer has none.
    pub code: Option<i64>,
    /// The gateway's message. Empty when the answer has none.
    pub message: String,
    /// The answer's payload, unparsed.
    data: Option<&'a RawValue>,
}

/// The envelope as it is on the wire.
#[derive(Deserialize)]
struct Wire<'a> {
    /// The result code.
    #[serde(default)]
    code: Option<i64>,
    /// The message.
    #[serde(default, deserialize_with = "or_default")]
    message: String,
    /// The payload.
    #[serde(default, borrow)]
    data: Option<&'a RawValue>,
}

impl<'a> Envelope<'a> {
    /// Read the envelope of `body`.
    ///
    /// # Errors
    ///
    /// [`Error::Json`] when `body` is not a JSON object, or `code` or `message` has the wrong
    /// type.
    pub fn parse(body: &'a [u8]) -> Result<Self, Error> {
        let text =
            str::from_utf8(body).map_err(|error| Error::Json(serde_json::Error::custom(error)))?;
        let wire: Wire<'a> = from_object(text).map_err(Error::Json)?;
        Ok(Self {
            code: wire.code,
            message: wire.message,
            data: wire.data,
        })
    }

    /// Read the envelope of `response`, which must have status 200 (spec §4.3 notes: E1, E2
    /// and E3).
    ///
    /// # Errors
    ///
    /// [`Error::Status`] for any other status, and as [`parse`](Self::parse).
    pub fn of_ok(response: &'a Response) -> Result<Self, Error> {
        match response.head.status {
            200 => Self::parse(&response.body),
            status => Err(Error::Status(status)),
        }
    }

    /// Succeed when the gateway accepted the request: the code is zero or absent.
    ///
    /// # Errors
    ///
    /// [`Error::SessionInvalid`] for the two codes that mean the session is gone, and
    /// [`Error::Refused`] for any other code.
    pub fn accepted(&self) -> Result<(), Error> {
        match self.code {
            None | Some(0) => Ok(()),
            Some(code) if SESSION_INVALID.contains(&code) => Err(Error::SessionInvalid { code }),
            Some(code) => Err(Error::Refused {
                code,
                message: self.message.clone(),
            }),
        }
    }

    /// The payload as a `T`. A missing or null payload is `T`'s default, so an answer with
    /// no `data` reads as one with every member absent.
    pub(super) fn data<T: DeserializeOwned + Default>(&self) -> Result<T, Error> {
        self.data
            .map_or_else(|| Ok(T::default()), |data| from_object(data.get()))
            .map_err(Error::Json)
    }

    /// The payload's JSON text, when there is one.
    pub(super) fn raw_data(&self) -> Option<&'a str> {
        self.data.map(RawValue::get)
    }
}
