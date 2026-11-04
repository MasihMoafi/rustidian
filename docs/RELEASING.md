# Releasing Rustidian

A release needs a version bump, one verified commit and a few Actions runs, in this order. Rust builds and tests run on
GitHub, not on this machine. Run every network command without the proxy variables (expensive network mode: downloads
never use the VPN); `scripts/preflight-release.py` and `scripts/upload-snap.sh` already do.

1. **Version.** Bump `version` in `Cargo.toml` and `Cargo.lock` (the `rustidian` entry), `pkgver` in
   `packaging/aur-bin/PKGBUILD` and the `<release>` in `packaging/rustidian.metainfo.xml`. Rewrite the release notes
   in `scripts/publish-checked-release.py`: they are hard-coded text for one release.
2. **Local lint.** `python3 scripts/lint-packaging.py` (about a second). It checks the snap recipe, AppImage layout,
   AppStream and desktop files, workflow pins, licence notices, brand icons, personal paths and that the quick CI check
   runs the same clippy command as the full run.
3. **Quick CI.** Push the branch. `native.yml` (fmt, clippy on every target with `-D warnings`, tests) and
   `packages.yml` (lint, snap, AppImage) must pass.
4. **Full verification.** `gh workflow run ux.yml --ref BRANCH`. It runs clippy, tests and the GUI checks (about 30 min).
5. **Verified package.** `gh workflow run ux.yml --ref BRANCH -f gui_only=true -f package_after_checks=true`.
   Only this run creates the `rustidian-verified-<commit>` artifact that publishing needs. Artifacts live one day.
6. **Preflight.** `python3 scripts/preflight-release.py RUN_ID_OF_STEP_5`. Fix every FAIL before going on.
7. **Publish.** `gh workflow run ux.yml --ref BRANCH -f publish_run=RUN_ID_OF_STEP_5`. It creates the release and
   fast-forwards `main`. `main` must be an ancestor of the branch: cherry-pick onto the current `main`, never merge.
8. **Snap.** `scripts/upload-snap.sh PACKAGES_RUN_ID edge`, run `store-smoke.yml`, then
   `snapcraft release rustidian REVISION stable`.
9. **AUR.** Put the tarball's SHA-256 from the release into `packaging/aur-bin/PKGBUILD`, then push to the AUR.

Known traps, each one a failure we had: the full run does not create the verified artifact (step 5); a branch cut from the
old public `main` fails clippy until the egui rename fixes are in; the app once wrote `.rustidian/agent-context.md` into every
opened folder and failed two GUI checks; the snapcraft `rust` plugin cannot find `rustup`, so the recipe installs Rust itself;
appimagetool validates the AppStream file strictly.
