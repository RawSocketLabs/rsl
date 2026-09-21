//! Readiness-driven TCP connection and deadline mechanics.

mod connector;
mod deadline;

pub use connector::Connector;
pub(crate) use deadline::{deadline, remaining};
