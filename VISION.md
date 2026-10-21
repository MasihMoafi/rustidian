# Rust Notes

A native Rust desktop notebook for Masih, using ordinary Markdown folders and an Obsidian-style interface. User requested on 20 October 2026: a fully Rust implementation, familiar Obsidian appearance, and faster everyday use.

## Core thesis

Masih narrowed the destination on 21 October: fast essential Markdown editing, checklists, highlighting, useful Ctrl+P commands, familiar working panels and small settings. Follow [the guide for starting over](docs/GUIDE.md); broad plugin/settings replication, graph and drawings are optional. Preserve older feature contracts as history.

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

Candidate app source `4dbb842` was built optimized by Actions39608458012 and passes128 Rust executions, Clippy, all focused native sections and six CPU conditions. It adds held-link navigation, correct queued Ctrl+Enter, generated native window controls, essential file menus, default development-folder exclusions and scrolled task clipping. The exact copied translation plan/spec passes five link/navigation groups; task overlays escape zero pixels into the chrome (822 before). The observer-free Ferrite comparison and six CPU conditions pass in Actions39627064113; full regressions stopped after77 groups at a reader-width fixture with Outline open. Its setup correction passes locally; the next full run is in `ES.md`. The default executable remains the older b9 debug app until release gates pass. No local Rust compilation or `target/` exists.

Previous fully verified app source `21a1271` passes 113 Rust executions/108 unique, Clippy/formatting and 96 full native GUI groups on Actions 39540024536. The downloaded release also passes eight local bookmark groups without compilation. File bookmarks support Unicode titles, ordering, immediate per-vault state, rename tracking, restart restoration and safe missing-target handling; state-write failure rolls back the shortcut. Tables support source-preserving cell editing, keyboard navigation and structural actions; nested callouts support all thirteen types, folding, type changes and rich Reading content. Existing safe saves, navigation, images, properties/search, folding and generated controls remain covered. Exact binary/source/bundle hashes, logs and observations are in `reports/release-verification.json` and `reports/github-21a1271/`.

All six declared editor CPU conditions pass on the GitHub AMD EPYC 9V45 runner; the 103,300-byte edited medians are 4.8625 ms unfolded and 6.7405 ms mostly folded, with 25 inputs verified in each condition. These are CPU observations on that host, not display latency or a speedup over measurements on other hosts. Preserve failed evidence and the historical release verifications.

Masih's acceptance of the essential workflows remains pending. Full visual bidi/desktop IME and complex-document folding remain unresolved. Native Mermaid diagrams are implemented and covered by focused checks on the current candidate; math, graph/canvas, recovery UI, bookmark groups/non-file targets, daily notes/templates, plugin equivalents and sync/mobile are deferred under the latest priorities. Existing JavaScript plugins do not execute here. Forty-nine generated/SVG Studio symbols are integrated in the current candidate; five knowledge-action artworks are prepared only. Preserve `How are you?!` in demo Home and never automate writes to the real vault/profile. Elpis activation belongs to its recovery coordinator.

Builds and Rust checks run only in private `MasihMoafi/rust-notes` Actions. Local17GB `target/` was trashed; never recreate it. Local formatting and small checks are allowed. Actions uses Bash pipefail, required X11 runtime libraries, one-day artifacts and no persistent Rust cache. Keep only the needed app bundle and at least1GB of VPN allowance in reserve. This is our transfer policy, not a provider-side cap. Prefer direct GitHub access and preserve shared proxy routing. See `AGENTS.md` and `ES.md`.

## What this file is for

Orient the next agent without repeating discovery. Keep implementation evidence in `ES.md` and product requirements in `docs/PLAN.md`.
