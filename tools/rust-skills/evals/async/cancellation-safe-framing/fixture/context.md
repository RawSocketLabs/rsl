# Fixture

A task repeatedly races `read_frame(&mut socket)` against a shutdown token with
`select!`. `read_frame` allocates a local `Vec`, reads a two-byte length, then
reads the payload. If shutdown wins, the future is dropped and the loop may be
re-entered during graceful drain. Frame size is currently unbounded.
