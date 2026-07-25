# Fixture

A public async library changes:

```toml
runtime = { version = "1", default-features = false, features = ["sync"] }
```

Previously it enabled `["rt", "sync", "time"]`. The author says the crate still
compiles with its default feature set and calls the change non-breaking. The
library exposes a public timeout helper behind its own `timeouts` feature and
tests only default features in CI.
