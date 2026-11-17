//! The control-plane API (spec §4): what each request to a gateway looks like, and what its
//! answer holds.
//!
//! Pure: a [`Call`] is turned into an [`http1::Request`] and a response body into a typed
//! value. Nothing here sends anything. The one I/O boundary is the [`Portal`] trait, which
//! carries a single exchange; the login sequence that drives it is not in this module.
//!
//! Covered so far: the requests of a password login, E1 to E3, E5, E10, E11, E22 and E23 of
//! the specification's endpoint catalogue.

mod anti_mitm;
mod auth_config;
mod call;
mod context;
mod envelope;
mod error;
mod gateway;
mod manifest;
mod members;
mod next_step;
mod online_info;
mod portal;
mod psw_reply;

pub use anti_mitm::{AntiMitmData, AntiMitmReply};
pub use auth_config::{AuthConfig, AuthServer};
pub use call::Call;
pub use context::Context;
pub use envelope::Envelope;
pub use error::Error;
pub use gateway::Gateway;
pub use manifest::Manifest;
pub use next_step::{NextEntry, NextStep};
pub use online_info::OnlineInfo;
pub use portal::{Portal, Reply};
pub use psw_reply::PswReply;
