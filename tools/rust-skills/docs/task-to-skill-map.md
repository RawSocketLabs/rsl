# Task-to-Skill Map

Use `rust-core` for material Rust work, then add only the task and capability
owners implicated by the request or changed artifact. `requires` transfers
instruction ownership; `routes_to` asks whether another owner is applicable;
`related` is navigation only.

| Task or changed artifact | Primary skill | Add when applicable |
|---|---|---|
| Write, fix, refactor, or extend Rust | `rust-implement` | Every changed domain below plus `rust-testing` |
| Review a diff, branch, commit, or design | `rust-review` | Every affected domain below; do not copy their checklists into review |
| Adopt, refresh, or customize skills in a repository | `rust-repository-onboarding` | Skills selected by the confirmed base, capabilities, and component overlays |
| Public API, durable internal contract, ownership boundary, traits, errors, docs, builders, or SemVer | `rust-api-design` | `rust-testing`; protocol, unsafe/FFI, dependency/security, or performance when exposed |
| Unit, integration, doctest, example, property, fuzz, snapshot, verification, or evidence selection | `rust-testing` | The domain that defines the property or oracle |
| Binary encoding, parsing, bit fields, framing, validation, CRC/FEC, known vectors, or wire state machine | `rust-protocol` | `rust-testing`; performance and unsafe/FFI only for demonstrated optimization |
| DSP kernel, SDR pipeline, sample metadata, rate change, discontinuity, reset/finish, or signal evidence | `rust-dsp` | Testing, performance, async/concurrency, protocol, or unsafe/FFI |
| Latency, throughput, allocation, memory, cache, branch, SIMD, code size, or compile time | `rust-performance` | Testing for harness validity; domain owner for correctness; unsafe/FFI for unsafe optimization |
| Future, task, thread, runtime, pool, lock, channel, queue, backpressure, cancellation, or shutdown | `rust-async-concurrency` | Testing; protocol/DSP for state semantics; performance for measured rates |
| Unsafe block, unsafe trait, manual `Send`/`Sync`, raw pointer, ABI, callback, bindgen, C/C++ integration | `rust-unsafe-ffi` | Testing, async/concurrency, embedded, dependency/security, or performance |
| Cargo manifest, lockfile, feature, license, advisory, build script, proc macro, native dependency, crypto, or threat boundary | `rust-dependencies-security` | API design for exposed contracts; embedded or unsafe/FFI for target/native effects |
| `no_std`, firmware, target triple, interrupt, DMA, peripheral, allocator, panic, linker, flashing, or hardware test | `rust-embedded` | Testing, unsafe/FFI, async/concurrency, dependency/security, and performance |
| Change this skills repository, schemas, profiles, references, adapters, examples, evals, or release | `rust-skill-maintenance` | The skill whose behavior changes |

## Review activation order

Always prioritize behavioral correctness, soundness, security, corruption or
data loss, concurrency, error behavior, compatibility, protocol conformance,
performance, maintainability, idiomaticity, then style. `rust-review` decides
whether a finding is actionable and how it is reported; capability owners
provide the technical contract.

## Repository profile hints

| Repository shape | Suggested base and capabilities | Typical skill pack |
|---|---|---|
| Public crate | `public-library + public-api` | `rust-standard` |
| CLI or application | `application`, often `platform-specific` | `rust-standard`, then capability owners |
| Service | `service + async + concurrent + networking` | `rust-systems` |
| Binary codec/parser | library/application base + `protocol + parser-serializer` | `rust-protocol-systems` |
| SDR/DSP system | appropriate base + `dsp + performance-sensitive`, often `real-time` | `rust-systems` plus `rust-dsp` |
| Firmware | appropriate base + `embedded + no-std + platform-specific` | Embedded, unsafe/FFI, dependencies, testing |
| FFI crate | library base + `ffi + platform-specific` | Unsafe/FFI, API, dependencies, testing |
| Security-sensitive code | any base + `security-sensitive`, possibly `cryptography` | Dependencies/security plus affected owners |
| Experimental research | `experimental`, then only demonstrated capabilities | Smallest relevant set |
| Mixed workspace | one root base + `workspace`, then component overlays | Union of confirmed component sets |

Profiles supply defaults, not rigid policy. Repository decisions and closer
directory instructions remain higher precedence.
