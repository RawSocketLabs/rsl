# rfus

RF unit parsing utilities for RSL ecosystem crates.

The public API is intentionally centered on `FromStr`, small domain types, and
optional serde support:

- `FrequencyHz`
- `Hertz`
- `SampleRateSps`
- `FrequencyRange`
- `ScanTarget`

## Accepted units

Frequency and generic hertz values accept no suffix, `h`, `hz`, `k`, `khz`,
`m`, `mhz`, `g`, and `ghz`.

Sample rates accept those same suffixes plus sample-rate forms: `sps`, `s/s`,
`ksps`, `ks/s`, `msps`, `ms/s`, `gsps`, and `gs/s`.

Numeric literals may contain underscores, optional whitespace may appear between
the number and unit, and decimal values are accepted when the scaled result is a
whole integer value.

## Examples

```rust
let frequency: rfus::FrequencyHz = "450m".parse()?;
let rate: rfus::SampleRateSps = "2msps".parse()?;
let target: rfus::ScanTarget = "400m-520m;800m-900m".parse()?;
```

Frequency ranges use `lower-upper` or `lower,upper`; range lists use semicolons.
`FrequencyRange` requires `lower < upper`.

Convenience helpers return primitive values when a domain type is not needed:

- `parse_frequency_hz`
- `parse_hertz_u32`
- `parse_sample_rate_sps`
- `parse_frequency_range`
- `parse_frequency_ranges`
