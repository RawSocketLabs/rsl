//! Client/server integration: handler rejections map to the right peer errors,
//! and the raw surface interoperates with the real server.

mod common;

use std::sync::Arc;

use tftp::client::Client;
use tftp::error::TftpError;
use tftp::server::{Handler, HandlerResult, Reject, Server};
use tftp::wire::ErrorCode;

use common::TEST_TIMEOUT;

/// A handler that serves one fixed file for reads and refuses every write.
struct ReadOnly(Vec<u8>);

impl Handler for ReadOnly {
    fn read(&self, _filename: &str) -> HandlerResult<Vec<u8>> {
        Ok(self.0.clone())
    }
    fn write(&self, _filename: &str, _contents: Vec<u8>) -> HandlerResult<()> {
        Err(Reject::access_violation())
    }
    fn authorize_write(&self, _filename: &str) -> HandlerResult<()> {
        Err(Reject::access_violation())
    }
}

fn spawn(handler: Arc<dyn Handler>) -> std::net::SocketAddr {
    let server = Server::bind("127.0.0.1:0", handler)
        .unwrap()
        .with_timeout(TEST_TIMEOUT);
    let addr = server.local_addr().unwrap();
    std::thread::spawn(move || {
        let _ = server.serve();
    });
    addr
}

#[test]
fn write_to_read_only_handler_is_refused_before_data() {
    let addr = spawn(Arc::new(ReadOnly(b"fixed".to_vec())));
    let err = Client::new(addr)
        .with_timeout(TEST_TIMEOUT)
        .put("anything", b"data")
        .expect_err("a read-only server must refuse the write");
    assert!(
        matches!(
            err,
            TftpError::Peer {
                code: ErrorCode::AccessViolation,
                ..
            }
        ),
        "got {err:?}"
    );
}

#[test]
fn read_from_read_only_handler_succeeds() {
    let addr = spawn(Arc::new(ReadOnly(b"fixed contents".to_vec())));
    let got = Client::new(addr)
        .with_timeout(TEST_TIMEOUT)
        .get("x")
        .unwrap();
    assert_eq!(got, b"fixed contents");
}

#[test]
fn custom_reject_code_is_surfaced() {
    struct ExistsGuard;
    impl Handler for ExistsGuard {
        fn read(&self, _f: &str) -> HandlerResult<Vec<u8>> {
            Err(Reject::not_found())
        }
        fn write(&self, _f: &str, _c: Vec<u8>) -> HandlerResult<()> {
            Ok(())
        }
        fn authorize_write(&self, _f: &str) -> HandlerResult<()> {
            Err(Reject::already_exists())
        }
    }

    let addr = spawn(Arc::new(ExistsGuard));
    let err = Client::new(addr)
        .with_timeout(TEST_TIMEOUT)
        .put("dup", b"x")
        .expect_err("must be refused");
    assert!(
        matches!(
            err,
            TftpError::Peer {
                code: ErrorCode::FileAlreadyExists,
                ..
            }
        ),
        "got {err:?}"
    );
}
