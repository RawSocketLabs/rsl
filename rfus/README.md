# rfus

RF frequency, sample-rate, and scan-target parsing utilities.

Values parse into small integer domain types through `FromStr`, with optional serde support:

| Type | Holds | Notes |
|------|-------|-------|
| `FrequencyHz` | RF frequency, `u64` Hz | no RF range bounds |
| `Hertz` | generic Hz quantity (bandwidth, offset), `u64` | `as_u32()` for hardware APIs |
| `SampleRateSps` | sample rate, `u32` S/s | |
| `FrequencyRange` | `lower < upper`, enforced by `new` | read with `lower()` / `upper()` |
| `FrequencyRanges` | non-empty list of ranges | |
| `ScanTarget` | one static frequency or `FrequencyRanges` | |

Errors are `ParseUnitError`, which is `#[non_exhaustive]`.

## Examples

```rust
use rfus::{FrequencyHz, SampleRateSps, ScanTarget};

fn main() -> Result<(), rfus::ParseUnitError> {
    let frequency: FrequencyHz = "450.5 MHz".parse()?;
    let rate: SampleRateSps = "2 MS/s".parse()?;
    let target: ScanTarget = "400m-520m; 800m-900m".parse()?;

    assert_eq!(frequency.hz(), 450_500_000);
    assert_eq!(rate.sps(), 2_000_000);
    if let ScanTarget::Ranges(ranges) = target {
        assert_eq!(ranges.as_slice()[1].lower().hz(), 800_000_000);
    }
    Ok(())
}
```

## Input rules

- **Units** are case-insensitive. Frequencies and generic hertz accept no suffix, `h`, `hz`,
  `k`, `khz`, `m`, `mhz`, `g`, and `ghz`. `m` is always mega, never milli. Sample rates also
  accept `sps`, `s/s`, `ksps`, `ks/s`, `msps`, `ms/s`, `gsps`, and `gs/s`.
- **Numbers** are unsigned decimals with optional single underscores between digits
  (`1_000_000`). A fraction is accepted only when the scaled value is a whole Hz or S/s
  (`12.5khz` is fine, `1.0000001hz` is not). Both sides of the point need a digit, so `.5` and
  `5.` are rejected.
- **Whitespace** may surround the value and separate a number from its unit.
- **Ranges** are `lower-upper` or `lower,upper`. Range lists use `;` and allow a trailing `;`.

`Display` emits bare integers without units. `FrequencyRange` displays as `lower,upper`.

## Helpers

These convenience functions return primitives when the domain type is not needed:
`parse_frequency_hz`, `parse_hertz_u32`, `parse_sample_rate_sps`, `parse_frequency_range`, and
`parse_frequency_ranges`.

## Serde

With the `serde` feature, `FrequencyHz`, `Hertz`, and `SampleRateSps` serialize as integers.
They deserialize from integers or human-readable strings. `FrequencyRange` deserializes from a
range string or `{ "lower", "upper" }`. `ScanTarget` deserializes from a compact string,
`{ "static_frequency" }`, or `{ "ranges" }` (non-empty). `ScanTarget` needs a self-describing
format such as JSON, TOML, or CBOR.

## Use through `rsl`

The `rsl` facade re-exports this crate as `rsl::rf` when its `rf` feature is enabled.
