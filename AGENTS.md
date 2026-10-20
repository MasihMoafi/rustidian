# Rust Notes resource rules

Masih requested on 21 October 2026: finish this existing app, use GitHub Actions for builds, delete local Rust build output, protect disk space, and keep 1 GB of VPN allowance in reserve.

- Run compilation, Rust tests, Clippy and the release GUI suite through `.github/workflows/native.yml`. Do not recreate the local `target/` directory. Local formatting and small source checks are allowed.
- Use the private `MasihMoafi/rust-notes` repository. Use `gh` for remote operations. Upload only project sources and required assets; ignore `target/` and local `reports/`.
- Keep Actions artifacts for one day and avoid persistent Rust build caches. Cancel superseded runs. Check storage before downloading an artifact; keep only the needed application bundle.
- Prefer direct GitHub access where it works; do not change shared proxy settings. Check the active provider's remaining allowance before substantial VPN transfers; stop our VPN transfers at the 1 GB reserve. A provider quota cannot be inferred from local traffic counters. Change connection only when necessary and within Masih's conditional authorization.
- Preserve all original vault, artwork and demo restrictions in `docs/PLAN.md` and `VISION.md`. Full Obsidian parity and Masih's acceptance remain unfinished; passing CI does not establish either.
