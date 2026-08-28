# rsl-compression — design

Compression is stateful and may expand its input. It is therefore modeled with separate encoder
and decoder contexts that append to caller-visible output buffers and report exact byte counts.
Algorithms will define their own flush and dictionary semantics rather than hiding them behind a
universal transformation API.

This crate does not own cryptographic “compression functions” such as the internal block function
of SHA-256. Those operations are inseparable layers of their cryptographic primitives and remain
in `rsl-crypto`.
