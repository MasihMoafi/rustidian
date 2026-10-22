# Contributing to Rustidian

Rust and egui contributors, testers and careful bug reports are welcome.
The goal is a familiar Obsidian workflow with faster startup and interaction.
The [coverage ledger](docs/PARITY.md) records the remaining work; pick a concrete
workflow and keep the change focused.

For a bug report, include the version, platform, steps and expected result.
Use a small synthetic Markdown example instead of private notes or profiles.
For an Obsidian comparison, record the actual reference version and show the
same steps in both apps. Include screenshots when appearance matters.

For a fix, first demonstrate the failure on the existing app. Preserve the
strongest behavioral regression that catches it, then verify the repaired flow
and the surrounding editing, saving and navigation behavior. Avoid tests that
only search source code for implementation details.

This workspace uses [Native Rust Actions](.github/workflows/native.yml) for
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
