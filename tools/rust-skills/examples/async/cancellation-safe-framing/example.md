# Preserve Framing State Across Cancellation

## Before

```rust
tokio::select! {
    result = stream.read_exact(&mut frame) => result?,
    _ = shutdown.cancelled() => return Ok(()),
}
```

Dropping the losing read can discard knowledge of partial progress.

## Review

Trace the losing future. If bytes were removed from the transport but the local
buffer and count are discarded, the next decoder starts mid-frame. Cancellation
must either preserve resumable state or abandon and reset the connection.

## After

Keep the partially filled frame and filled count in an owned driver state that
survives the selection point. Alternatively, treat cancellation as terminal,
close the transport, and document that framing state is discarded.

## Tests

Inject cancellation before input, after every partial byte count, and after a
complete frame. Verify exact resume equivalence for a resumable design and
connection reset for a terminal design. Test shutdown cleanup and bounded join.

## Lesson

Cancellation is an ownership and state transition, not merely an error return.
Every losing branch needs an explicit partial-progress policy.

## Applies when

- An async operation can consume input or mutate state before suspension.
- A timeout, `select!`, abort, or dropped future races with protocol progress.

## Does not apply when

- The operation is documented and proven cancellation-safe.
- Cancellation always destroys the complete resource and no state is reused.
