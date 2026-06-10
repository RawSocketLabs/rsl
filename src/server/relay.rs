use std::io;
use std::net::{Shutdown, TcpStream};
use std::thread;

use crate::error::{Result, SocksError};

/// Bidirectionally copies data between two streams until both directions
/// reach end-of-file, propagating half-closes so each side terminates.
pub(crate) fn relay(client: TcpStream, remote: TcpStream) -> Result<()> {
    tracing::debug!("relay started");
    let mut client_read = client.try_clone()?;
    let mut remote_write = remote.try_clone()?;
    let mut remote_read = remote;
    let mut client_write = client;

    let upstream = thread::spawn(move || {
        let bytes = io::copy(&mut client_read, &mut remote_write).unwrap_or(0);
        let _ = remote_write.shutdown(Shutdown::Write);
        bytes
    });

    let downstream = io::copy(&mut remote_read, &mut client_write).unwrap_or(0);
    let _ = client_write.shutdown(Shutdown::Write);

    match upstream.join() {
        Ok(upstream) => {
            tracing::debug!(upstream, downstream, "relay finished");
            Ok(())
        }
        Err(_) => {
            tracing::error!("relay thread panicked");
            Err(SocksError::Io(io::Error::other("relay thread panicked")))
        }
    }
}
