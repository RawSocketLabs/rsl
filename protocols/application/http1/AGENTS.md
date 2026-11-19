# application/http1

HTTP/1.1 message syntax for a client (RFC 9112, with RFC 9110 for field and token syntax).
refcheck protocol name: **`http1`**.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../../AGENTS.md) and protocol guide [`../../AGENTS.md`](../../AGENTS.md)
> (dual-use philosophy) also apply.

## Status — dev (codec and a blocking exchange)

A request type that encodes to bytes, a sans-I/O decoder that turns a server's bytes into a
response, and `blocking::exchange`, which does one request and response over any blocking
`Read + Write` transport. No redirects, cookies, connection pool, proxies or
`Expect: 100-continue`; no content codings (nothing is decompressed); no transfer coding but
`chunked`; no clock, so no timeout of its own.

## Architecture

- `src/wire/request.rs` — `Request { method, target, headers, body }`. `encode` writes the
  fields verbatim. `Request::get`, `Request::post` and `Request::header` are the guided
  path.
- `src/wire/headers.rs` — `Headers`, an ordered list with case-insensitive lookup and
  comma-list helpers.
- `src/wire/response_head.rs` — `ResponseHead`: the typed status line and fields as a view
  of the raw head bytes it keeps.
- `src/wire/body_framing.rs` — `BodyFraming::of_response`, the RFC 9112 §6.3 decision of how
  a response's body ends, in the RFC's order of precedence.
- `src/wire/chunked_decoder.rs` — `ChunkedDecoder`, incremental, with extensions and
  trailers skipped under a bound.
- `src/wire/response_decoder.rs` — `ResponseDecoder`, the sans-I/O state machine: head,
  interim responses, body, end of connection. Every framing decision lives here, so that a
  read loop built on it needs none. A loop that inspects a status code or a header to decide
  how much to read is a bug in the making; move the decision into the decoder.
- `src/blocking/` — `exchange`: write the request, flush, read until the decoder says the
  response is complete. It holds no protocol logic beyond telling the decoder how the
  connection ended (end of input is a clean close; an `UnexpectedEof` error, which is how a
  TLS stream reports a missing `close_notify`, is not) and clearing `keep_alive` when the
  request asked to close, was not HTTP/1.1, or surplus bytes arrived with the response's
  end. Surplus that arrives later cannot be seen through `Read`.
- `src/limits.rs` — `Limits`. Every buffer is bounded and each bound fails as soon as it is
  known to be exceeded.

## This crate does not use bnb

The protocols group's codec is bnb, and a shape bnb cannot express is normally a bnb finding
to fix there. HTTP/1 is the recorded exception:

- A head is lines of text ended by a two-byte delimiter, with token grammars and optional
  whitespace. It has no bit layout, no fixed widths and no derived lengths, which is what bnb
  is for.
- bnb's only delimiter codec reads one byte at a time to a one-byte terminator, and its
  writer refuses a value containing the terminator, which is exactly what the raw request
  path must be able to send.
- A delimiter gives no "need N more bytes" hint, so retrying a bnb decode as bytes arrive
  would rescan from the start each time. The decoder here scans each byte once.

The finding is recorded in `../../ROADMAP.md` under bnb co-evolution. A second text protocol
should reuse this reasoning, not this code, until there are enough of them to say what a
shared text-line layer would be.

## Entry points

- Build a request: `Request::get` / `Request::post`, then `.header(..)`; `encode()` for the
  bytes. For anything the guided path refuses, set the public fields.
- Decode a response: `ResponseDecoder::new(&request.method, limits)`, then `receive(bytes)`
  until `is_complete()`, calling `receive_eof(clean)` if the connection ends first;
  `finish()` for the `Response`. `receive` returns how many bytes it used: what is left
  over belongs to the next response.
- `BodyFraming::of_response` and `ChunkedDecoder` are public for callers that frame bodies
  themselves; the decoder is the normal way in.
- One exchange over a transport: `blocking::exchange(&mut transport, &request, limits)`.
  Reuse the transport for another only when the response's `keep_alive` is true, and treat
  a failure on a reused connection as a reason to retry on a new one.

## The dual-use rule here

- **Requests:** every `Request` field is public and `encode` checks nothing, so any method,
  target, header bytes or mismatched `Content-Length` can be sent. The guided constructors
  refuse a carriage return, line feed or NUL in a value, a non-token name, and whitespace or
  control bytes in a target: the bytes that let a value end its own line and inject a field
  or a second request.
- **Responses:** values are preserved whatever they are (any version digits, any three-digit
  status, any reason, unknown fields), and the raw head is kept. What is refused is syntax
  that lets two parsers disagree about where a field or a message ends: a lone LF or CR, a
  folded field, whitespace before a colon, NUL, `Transfer-Encoding` together with
  `Content-Length`, a `Content-Length` that is not one decimal number. A client that
  accepted those could be made to read a different message than the server (or anything in
  between) believes it sent.
- A body that runs to the end of the connection completes only on a clean close, unless
  `Limits::accept_unclean_close` is set. Over TLS a clean close is a `close_notify`; without
  one the body cannot be told from a truncated one.
- Where the RFC lets a recipient choose and this crate takes the stricter choice, or does
  something else, the item carries a `//~ rejects` or `//~ deviates` line with the reason: a
  lone LF as a line end and a folded field (`response_head.rs`), `Transfer-Encoding` with
  `Content-Length`, and a transfer coding it cannot decode (`body_framing.rs`).

## Testing

- **`contract`** — `tests/contract.rs`: request bytes; a corpus of responses, one captured
  from a real server and the rest synthetic (laid out from the RFC's grammar, which has
  almost no byte-exact examples). The anchor is **split invariance**: every response, cut at
  every point, must decode to the same response using the same number of bytes; bytes after
  it are left for the next; every strict prefix is an unfinished response.
- **`adversarial`** — `tests/adversarial.rs`: the refused syntax above (each at every split),
  malformed chunking, each limit at its size and one past it, interim-response floods, and
  the guided request refusals beside the raw path.
- **`unit`** — inline, for what only the crate can reach: the token-byte table, the
  whitespace trim, and the interim-response arm of the framing rules.
- **`blocking`** — `tests/blocking.rs`: the exchange over a scripted transport that panics
  on a read before the request is flushed or after the response is complete.
- **`interop`** — `tests/interop.rs`, ignored by default: the exchange against Python's
  `http.server` over real TCP. Run it with `cargo test -p http1 --test interop -- --ignored`.
- Each framing decision was confirmed by changing it and seeing a test fail. Do the same for
  a new one. The [`http_response` fuzz target](../sangfor/fuzz/README.md) also compares the
  result and consumed length across transport read sizes.

Run everything: `cargo test -p http1`.

## Scope notes

- Field and list syntax follow RFC 9110 §5; only what framing needs is interpreted
  (`Content-Length`, `Transfer-Encoding`, `Connection`).
- Form encoding, URIs and cookies are not HTTP/1 syntax and are not in this crate.
- A deadline for a whole exchange is the transport's to enforce: a read timeout on a socket
  bounds one read, not the exchange. A transport wrapper that re-arms the remaining time
  before each read and write is still to be written.
