# Rust Notes

A native Rust desktop notebook for Masih, using ordinary Markdown folders and an Obsidian-style interface. User requested on 20 October 2026: a fully Rust implementation, familiar Obsidian appearance, and faster everyday use.

## Core thesis

Keep the vault portable and the interface responsive. Native Rust is the implementation constraint; measured performance, safe editing, and familiar behavior are the acceptance criteria. Changing Linux distributions is not a dependency.

## Map

- `src/`: native desktop interface and vault/file operations.
- `tests/`: behavioral checks for vault safety and links.
- `docs/PLAN.md`: authoritative scope, stages, and acceptance criteria.
- `ES.md`: current work, evidence, and remaining issues.
- `demo-vault/`: disposable demonstration notes, never Masih's real notes.
- `reports/`: verification evidence and screenshots.
- `assets/icons/`: generated raster originals, prompts, SVG Studio vector output, and proof sheets. Follow its README; do not replace these with hand-drawn icon paths.
- `scripts/`: image-to-vector preparation and proof tooling; not application runtime dependencies.

## Honest state — 21 October 2026

Verified app source `7e2a903` passes107 Rust executions/102 unique, Clippy/formatting and89 full native GUI groups on Actions39527354572. The downloaded release also passes10 local callout groups without compilation. Tables now support source-preserving cell editing, keyboard navigation and structural actions; nested callouts support all thirteen types, folding, type changes and rich Reading content. Existing safe saves, navigation, images, properties/search, folding and generated controls remain covered. Exact binary/source/bundle hashes, logs and observations are in `reports/release-verification.json` and `reports/github-7e2a903/`.

All six declared editor CPU conditions pass on the GitHub AMD EPYC7763 runner; the103,300-byte edited medians are8.6975ms unfolded and10.328ms mostly folded, with25 inputs verified in each condition. These are CPU observations on that host, not display latency or a speedup over historical local measurements. Preserve failed evidence and the historical image/folding verification.

Full Obsidian appearance/product parity and Masih's acceptance remain unfinished. Math/diagrams, graph/canvas, recovery, bookmarks/daily notes/templates, plugin equivalents, optional sync/mobile, complex-document folding and full bidi/desktop IME remain. Existing JavaScript plugins do not execute here. Forty-four generated/SVG Studio symbols are integrated; six knowledge-action artworks are prepared only. Preserve `How are you?!` in demo Home and never automate writes to the real vault/profile. Elpis activation belongs to its recovery coordinator.

Builds and Rust checks run only in private `MasihMoafi/rust-notes` Actions. Local17GB `target/` was trashed; never recreate it. Local formatting and small checks are allowed. Actions uses Bash pipefail, required X11 runtime libraries, one-day artifacts and no persistent Rust cache. Keep only the needed app bundle and at least1GB of VPN allowance in reserve. This is our transfer policy, not a provider-side cap. Prefer direct GitHub access and preserve shared proxy routing. See `AGENTS.md` and `ES.md`.

## What this file is for

Orient the next agent without repeating discovery. Keep implementation evidence in `ES.md` and product requirements in `docs/PLAN.md`.
