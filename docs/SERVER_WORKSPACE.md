# Server/client build separation

The desktop root workspace excludes `dokkomplekt-license-server` and the optional Python binding. Build them explicitly and independently:

```bash
cargo build --manifest-path crates/dokkomplekt-license-server/Cargo.toml
cargo build --manifest-path crates/dokkomplekt-license-python/Cargo.toml
```

The desktop release graph therefore does not include Axum, PostgreSQL or payment backend code.
