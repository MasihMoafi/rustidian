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

The native Linux editor implements source-preserving Live/Source/Reading/Split views, atomic conflict-aware saves, tabs/history, external refresh, wiki and Markdown navigation, guarded note/folder/image moves and system Trash, YAML properties/search, generated SVG controls, syntax colors, native reading tables and heading folding. Image tabs now support Fit/100%, refresh, links, history, clean restoration and byte-safe read-only behavior. The last verified release passes 94 executed Rust checks (89 unique) and 71 native GUI checks; exact build/source/procedure evidence is in `reports/release-verification.json`. The 103,300-byte editing CPU medians are 11.505ms unfolded and 14.5435ms mostly folded; all declared gates and 25-input checks pass. Keep the failed first folded timing as evidence. These are CPU UI measurements, not GPU/display latency or full-product superiority.

Full Obsidian appearance/product parity and Masih's acceptance remain unfinished. The visual-table candidate passes 103 Rust executions/98 unique and ten focused native GUI groups. Full regression and current performance acceptance remain pending. Callouts/math/diagrams, graph/canvas, file recovery, bookmarks/daily notes/templates, plugin equivalents, optional sync/mobile and full bidi/desktop IME still require work. Existing JavaScript plugins do not execute in this Rust app. The six new knowledge-action icons are generated, SVG Studio-traced and visually proved; they are not yet used by the executable. Preserve the user's `How are you?!` Setext heading in demo Home and never automate writes to the real vault/profile. Continue this project only; Elpis activation belongs to its recovery coordinator.

Masih's latest resource restriction: builds and Rust checks move to the private `MasihMoafi/rust-notes` GitHub Actions workflow; local `target/` was trashed (17 GB). Do not compile locally or recreate it. Local source/format checks remain allowed. The portable sample-vault startup and Actions workflow are new, pending CI. Existing performance reports are from the local machine and cannot be compared directly with GitHub runner timings. Keep Actions artifacts one day, no persistent build cache, and at least 1 GB of VPN allowance in reserve. Direct GitHub access works; do not change shared proxy routing. See `AGENTS.md` and current `ES.md`.

## What this file is for

Orient the next agent without repeating discovery. Keep implementation evidence in `ES.md` and product requirements in `docs/PLAN.md`.
