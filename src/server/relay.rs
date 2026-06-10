use std::io;
use std::net::{Shutdown, TcpStream};
use std::thread;

use crate::error::{Result, SocksError};

/// Bidirectionally copies data between two streams until both directions
/// reach end-of-file, propagating half-closes so each side terminates.
pub(crate) fn relay(client: TcpStream, remote: TcpStream) -> Result<()> {
    let mut client_read = client.try_clone()?;
    let mut remote_write = remote.try_clone()?;
    let mut remote_read = remote;
    let mut client_write = client;

    let upstream = thread::spawn(move || {
        let _ = io::copy(&mut client_read, &mut remote_write);
        let _ = remote_write.shutdown(Shutdown::Write);
    });

    let _ = io::copy(&mut remote_read, &mut client_write);
    let _ = client_write.shutdown(Shutdown::Write);

    upstream
        .join()
        .map_err(|_| SocksError::Io(io::Error::other("relay thread panicked")))
}
