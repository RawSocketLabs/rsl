//! Shared blocking client authentication and first command reply.
// --- Standard library ---
use std::io::{Read, Write};

// --- Internal modules ---
use crate::v5::{
    AuthMethod, Command, Endpoint, MethodRequest, MethodSelection, Reply, Request as WireRequest,
    UsernamePasswordRequest, UsernamePasswordResponse,
};
use crate::{Stream, error::Error};

/// Authenticate, send the selected command, and validate its first reply.
pub(super) fn exchange<S: Read + Write>(
    stream: S,
    dest: Endpoint,
    command: Command,
    method: AuthMethod,
    credentials: Option<UsernamePasswordRequest>,
) -> Result<(Stream<S>, Endpoint), Error> {
    // Build both messages before any I/O.
    let offer = MethodRequest::builder().methods(vec![method]).build()?;
    let request = WireRequest::builder()
        .command(command)
        .destination(dest)
        .build()?;

    // Send the offer and receive the selected method.
    let mut stream = Stream::new(stream);
    stream.write_message(&offer)?;
    stream
        .read_message::<MethodSelection>()?
        .check_offered(method)?;

    // If there are credentials, send them and validate the response.
    if let Some(credentials) = credentials {
        stream.write_message(&credentials)?;
        stream
            .read_message::<UsernamePasswordResponse>()?
            .ensure_success()?;
    }

    // Send the command and validate its first reply.
    stream.write_message(&request)?;
    let response = stream.read_message::<Reply>()?;
    response.ensure_success()?;

    Ok((stream, response.bound))
}
