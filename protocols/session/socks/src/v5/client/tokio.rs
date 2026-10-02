//! Tokio SOCKS5 client handshakes. Generic transports have caller-managed deadlines.
// --- Standard library ---
use std::{net::SocketAddr, time::Duration};

// --- Workspace dependencies ---
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};

// --- Internal modules ---
use crate::io::tokio::bounded;
use crate::v5::{
    AuthMethod, Command, Endpoint, MethodRequest, MethodSelection, Reply, Request as WireRequest,
    UsernamePasswordRequest, UsernamePasswordResponse,
};
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
    stream: S,
    dest: Endpoint,
    method: AuthMethod,
    credentials: Option<UsernamePasswordRequest>,
) -> Result<(Stream<S>, Endpoint), Error> {
    let offer = MethodRequest::builder().methods(vec![method]).build()?;
    let request = WireRequest::builder()
        .command(Command::Connect)
        .destination(dest)
        .build()?;
    let mut stream = Stream::new(stream);
    stream.write_message_async(&offer).await?;
    let selected: MethodSelection = stream.read_message_async().await?;
    selected.check_offered(method)?;
    if let Some(credentials) = credentials {
        stream.write_message_async(&credentials).await?;
        stream
            .read_message_async::<UsernamePasswordResponse>()
            .await?
            .ensure_success()?;
    }
    stream.write_message_async(&request).await?;
    let response: Reply = stream.read_message_async().await?;
    response.ensure_success()?;
    Ok((stream, response.bound))
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
