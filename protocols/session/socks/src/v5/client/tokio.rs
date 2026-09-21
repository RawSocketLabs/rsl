//! Tokio SOCKS5 client handshakes. Generic transports have caller-managed deadlines.
use crate::io::tokio::{bounded, write};
use crate::v5::{
    AuthMethod, Command, Endpoint, MethodRequest, MethodSelection, Reply, Request as WireRequest,
    UsernamePasswordRequest, UsernamePasswordResponse, VERSION,
};
use crate::{Stream, error::Error, v5::auth::ClientAuth};
use std::{net::SocketAddr, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpStream,
};
/// Negotiate CONNECT and return a lossless tunnel and proxy-bound endpoint. Domains
/// go to the proxy unchanged, without local DNS. Prefetched bytes stay in the tunnel.
///
/// # Errors
/// Returns a terminal transport, codec, authentication, or peer-reply error.
/// Invalid local arguments fail before writing any handshake bytes.
pub async fn connect<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    destination: Endpoint,
    auth: ClientAuth<'_>,
) -> Result<(Stream<S>, Endpoint), Error> {
    destination.validate_connect_destination()?;
    let credentials = auth.credentials()?;
    connect_prepared(stream, destination, auth.method(), credentials).await
}

async fn connect_prepared<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    destination: Endpoint,
    method: AuthMethod,
    credentials: Option<UsernamePasswordRequest>,
) -> Result<(Stream<S>, Endpoint), Error> {
    let mut stream = Stream::new(stream);
    write(
        &mut stream,
        &MethodRequest {
            version: VERSION,
            methods: vec![method],
        },
    )
    .await?;
    let selected: MethodSelection = stream.read_message_async().await?;
    selected.check_offered(method)?;
    if let Some(credentials) = credentials {
        write(&mut stream, &credentials).await?;
        stream
            .read_message_async::<UsernamePasswordResponse>()
            .await?
            .ensure_success()?;
    }
    write(
        &mut stream,
        &WireRequest {
            version: VERSION,
            reserved: 0,
            command: Command::Connect,
            destination,
        },
    )
    .await?;
    let response: Reply = stream.read_message_async().await?;
    response.ensure_success()?;
    Ok((stream, response.bound))
}

/// Dial a numeric proxy address and negotiate within one absolute timeout.
///
/// # Errors
/// Returns a terminal connection/session error, including an invalid timeout.
pub async fn connect_tcp(
    proxy: SocketAddr,
    destination: Endpoint,
    auth: ClientAuth<'_>,
    timeout: Duration,
) -> Result<(Stream<TcpStream>, Endpoint), Error> {
    destination.validate_connect_destination()?;
    let credentials = auth.credentials()?;
    bounded(timeout, async {
        let stream = TcpStream::connect(proxy).await?;
        connect_prepared(stream, destination, auth.method(), credentials).await
    })
    .await
}
