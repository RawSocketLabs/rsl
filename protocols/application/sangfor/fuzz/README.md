# Sangfor fuzzing

Coverage-guided fuzzing of what a gateway or a node controls in the aTrust client: the wire
formats, the control-plane answers, the policy document, the values the signing primitives are
given, the login sequence, the TCP tunnel and the HTTP and TLS decoders it uses.
This is a separate workspace (note the empty `[workspace]` table in `Cargo.toml`) so
`libfuzzer-sys` and the nightly toolchain never touch the parent workspace or its stable
`Cargo.lock`.

```bash
cargo install cargo-fuzz                                                    # once
cargo +nightly fuzz run wire --fuzz-dir protocols/application/sangfor/fuzz  # from the repo root
```

The tree pins stable through `rust-toolchain.toml`, so the explicit `+nightly` is required. If
`cargo fuzz` picks a target without a sanitizer runtime, add `--target x86_64-unknown-linux-gnu`,
as CI does. A crash leaves a reproducer in `artifacts/<target>/`; pass its path to replay it.

Routine CI does not fuzz and does not build these targets, so a change that breaks one shows
only when they are next built. Each runs with a bounded budget (45 seconds or 500,000 runs)
when a release reaches this workspace through its dependencies. No Sangfor crate is released
yet, but `sangfor-atrust-wire` is built on `bnb`, so a `bitsandbytes` release selects every target.

| Target | Boundary | Invariants |
|---|---|---|
| `http_response` | `ResponseDecoder` over raw input or an HTTP head followed by input, up to 4 KiB | Whole and fragmented reads agree on the error or the complete response and length consumed. A valid generated Content-Length response delivers the original body. |
| `tls_client` | `Client::receive` over raw records, a ServerHello body, or a valid ServerHello followed by a Certificate body, up to 4 KiB of input | Whole and fragmented reads agree on errors, outbound TLS bytes, plaintext, and peer closure. |
| `sig` | The primitives of `sangfor-atrust-sig` over text a gateway supplies: the canonical string, the challenge check, key derivation, verification, encryption of the login secret for an announced key, the certificate comparison | no panic; a derived key verifies what it signed |
| `wire` | Every type of `sangfor-atrust-wire` over arbitrary bytes | no panic; a decoded value encodes to exactly the bytes it took, decodes the same from those bytes alone, and was not decodable from one byte fewer |
| `api` | Every control-plane answer parser over a chosen status and body: whole, as the `data` of an accepted envelope, or as one list entry repeated past the bound | no panic; an answer's lists hold at most 256 entries |
| `policy` | `Policy::parse` over a whole document, and over text and numeric values set into one rule of a valid document; then every lookup | no panic; a document that is JSON is read whatever its values; a document yields no more rules than it has bytes; an address block or range is the one rule the specification gives it |
| `tunnel` | `tcp::Stream` over a node that sends arbitrary bytes after a chosen part of a valid answer | no panic; how the node's bytes are cut into reads changes nothing a caller can observe; a write goes out as it is in raw mode, and in framed mode as whole frames that read back as it |
| `login` | `Session::login` against a gateway whose every answer is the valid one or a status, header line and body of the input's choosing | no panic; at most seven requests plus `max_steps`; the session reported is the one the jar holds; a strict login never succeeds past a failed anti-MITM check, and after one nothing more was sent and the jar is what it was; a gateway that answers properly is logged in to with every check passed |

The corpus is not kept in the repository, so each target carries its own starting points: a
selector byte chooses how much valid structure surrounds the input. The Sangfor-specific
targets use the specification's vectors (V3 to V6, V15, V16, V18, V21), which were derived
from its rules and not captured from a gateway. Fuzzing is engineering evidence, not an audit
or a security proof.

Not covered: the `net` feature is off here, so its one parser of gateway text, the split of a
node address into host and port, has only its unit tests.
