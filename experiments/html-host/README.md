# Local HTML engine experiment

Question: can a separate Rust CEF process render local HTML/CSS/images and accept
native input while the notebook keeps its native editor and document ownership?

CEF156.2.0+156.0.14 CPU offscreen rendering passed two disposable X11 cases:
the button changes both the reported DOM and painted status text; a disabled
button leaves both unchanged. Relative CSS/SVG load and source files stay intact.
The browser acknowledges close before shutdown. Sandbox remains enabled.

This is an isolated CLI experiment. Rustidian has no embedded browser tab or
Obsidian API compatibility yet. Wayland, resize/scroll/keyboard/IME, accessibility,
idle cost and production process recovery remain unverified.

Use a normalized CEF156.0.14 SDK with `archive.json`, libraries, resources/locales
and required licenses. Set `CEF_PATH` to that directory and `CARGO_TARGET_DIR` to
a separate New Volume cache. The SDK is1.59GB including debug symbols; the
verified minimal download is331,729,249bytes. These are SDK sizes, not an app
package measurement. No SDK is included in Git or the29.6MB core installer.

Build with one job under the workstation game/disk/thermal guards:

```sh
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_RELEASE_OPT_LEVEL=1 \
  cargo +1.95.0 build --offline --locked --release --manifest-path experiments/html-host/Cargo.toml
```

Run `verify.py` through the owned thermal-run process group, passing `--binary`,
`--cef-path` and `--report`. It allocates a private X11 display and CEF profile;
CEF children stay inside that guarded process group. Raw workstation captures,
exact hashes, compiler/platform failures and successful observations are in
`reports/html-host-20261026/`. See `NOTICE` before distributing a runtime.
