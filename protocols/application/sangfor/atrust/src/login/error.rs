//! Why a login did not complete.

// --- Standard library ---
use std::io;

// --- Workspace dependencies ---
use thiserror::Error as ThisError;

// --- Internal modules ---
use super::{Check, Step};
use crate::{api, policy};

/// A login that did not complete.
#[derive(Debug, ThisError)]
#[non_exhaustive]
pub enum Error {
    /// A request could not be carried to the gateway and back.
    #[error("the exchange with the gateway failed")]
    Transport(#[source] io::Error),
    /// A request could not be built, or the gateway refused it or answered with something
    /// else than the endpoint returns.
    #[error(transparent)]
    Api(#[from] api::Error),
    /// A protocol primitive refused its input: most often the gateway's RSA key, which the
    /// password is encrypted under.
    #[error("a protocol primitive failed")]
    Sig(#[from] sangfor_atrust_sig::Error),
    /// An anti-MITM check failed, and the login was told to stop when one does.
    #[error("the anti-MITM check failed: {0:?}")]
    AntiMitm(Check),
    /// The gateway offers no password login for the domain.
    #[error("the gateway offers no password login for the domain {domain:?}")]
    NoLoginMethod {
        /// The login domain asked for.
        domain: String,
    },
    /// The gateway wants a click captcha solved first, which this client cannot do yet.
    #[error("the gateway demands a captcha")]
    CaptchaRequired,
    /// The gateway accepted the password and issued no ticket.
    #[error("the gateway issued no ticket")]
    NoTicket,
    /// The gateway asks for an authentication step this client cannot do yet.
    #[error("the gateway asks for the unsupported authentication step {:?}", .0.service)]
    UnsupportedStep(Step),
    /// The gateway kept naming further steps past the limit.
    #[error("the gateway named more authentication steps than the limit allows")]
    TooManySteps,
    /// The login went through, and the gateway set no usable session identifier.
    #[error("the gateway set no session identifier")]
    NoSession,
    /// The policy document could not be read.
    #[error(transparent)]
    Policy(#[from] policy::Error),
}
