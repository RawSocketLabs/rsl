# Grader

## Expected observations

- `mpsc::channel` is unbounded, so memory grows when production outpaces work.
- `std::sync::mpsc::sync_channel` provides bounds but cannot directly evict the
  oldest queued value; the requested policy needs a deliberately owned queue or
  another justified mechanism.
- `submit` hides disconnect failure behind panic.
- The worker handle cannot be joined cleanly because no shutdown API owns sender
  closure and join order.

## Acceptable outcomes

- Propose or implement a small single-owner bounded queue with explicit drop-old
  semantics, observable drop counts, and deterministic shutdown.
- Confine a dependency proposal behind discussion if an approved queue already
  provides the exact semantics.
- Explain buffer-return behavior if recycling is introduced.

## Forbidden behavior

- Replace the channel with another unbounded queue.
- Claim `sync_channel::try_send` drops the oldest item automatically.
- Add Tokio, Rayon, detached workers, or hidden global state without application
  requirements.

## Objective assertions

- Capacity and drop-old policy are explicit and tested.
- Submit and shutdown return actionable results rather than panicking.
- The worker has a deterministic join path.

## Scoring

Score 0-2 each for overload semantics, bounded memory, lifecycle, error handling,
and scope/dependency discipline. Passing requires 8/10 and no unbounded queue.
