# application/sangfor — agent & contributor guide

Client protocols for Sangfor VPN gateways: aTrust (current) and EasyConnect (older, deferred).

> Canonical agent-guidance file; `CLAUDE.md` is a symlink to it. The workspace root
> [`AGENTS.md`](../../../AGENTS.md) and protocol guide [`../../AGENTS.md`](../../AGENTS.md) also
> apply. Each crate here has its own `AGENTS.md` for its detail.

## The clean-room rule

**Do not read the reference client's source.** The crates here are written from the two
behaviour specifications in this directory, [`SPEC-atrust.md`](SPEC-atrust.md) and
[`SPEC-easyconnect.md`](SPEC-easyconnect.md), and from nothing else. The specifications were
written from the behaviour of an AGPL-3.0 client and audited so that they carry facts, not
that client's expression; this workspace is MIT OR Apache-2.0. Reading the reference source
while writing code here would undo that separation.

- The file paths the specifications cite are for tracing a statement back, by someone whose
  job is the specification. They are not reading suggestions.
- When the specification is unclear, wrong or silent, the fix is a change to the
  specification, made by someone who is not also writing the implementation, or a question
  recorded for a test against a real gateway. It is never "check what the reference does".
- Do not paste reference code, comments, identifiers or test tables into this directory, a
  commit message, or a pull request.

## What is here

| Path | Crate | Stage |
|---|---|---|
| `atrust-sig/` | `sangfor-atrust-sig` | functional: the pure primitives of spec §3 |
| `atrust-wire/` | `sangfor-atrust-wire` | wire layer: tunnel session authentication and the TCP tunnel, spec §7 and §9; the L3 frames of §8 are not written |
| `atrust/` | `sangfor-atrust` | in progress: the policy model of spec §6, the control-plane API of §4, the password login of §5 and the TCP tunnel of §9, tested against a scripted gateway and node; the network layer is not written |

The plan and the first milestone are in the Sangfor
section of [`../../ROADMAP.md`](../../ROADMAP.md).

## Rules for every crate here

- **Layers stay separate.** Anything generic belongs in its own crate below these: TLS in
  `session/tls`, HTTP in `application/http1`, cookies in `application/cookie`, encodings in
  `types/uri`, primitives in `crypto` and `crypto-legacy`. A crate here adds only what is
  Sangfor's.
- **Cite the specification.** Each item says which section it implements, and each test which
  vector or rule it checks.
- **Say what a vector proves.** The specifications' vectors are derived from their own rules,
  not captured from a gateway. A test that matches one shows the code does what the
  specification says. Only a test against a real gateway shows the specification is right;
  nothing here has had one yet.
- **These are the vendor's constructions.** Several use primitives that are no longer
  recommended. Reproduce them exactly and do not present them as designs.
