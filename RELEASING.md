# Releasing

This workspace publishes two crates:

1. `spout2-sys`
2. `spout2-rs`

Publish `spout2-sys` first. The top-level crate depends on the matching
published `spout2-sys` version, so `cargo package -p spout2-rs` cannot resolve
fully from crates.io until `spout2-sys` exists there.

Before publishing:

```text
cargo fmt --all -- --check
cargo test --workspace
cargo test --no-default-features --features dx
cargo test --no-default-features --features gl
cargo build --no-default-features
cargo clippy --all-targets -- -D warnings
cargo doc --no-deps --all-features
cargo package -p spout2-sys
cargo package -p spout2-rs
```

The `spout2-sys` manifest uses an explicit `include` list so the published
crate contains only the shim, Rust bindings, required Spout SDK source/header
files, and license files. The full upstream SDK checkout can remain in the repo
for reference without being published to crates.io.
