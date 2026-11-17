//! Helpers shared by the test binaries.
#![allow(dead_code, reason = "each test binary uses a different part")]

pub(crate) mod fixtures;

use http1::{Limits, Response, ResponseDecoder};

/// An HTTP response with `status`, the given extra header lines and `body`, decoded from its
/// wire form the way a real one is.
pub(crate) fn response(status: u16, headers: &[&str], body: &[u8]) -> Response {
    let mut wire = format!("HTTP/1.1 {status} X\r\nContent-Length: {}\r\n", body.len());
    for header in headers {
        wire.push_str(header);
        wire.push_str("\r\n");
    }
    wire.push_str("\r\n");
    let mut wire = wire.into_bytes();
    wire.extend_from_slice(body);

    let mut decoder = ResponseDecoder::new(b"GET", Limits::default());
    let consumed = decoder.receive(&wire).unwrap();
    assert_eq!(
        consumed,
        wire.len(),
        "the test response must be one whole response"
    );
    decoder.finish().unwrap()
}

/// A 200 response with a JSON body.
pub(crate) fn ok(body: &str) -> Response {
    response(200, &[], body.as_bytes())
}

/// A source of bytes that counts upward. It makes a login's output reproducible. It is not
/// random and must never leave the tests.
#[derive(Default)]
pub(crate) struct Counting(u8);

impl crypto::RandomSource for Counting {
    fn fill_bytes(&mut self, output: &mut [u8]) -> crypto::Result<()> {
        for byte in output {
            self.0 = self.0.wrapping_add(1);
            *byte = self.0;
        }
        Ok(())
    }
}

/// A gateway that answers from a script: each step names the path it expects and what it
/// answers, and every request is kept for inspection. A request the script does not expect
/// fails the test.
#[derive(Default)]
pub(crate) struct Script {
    steps: std::collections::VecDeque<(&'static str, std::io::Result<sangfor_atrust::api::Reply>)>,
    /// The requests received, in order.
    pub(crate) seen: Vec<http1::Request>,
}

impl Script {
    /// Expect a request for `path` next and answer it with `response`.
    pub(crate) fn then(mut self, path: &'static str, response: Response) -> Self {
        let reply = sangfor_atrust::api::Reply {
            response,
            peer_chain: Vec::new(),
        };
        self.steps.push_back((path, Ok(reply)));
        self
    }

    /// Like [`then`](Self::then), over a connection whose server presented `peer_chain`.
    pub(crate) fn then_over(
        mut self,
        path: &'static str,
        response: Response,
        peer_chain: Vec<Vec<u8>>,
    ) -> Self {
        let reply = sangfor_atrust::api::Reply {
            response,
            peer_chain,
        };
        self.steps.push_back((path, Ok(reply)));
        self
    }

    /// Expect a request for `path` next and fail to carry it.
    pub(crate) fn then_fail(mut self, path: &'static str) -> Self {
        let error = std::io::Error::from(std::io::ErrorKind::ConnectionReset);
        self.steps.push_back((path, Err(error)));
        self
    }

    /// The paths of the requests received, in order.
    pub(crate) fn paths(&self) -> Vec<String> {
        self.seen.iter().map(path_of).collect()
    }

    /// The request received for `path`. There must be exactly one.
    pub(crate) fn request(&self, path: &str) -> &http1::Request {
        let mut matching = self.seen.iter().filter(|request| path_of(request) == path);
        let request = matching
            .next()
            .unwrap_or_else(|| panic!("no request for {path}"));
        assert!(
            matching.next().is_none(),
            "more than one request for {path}"
        );
        request
    }

    /// Whether every scripted step was used.
    pub(crate) fn is_finished(&self) -> bool {
        self.steps.is_empty()
    }
}

impl sangfor_atrust::api::Portal for Script {
    fn exchange(
        &mut self,
        request: &http1::Request,
    ) -> std::io::Result<sangfor_atrust::api::Reply> {
        let path = path_of(request);
        let (expected, reply) = self
            .steps
            .pop_front()
            .unwrap_or_else(|| panic!("the script is over, and a request came for {path}"));
        assert_eq!(
            path, expected,
            "the request is not the one the script expects next"
        );
        self.seen.push(request.clone());
        reply
    }
}

/// The path of a request, without its query.
fn path_of(request: &http1::Request) -> String {
    let target = String::from_utf8(request.target.clone()).unwrap();
    target.split('?').next().unwrap().to_owned()
}

/// The value of a request's header as text, when it has one.
pub(crate) fn header<'a>(request: &'a http1::Request, name: &str) -> Option<&'a str> {
    request
        .headers
        .get(name)
        .map(|value| std::str::from_utf8(value).unwrap())
}
