# Releasing

This workspace publishes two crates:

1. `nanalive-spout-sys`
2. `nanalive-spout`

Publish `nanalive-spout-sys` first. The top-level crate depends on the matching
published sys version, so `cargo package -p nanalive-spout` cannot resolve fully
from crates.io until `nanalive-spout-sys` exists there.

Before publishing:

```text
cargo fmt --all -- --check
cargo test --workspace
cargo test --no-default-features --features cpu-dx11
cargo test --no-default-features --features gpu-dx12-experimental
cargo build --no-default-features
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps --all-features
cargo package -p nanalive-spout-sys
cargo package -p nanalive-spout
```

The `nanalive-spout-sys` manifest uses an explicit `include` list so the published
crate contains only the shim, Rust bindings, required Spout SDK source/header
files, and license files. The full upstream SDK checkout can remain in the repo
for reference without being published to crates.io.
