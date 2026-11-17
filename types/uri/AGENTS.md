# types/uri

URI value types and encodings. So far: percent-encoding of form and query data.

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../AGENTS.md) also applies.

## What it is

- `form::Encoding` — one way of percent-encoding, stated in full: the set of bytes kept as
  they are (`ByteSet`) and how a space is written (`Space`). `component` encodes one name or
  value; `pairs` joins `name=value` with `&` in the order given.
- `Encoding::QUERY` — the common server-side form: RFC 3986 unreserved characters kept, space
  as `+`, everything else encoded with uppercase hex. It matches Python's `quote_plus` and
  Go's `QueryEscape`.

There is no URI type and no decoder yet. Add them here when a consumer needs to take a URI
apart (a `Location` header, say), not before.

## Rules

- `#![no_std]` with `alloc`, `#![forbid(unsafe_code)]`, no dependencies. No I/O and no
  protocol logic: values and their encodings only.
- **No default encoding and no sorting.** Encoders in the wild disagree about `*`, `~` and
  the space, and protocols that sign or compare encoded bytes depend on exactly one answer
  and one order. The caller names the encoding and supplies the order; adding a "sensible
  default" or a sort flag here would hide the decision a protocol has to make.
- A new named encoding gets a constant and a test against an independent implementation
  that produces it, named in the test.

## Testing

`tests/contract.rs`: expected values from Python's `urllib.parse.quote_plus`; a
caller-defined set; pair joining; and, for every byte value, that its encoding is one of the
three allowed forms and names the byte.

Run: `cargo test -p uri`.
