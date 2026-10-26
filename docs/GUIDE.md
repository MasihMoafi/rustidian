# Rustidian contributor guide

Rustidian is a native Rust notebook for ordinary Markdown folders. The interface uses egui/eframe; vault, editor and persistence behavior lives in `src/`. Behavioral checks live in `tests/`.

Use the declared Rust toolchain. Run formatting and core checks with:

```sh
cargo fmt --all -- --check
cargo clippy --locked --no-default-features --lib --tests -- -D warnings
cargo test --locked --no-default-features --lib --tests
```

Desktop builds require the Linux development libraries listed in the Actions workflows. Build with `cargo build --locked --release`. Native GUI checks use disposable vault copies on a private X11 display through `tests/gui_smoke.py`; never use personal notes as test fixtures.

Preserve ordinary file bytes, atomic saves, external-change detection and chronological Undo/Redo. Verify visible behavior and exact saved content before packaging. `scripts/package-linux.py` and `scripts/package-deb.py` package an explicitly selected executable; the release assets include checksums and build information.

Keep private notes, session logs, profiles, credentials and local reports out of commits and release archives. [Feature coverage](PARITY.md) describes current limits.
