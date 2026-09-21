//! `cargo run -p socks --features mio --example mio_proxy -- 127.0.0.1:1080 127.0.0.1:8080`
//! Only the explicitly supplied numeric target is permitted. No-auth is an explicit
//! demonstration choice. Use `Proxy::shutdown_handle()` in the application's signal
//! handler/controller to stop; no signal/runtime dependency is added here.
use mio::net::TcpListener;
use socks::{
    Server, Version,
    proxy::mio::Proxy,
    server::policy::{Policy, ServerAuth},
};
use std::{
    io::{self, Write},
    net::SocketAddr,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let bind: SocketAddr = args
        .next()
        .ok_or("supply bind address and permitted target")?
        .parse()?;
    let allowed: SocketAddr = args
        .next()
        .ok_or("supply a permitted numeric target")?
        .parse()?;
    let server = Server::builder()
        .protocols([Version::V5])
        .policy(Policy::new(
            ServerAuth::no_authentication(),
            move |context| context.target == allowed,
        ))
        .build()?;
    let mut proxy = Proxy::new(TcpListener::bind(bind)?, server)?;
    proxy.run(|error| {
        let _ = writeln!(io::stderr(), "{error}");
    })?;
    Ok(())
}
