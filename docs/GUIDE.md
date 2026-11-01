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

## Undo changes without losing earlier edits

Keep the policy in the shared vendored `TextEditState`, not in individual note views: a 500 ms pause ends a typing group and continuous typing keeps a checkpoint each second. Paste, cut and selection replacement are single edits. Before recording their final checkpoint, feed the changed state so editing after Undo discards the old Redo branch. Keep cursor-only snapshot skipping and per-document shared histories intact.

`tests/undo_input_behavior.rs` drives actual widget input. For a small regression build, `cargo rustc --offline --test undo_input_behavior` avoids rebuilding the application binary. For the app boundary, `tests/gui_smoke.py --binary /exact/candidate --section undo` uses disposable notes, real shortcuts and exact saved bytes in Source and Live.
