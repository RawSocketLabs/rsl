//! Tokio SOCKS5 client handshakes. Generic transports have caller-managed deadlines.
// --- Standard library ---
use std::{io, net::SocketAddr, time::Duration};

// --- Workspace dependencies ---
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};

// --- Internal modules ---
use crate::io::stream::MAX_FRAME_LEN;
use crate::io::tokio::bounded;
use crate::v5::sansio::{Client, Step};
use crate::v5::{AuthMethod, Command, Endpoint, UsernamePasswordRequest};
use crate::{Stream, error::Error, v5::auth::ClientAuth};
/// Negotiate CONNECT and return a lossless tunnel and proxy-bound endpoint. Domains
/// go to the proxy unchanged, without local DNS. Prefetched bytes stay in the tunnel.
///
/// # Errors
/// Returns a terminal transport, codec, authentication, or peer-reply error.
/// Invalid local arguments fail before writing any handshake bytes.
pub async fn connect_with<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    dest: Endpoint,
    auth: ClientAuth<'_>,
) -> Result<(Stream<S>, Endpoint), Error> {
    dest.validate_destination()?;
    let credentials = auth.credentials()?;
    connect_prepared(stream, dest, auth.method(), credentials).await
}

async fn connect_prepared<S: AsyncRead + AsyncWrite + Unpin>(
    mut stream: S,
    dest: Endpoint,
    method: AuthMethod,
    credentials: Option<UsernamePasswordRequest>,
) -> Result<(Stream<S>, Endpoint), Error> {
    let mut client = Client::prepared(dest, Command::Connect, method, credentials)?;
    let mut scratch = [0; MAX_FRAME_LEN];
    loop {
        match client.advance()? {
            Step::Transmit => {
                let pending = client.transmit();
                stream.write_all(pending).await?;
                let count = pending.len();
                client.sent(count);
                stream.flush().await?;
            }
            Step::Receive => {
                let limit = client.receive_limit();
                match read(&mut stream, &mut scratch[..limit]).await? {
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
async fn read<S: AsyncRead + Unpin>(stream: &mut S, bytes: &mut [u8]) -> io::Result<usize> {
    loop {
        match stream.read(bytes).await {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            result => return result,
        }
    }
}

/// Dial a numeric proxy address and negotiate within one absolute timeout.
///
/// # Errors
/// Returns a terminal connection/session error, including an invalid timeout.
pub async fn connect(
    proxy: SocketAddr,
    dest: Endpoint,
    auth: ClientAuth<'_>,
    timeout: Duration,
) -> Result<(Stream<TcpStream>, Endpoint), Error> {
    dest.validate_destination()?;
    let credentials = auth.credentials()?;
    bounded(timeout, async {
        let stream = TcpStream::connect(proxy).await?;
        connect_prepared(stream, dest, auth.method(), credentials).await
    })
    .await
}
