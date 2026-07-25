# Embedded and `no_std` Contracts

### EMBED-TARGET-001 Separate host, target, emulator, and hardware evidence

- **Strength:** MUST
- **Applies to:** `no_std`, firmware, architecture-specific code, peripherals,
  linkers, bootloaders, and board behavior
- **Directive:** Record the exact target triple, CPU features, hardware revision,
  linker and memory layout, toolchain, build profile, and test environment.
  Label host compilation, target compilation, emulator execution, flashing, and
  observed hardware behavior separately.
- **Why:** Host tests cannot establish ABI, linker, interrupt, peripheral,
  timing, cache, or electrical behavior on the target.
- **Exceptions:** Pure platform-independent logic may rely on host runtime tests
  plus a target compile when its boundary to hardware is independently tested.
- **Mechanical owner:** Target builds, emulator or hardware tests, artifact
  inspection, and evidence report.
- **Sources:** Preferences R57-R64 and R73-R79.

### EMBED-RESOURCE-001 Make constrained resources explicit

- **Strength:** MUST
- **Applies to:** heap, stack, static memory, DMA pools, flash, binary size,
  interrupt latency, power, and bounded execution
- **Directive:** State whether allocation is forbidden, bounded, startup-only, or
  allowed; define panic behavior; bound stack, static storage, DMA buffers,
  queues, and recursion; and measure applicable flash, RAM, code size, latency,
  and timing on the shipped target configuration.
- **Why:** Implicit desktop resource assumptions become link failures, runtime
  exhaustion, missed deadlines, or unrecoverable resets on constrained targets.
- **Exceptions:** Experimental bring-up may defer precise budgets when the
  deferral is explicit and no production guarantee is claimed.
- **Mechanical owner:** Link map and size checks, stack analysis where
  available, allocation tests, timing tests, and target review.
- **Sources:** Preferences R37-R40, R58-R64, R72, and R172.

### EMBED-CONC-001 Treat interrupts and DMA as concurrent owners

- **Strength:** MUST
- **Applies to:** interrupts, exceptions, DMA, memory-mapped I/O, critical
  sections, atomics, shared buffers, and peripheral state
- **Directive:** Define ownership and synchronization across main code,
  interrupts, DMA engines, and cores. Keep critical sections bounded, select
  atomics supported by the target, make cache coherency and memory ordering
  explicit, and prevent safe code from reusing a buffer while hardware owns it.
- **Why:** Hardware agents and interrupts create real concurrent mutation even
  without operating-system threads.
- **Exceptions:** Single-owner peripherals with interrupts disabled by design
  may use a simpler proof when startup and reset transitions preserve it.
- **Mechanical owner:** State-machine and ownership tests, target race tests,
  Loom or host models where representative, unsafe review, and hardware tests.
- **Sources:** Preferences R29-R34, R42-R47, and R73-R79.
