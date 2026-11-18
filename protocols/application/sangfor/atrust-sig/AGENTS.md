# application/sangfor/atrust-sig

The pure primitives of the Sangfor aTrust client protocol: specification §3
([`../SPEC-atrust.md`](../SPEC-atrust.md)). refcheck protocol name: none (no public standard).

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The group guide
> [`../AGENTS.md`](../AGENTS.md), with its clean-room rule, applies first.

## Status — functional, unverified against a gateway

The rules of specification §3 are implemented and match the specification's vectors V1 to
V8, except V6's request body, which is the API layer's to build. No value here has been
checked against a real gateway.

## Architecture

No I/O, no HTTP, no clock, no state beyond the nonce generator's. One file per rule:

- `sign_key.rs` — `SignKey`: the key derived from each `authConfig` response, and the
  protocol's signature form, an HMAC-SHA-256 tag as 64 uppercase hexadecimal digits. `verify`
  uppercases what it is given and compares without stopping at the first difference.
- `challenge.rs` — `challenge_passes`: the AES-128-CBC challenge check.
- `canonical.rs` — `canonical_string`: the text the `mitmSig` covers, built from the JSON
  object as received, with numbers kept as written.
- `certificate.rs` — the certificate digest and `chain_matches`.
- `nonce.rs` — `NonceGenerator`, the vendor's clock-seeded generator. Not a source of secrets.
- `secret.rs` — `encrypt_secret`: the login secret under the gateway's RSA key.
- `identifiers.rs` — device ID, MAC address, connection ID and the header encodings.
- `hex.rs` — the two letter cases, and hexadecimal numbers of any digit count.

Primitives come from `crypto` (SHA-256, HMAC-SHA-256, the AES-128 block cipher) and
`crypto-legacy` (CBC chaining, MD5, RSA PKCS #1 v1.5 encryption). The PKCS #7 padding of the
challenge and the chunking of the secret are the protocol's and are here. JSON is parsed by
`serde_json` with raw values, so that a number's text survives.

## Rules

- **Letter case is part of the wire format.** Signatures, nonces and digests are uppercase;
  the encrypted secret, device ID and MAC address are lowercase. `hex::encode` takes the case
  as an argument so that each call site states it.
- **Text is used as received.** The device key strings, the certificate base64 and the
  challenge are hashed as the server wrote them. Do not decode, trim or re-encode them.
- **Comparisons of a secret against a peer's value** go through `crypto::Secret::matches`:
  signatures and the encrypted challenge. Certificate digests are public on both sides and
  are compared plainly.
- **The server is not trusted yet when these run.** Its inputs are bounded before they cost
  anything: JSON nesting at 16 levels, the RSA modulus at 8192 bits, and any nonce state is
  reduced into range.
- **Points the specification leaves open** are settled in `canonical_string` and listed in
  its documentation: the name a scalar inside an array takes, which of two members with one
  name survives, a `mitmSig` that holds a structure, the letter case of that name, and
  invalid string escapes. Real payloads are flat. If a gateway ever sends otherwise, that is
  a specification question, not a guess to make here.

## Testing

- **`contract`** — `tests/contract.rs`: the specification's vectors V1 to V8, then the rules
  it states without a vector, with expected values from Python where arithmetic is involved.
  `encrypt_secret` uses random padding, so it is checked by decrypting with the private half
  of a throwaway key.
- **`adversarial`** — `tests/adversarial.rs`: the bounds above, and invalid JSON.
- **`unit`** — inline, for `hex`.

The fuzz target is `sig` in [`../fuzz`](../fuzz/README.md).

Run: `cargo test -p sangfor-atrust-sig`.
