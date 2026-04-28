# rfus

RF unit parsing utilities for RSL ecosystem crates.

The public API is intentionally centered on `FromStr` and serde support:

- `FrequencyHz`
- `SampleRateSps`
- `FrequencyRange`
- `ScanTarget`

Examples:

```rust
let frequency: rfus::FrequencyHz = "450m".parse()?;
let rate: rfus::SampleRateSps = "2msps".parse()?;
let target: rfus::ScanTarget = "400m-520m;800m-900m".parse()?;
```
