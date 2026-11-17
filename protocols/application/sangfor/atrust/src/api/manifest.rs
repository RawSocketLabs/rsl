//! The answer to the manifest request (E1).

// --- Workspace dependencies ---
use http1::Response;
use serde::Deserialize;

// --- Internal modules ---
use super::members::{object, or_default};
use super::{Envelope, Error};

/// A gateway's capabilities, as far as this client uses them (spec §11.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Manifest {
    /// Whether TCP tunnels use the framed, zero-RTT data phase.
    pub zero_rtt: bool,
}

impl Manifest {
    /// Read a manifest answer. It must have status 200 and a zero code.
    ///
    /// # Errors
    ///
    /// [`Error::Status`], [`Error::SessionInvalid`], [`Error::Refused`] or [`Error::Json`]:
    /// in every case the manifest was not obtained.
    pub fn parse(response: &Response) -> Result<Self, Error> {
        /// The `data` member.
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Data {
            /// What the gateway can do.
            #[serde(deserialize_with = "object")]
            capacities: Capacities,
            /// What the gateway has switched on.
            #[serde(deserialize_with = "object")]
            options: Options,
        }
        /// The `capacities` member.
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Capacities {
            /// The zero-RTT capability.
            #[serde(rename = "zeroRTT", deserialize_with = "object")]
            zero_rtt: Version,
        }
        /// A capability's version.
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Version {
            /// Empty when the gateway lacks the capability.
            #[serde(deserialize_with = "or_default")]
            version: String,
        }
        /// The `options` member.
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Options {
            /// The zero-RTT option.
            #[serde(deserialize_with = "object")]
            tun0rtt: Switch,
        }
        /// An option's switch.
        #[derive(Default, Deserialize)]
        #[serde(default)]
        struct Switch {
            /// Whether the option is on.
            #[serde(deserialize_with = "or_default")]
            enable: bool,
        }
        let envelope = Envelope::of_ok(response)?;
        envelope.accepted()?;
        let data: Data = envelope.data()?;
        Ok(Self {
            zero_rtt: !data.capacities.zero_rtt.version.is_empty() && data.options.tun0rtt.enable,
        })
    }
}
