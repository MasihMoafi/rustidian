# Contributing to Rustidian

Rust and egui contributors, testers and careful bug reports are welcome.
The goal is a familiar Obsidian workflow with faster startup and interaction.
The [coverage ledger](docs/PARITY.md) records the remaining work; pick a concrete
workflow and keep the change focused.

Use the [bug report](https://github.com/MasihMoafi/rustidian/issues/new?template=bug_report.yml) or [Obsidian workflow gap](https://github.com/MasihMoafi/rustidian/issues/new?template=parity_gap.yml) form. Include `rustidian --version`, your Linux distribution, X11/Wayland, steps and expected result.
Use a small synthetic Markdown example instead of private notes or profiles.
For an Obsidian comparison, record the actual reference version and show the
same steps in both apps. Include screenshots when appearance matters.

For a fix, first demonstrate the failure on the existing app. Preserve the
strongest behavioral regression that catches it, then verify the repaired flow
and the surrounding editing, saving and navigation behavior. Avoid tests that
only search source code for implementation details.

Build with the declared Rust 1.95+ toolchain and the locked dependencies:

```sh
cargo build --locked --release --bin rustidian
./target/release/rustidian ./demo-vault
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

Linux builds need the X11, Wayland, EGL and xkbcommon development libraries. [Native Rust Actions](.github/workflows/native.yml) lists the exact Ubuntu packages. Font data is bundled; no account, API key or network service is needed to run the editor. Package a verified Linux binary with `python3 scripts/package-linux.py --binary target/release/rustidian --output rustidian-linux-x86_64.tar.gz`.

This maintainer workspace uses [Native Rust Actions](.github/workflows/native.yml) for
compilation, Rust tests, Clippy, native GUI checks and performance gates. Keep
local work light with `cargo fmt --all -- --check`; do not commit `target/`,
profiles, real vaults or local reports. A maintainer checks the exact optimized
artifact before delivery. See [the development guide](docs/GUIDE.md) for the
reference-first workflow and source map.

Keep changes native, reuse existing file and rendering paths, and measure
performance with matching inputs and recorded versions. Do not claim whole-app
parity or universal speed from a passing test or a small benchmark.

Project code is [MIT licensed](LICENSE). Bundled fonts, artwork provenance and
vendored components retain their own notices. Contribute only work you have
the right to share.
