# Security policy

## Reporting

Please report suspected vulnerabilities through GitHub's **private vulnerability reporting**
(the repository's *Security* tab → *Report a vulnerability*), not a public issue. We'll
acknowledge and work a fix privately before disclosure.

## Threat model & dual-use scope

These crates are **dual-use by design**: they exist to produce and parse both RFC-correct *and*
deliberately non-conformant network traffic (fuzzing, red-teaming, interop testing). That shapes
what is and isn't a vulnerability.

**Is a vulnerability:**

- A panic, hang, out-of-bounds access, or unbounded allocation while **decoding untrusted bytes**
  (the parsers must be robust on arbitrary input — decode of any byte string returns an error, not
  a crash).
- Memory unsafety anywhere (the codec is zero-`unsafe`; raw-socket I/O isolates any FFI).
- A checksum/length/parser bug that causes **silent corruption** of a value that was meant to
  round-trip.

**Is *not* a vulnerability (it's the intended dual-use behavior):**

- The library producing non-compliant or malicious-looking traffic on request — that is the
  point of the raw/escape-hatch path.
- A parser accepting representable-but-non-compliant input (unknown values as `Custom(..)`).
  Permissiveness is a feature; only the *physically unencodable* is refused.
- Using these crates to test, probe, or attack systems **you are authorized to test**.

Misuse against systems you do not have permission to test is your responsibility, not a defect in
these tools.
