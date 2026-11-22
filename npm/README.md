# rustidian

This package installs and launches the Rustidian desktop notebook with npm. Rustidian is a native Rust Markdown notebook for local vaults on Linux.

The package supports Linux x86_64 only (Ubuntu 24.04 / glibc 2.39 or newer).

## Quick start

```sh
npm install -g rustidian
rustidian
```

Or run without installing globally:

```sh
npx rustidian
```

Expected result: the first run downloads and verifies the matching [GitHub release](https://github.com/MasihMoafi/rustidian/releases), then launches Rustidian. Later runs start immediately.

## How it works

The package contains only a small Node launcher and no bundled binary. On its first run the launcher downloads `rustidian-linux-x86_64.tar.gz` and `SHA256SUMS` from the matching GitHub release, verifies the archive's SHA-256 checksum against `SHA256SUMS`, and extracts the bundle into `~/.local/share/rustidian-npm/<version>/`. An archive whose checksum does not match is deleted and never run.

Set `RUSTIDIAN_NPM_HOME` to store the downloaded bundle in another directory. Removing that directory removes the binary; `npm uninstall -g rustidian` removes the launcher.

Limits:

- Platforms other than Linux x86_64 receive a clear error message.
- The package version matches the Rustidian GitHub release version it downloads.

## Evidence

`npm test` runs three tests against a local stand-in release server:

- A first run downloads and verifies `rustidian-linux-x86_64.tar.gz` against `SHA256SUMS`, extracts it, and runs `rustidian` with all arguments and exit status preserved. A second run downloads nothing.
- An archive with a mismatched checksum is refused and never executed.
- Another platform receives the Linux x86_64 error message.
