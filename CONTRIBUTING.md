# Contributing

`tablegram` aims to be a drop-in native Rust parser/writer for persisted
ADO `Recordset` XML and ADTG data. Changes should preserve MDAC-compatible
behavior unless a deviation is documented with a fixture and test.

## Development Checks

Run these before submitting changes:

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --quiet
RUSTDOCFLAGS='-D warnings' cargo doc --no-deps --quiet
tools/verify_fuzz_parse_any.sh 5
```

The fuzz smoke requires nightly Rust and `cargo-fuzz`:

```bash
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
```

## Corpus Policy

Fixtures under `corpus/`, `tests/fixtures/`, and `fuzz/corpus/parse_any/` are
compatibility inputs. Do not rewrite or minimize them casually; add new focused
fixtures for new behavior or bugs.

Windows/MDAC oracle generation scripts live under `tools/`. They are development
tools and should not contain private endpoints, passwords, or machine-specific
defaults. Pass SQL Server connection details through command-line arguments or
environment variables.

## Compatibility Changes

For parser or writer behavior changes:

- Add a Rust regression test for the exact behavior.
- Add or update a corpus fixture when the behavior depends on MDAC bytes.
- Run the XP/MDAC oracle scripts when serialization output changes.
- Document unsupported MDAC behavior explicitly rather than silently dropping
  data.
