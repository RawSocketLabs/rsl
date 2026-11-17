//! Logging in (spec §5): the sequence of control-plane requests that turns a user name and
//! password into a session, a user identity and a policy.
//!
//! [`Session`] holds what the gateway hands out along the way (cookies, the CSRF token, the
//! key material) and [`Session::login`] runs the sequence over a
//! [`Portal`](crate::api::Portal). It is plain blocking code: each request is sent and
//! answered before the next is built.
//!
//! Covered so far: the password method, and a secondary chain made of `auth/authCheck` steps
//! only. A gateway that asks for a captcha, a one-time code or any other step is reported as
//! such, not worked around. A manifest that cannot be fetched fails the login: there is no
//! stored copy to fall back on yet (spec §11.1).

mod anti_mitm;
mod credentials;
mod device;
mod error;
mod options;
mod outcome;
mod sequence;
mod session;
mod step;

pub use anti_mitm::{AntiMitm, Check};
pub use credentials::Credentials;
pub use device::Device;
pub use error::Error;
pub use options::Options;
pub use outcome::Login;
pub use session::Session;
pub use step::Step;
