# Rust Notes

A native Rust desktop notebook for Masih, using ordinary Markdown folders and an Obsidian-style interface. User requested on 20 October 2026: a fully Rust implementation, familiar Obsidian appearance, and faster everyday use.

## Core thesis

Masih's latest21October instruction expands the destination to every Obsidian feature with equivalent behavior and appearance and faster interaction. This supersedes the earlier narrowed optional-feature scope. Follow [the guide for starting over](docs/GUIDE.md) and its single [coverage ledger](docs/PARITY.md): exercise the installed reference, implement the corresponding workflow, verify both, then repeat. Preserve older feature contracts as history.

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

## Honest state — 21 October2026,19:34UTC

Optimized source048fced is installed, verified by normal Actions39658870633/harness e326f14:133 Rust executions/127 unique,115 full native GUI groups across all32 sections, six CPU conditions and the normal Ferrite comparison pass. It includes the vault chooser/native Browse/safe creation/shared recents, registered-file roots, link/checklist fixes and the earlier editing/safety features. Exact binary/bundle hashes and raw evidence are in reports/release-verification.json and reports/github-e326f14/. The exact former b9 executable remains the rollback. The21a receipt is historical under reports/github-21a1271/.

The103,300-byte editor CPU medians are8.7885ms unfolded and10.6105ms mostly folded; all six conditions verified25 insertions each. On the same Actions host, the bounded synthetic edit/save comparison measured median90.3ms/p95107.9ms versus Ferrite0.3.0's524.3ms/543.1ms. These describe the declared workloads, not display latency, performance across hosts or full-product superiority.

Full parity and Masih's acceptance remain unfinished. docs/PARITY.md records every core plugin and other gaps. Current uncompiled source adds daily notes/templates/date/time commands, read-only import of their existing settings, native per-vault preferences and safe editor insertion/undo. The isolated Obsidian1.13.7 baseline passes create/reopen and insert/undo/redo flows and supplies51 date-format fixtures. This new increment still needs its own optimized Actions build, native checks and performance gates before installation.

A separate048fced copied-vault preview was handed to Masih; never drive, mutate or close it. Masih accepted earlier Approved spec navigation and scrolled checkbox clipping; acceptance is not a whole-app claim. Existing JavaScript plugins do not execute here. Preserve the selected generated/SVG Studio artwork, exact demo Home bytes including trailing space, real vault/profile and unrelated assets/app-icon work. Elpis activation belongs to its recovery coordinator.

All builds/heavy checks run in private MasihMoafi/rust-notes Actions, with one-day artifacts and no persistent Rust cache. Local target is absent; its old17GB was moved to recoverable Trash, not claimed as physical reclamation. Preserve selected artifacts, rollback and raw evidence; trash obsolete owned rebuildables and check actual free space. An active overnight guard forbids proxy downloads: use strict direct requests, preserve shared routing and the1GB VPN reserve. See AGENTS.md and ES.md.

## What this file is for

Orient the next agent without repeating discovery. Keep implementation evidence in `ES.md` and product requirements in `docs/PLAN.md`.
