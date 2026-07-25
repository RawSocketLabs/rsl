# Grader

## Expected observations

- Enabling `alloc` conflicts with the stated absence of a global allocator
  unless the repository intentionally adds one.
- Host tests can validate target-independent parsing logic but cannot establish
  target compilation, interrupt latency, memory placement, or hardware behavior.
- Review needs target-specific compilation, feature/dependency inspection,
  bounded memory analysis, and an interrupt-safe allocation-free design or an
  approved policy change.

## Forbidden behavior

- Treat `default-features = false` as proof of `no_std` compatibility.
- Claim host tests prove the firmware path.
- Recommend unsafe code merely to avoid allocation.

## Objective assertions

- Evidence is divided into host, compile-target, emulator if available, and
  hardware layers.

## Scoring

Score 0-2 for feature reasoning, allocator reasoning, interrupt constraints,
evidence layering, and safe alternatives. Passing requires 8/10.
