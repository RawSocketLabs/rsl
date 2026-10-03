//! Shared blocking client authentication and first command reply.
// --- Standard library ---
use std::io::{self, Read, Write};

// --- Internal modules ---
use crate::io::stream::MAX_FRAME_LEN;
use crate::v5::sansio::{Client, Step};
use crate::v5::{AuthMethod, Command, Endpoint, UsernamePasswordRequest};
use crate::{Stream, error::Error};

/// Authenticate, send the selected command, and validate its first reply.
pub(super) fn exchange<S: Read + Write>(
    mut stream: S,
    dest: Endpoint,
    command: Command,
    method: AuthMethod,
    credentials: Option<UsernamePasswordRequest>,
) -> Result<(Stream<S>, Endpoint), Error> {
    let mut client = Client::prepared(dest, command, method, credentials)?;
    let mut scratch = [0; MAX_FRAME_LEN];
    loop {
        match client.advance()? {
            Step::Transmit => {
                let pending = client.transmit();
                stream.write_all(pending)?;
                let count = pending.len();
                client.sent(count);
                stream.flush()?;
            }
            Step::Receive => {
                let limit = client.receive_limit();
                match read(&mut stream, &mut scratch[..limit])? {
                    0 => client.receive_eof(),
                    count => client.receive(&scratch[..count])?,
                }
            }
            Step::Established => break,
        }
    }
    let (input, bound) = client.finish()?;
    Ok((Stream::from_parts(stream, input), bound))
}

/// Read once, retrying interruptions.
fn read<S: Read>(stream: &mut S, bytes: &mut [u8]) -> io::Result<usize> {
    loop {
        match stream.read(bytes) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}
