//! Capabilities shared by the SOCKS5 server backends, separate from wire validity.
use crate::{
    error::Error,
    v5::{Command, Request},
};

pub(super) fn check_request(request: &Request) -> Result<(), Error> {
    request.check_header()?;
    if request.command != Command::Connect {
        return Err(Error::UnsupportedCommand(request.command));
    }
    request.destination.validate_connect_destination()
}
