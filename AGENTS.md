# Rust Notes resource rules

Use [reference-driven-product](../skills/reference-driven-product/SKILL.md) for the current reference, feedback, OSS comparison and rebuild-guide work. On21October2026 Masih expanded the target to every Obsidian feature: exercise the reference, implement the corresponding workflow, verify it, then repeat. [docs/PARITY.md](docs/PARITY.md) records coverage and remaining gaps. This supersedes the earlier optional graph/drawings/plugin scope. Keep all resource and data restrictions below.

Masih requested on 21 October 2026: finish this existing app, use GitHub Actions for builds, delete local Rust build output, protect disk space, and keep 1 GB of VPN allowance in reserve.

- Run compilation, Rust tests, Clippy and the release GUI suite through `.github/workflows/native.yml`. Do not recreate the local `target/` directory. Local formatting and small source checks are allowed.
- Use the private `MasihMoafi/rustidian` repository. Use `gh` for remote operations. Upload only project sources and required assets; ignore `target/` and local `reports/`.
- Keep Actions artifacts for one day and avoid persistent Rust build caches. Use one combined candidate: check the active run in ES.md before pushing concurrent changes. Keep independent work local until that run completes; cancel only for a known failure or a necessary superseding fix. Preserve every approved change when integrating. Check storage before downloading an artifact; keep only the needed application bundle.
- Prefer direct GitHub access where it works; do not change shared proxy settings. Check the active provider's remaining allowance before substantial VPN transfers; stop our VPN transfers at the 1 GB reserve. A provider quota cannot be inferred from local traffic counters. Change connection only when necessary and within Masih's conditional authorization.
- Preserve all original vault, artwork and demo restrictions in `docs/PLAN.md` and `VISION.md`. Full Obsidian parity and Masih's acceptance remain unfinished; passing CI does not establish either.

- User-facing installs must use the optimized Actions release executable, verified against release.sha256. Debug candidate archives are for disposable native checks only; their interaction latency is not release performance.
