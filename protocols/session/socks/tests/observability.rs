#![cfg(feature = "v5")]
//! Verifies the crate emits `tracing` events when a subscriber is installed.
//!
//! This lives in its own integration-test binary on purpose: tracing caches a
//! callsite's interest on first encounter, so the first time an instrumented
//! callsite is hit must be inside the subscriber's scope for the event to be
//! recorded. A dedicated binary guarantees that.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use socks::client::Client;
use socks::server::Server;
use tracing_subscriber::layer::SubscriberExt;

/// A subscriber layer that just counts events.
struct Counter(Arc<AtomicUsize>);

impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for Counter {
    fn on_event(&self, _: &tracing::Event<'_>, _: tracing_subscriber::layer::Context<'_, S>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

fn spawn_echo() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = [0u8; 64];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 || stream.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    });
    addr
}

#[test]
fn emits_events_with_a_subscriber() {
    let count = Arc::new(AtomicUsize::new(0));
    let subscriber = tracing_subscriber::registry().with(Counter(count.clone()));
    let _guard = tracing::subscriber::set_default(subscriber);

    let echo = spawn_echo();
    let server = Server::bind("127.0.0.1:0").unwrap();
    let proxy = server.local_addr().unwrap();
    let handle = thread::spawn(move || server.accept());

    // The client's negotiate runs on this thread, inside the subscriber scope,
    // so its "connecting to proxy" event is recorded.
    let mut stream = Client::new(proxy).connect(echo).expect("connect");
    stream.write_all(b"ping").unwrap();
    drop(stream);
    let _ = handle.join();

    assert!(
        count.load(Ordering::Relaxed) > 0,
        "tracing should emit events"
    );
}
