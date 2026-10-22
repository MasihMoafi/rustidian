# Rustidian

A native Rust desktop notebook for Masih, formerly Rust Notes, using ordinary Markdown folders and an Obsidian-style interface. User requested on 20 October 2026: a fully Rust implementation, familiar Obsidian appearance, and faster everyday use.

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

## Honest state — 22 October 2026

Optimized source9febfbb is installed, verified by corrected harness ecad9cb in Actions39704845915. All172 Rust executions (166 unique),164 full GUI checks across40 workflows,15 focused workflows, six CPU conditions and the serial Ferrite comparison pass. Exact source/build/verification identities, executable/bundle hashes and protected paths are in reports/release-verification.json; raw final evidence is reports/github-ecad9cb/normal/. The former048fced executable remains the rollback. The original9feb normal run is retained as failed harness evidence; the unchanged source executable was accepted only after the corrected complete verification.

Installed features include the vault chooser/native Browse/shared recents, registered-file roots, daily notes/templates, named workspaces, recovery, unique/random notes, pinning and immediate copies, alongside earlier editing and file safety. The103,300-byte CPU medians on this verification host are1.980ms idle/11.4995ms edited unfolded and1.987ms/9.966ms folded. The bounded same-host serial edit/save comparison measured median86.49ms/p9592.20ms versus Ferrite0.3.0 at520.46ms/687.04ms. These are declared workloads, not display latency, cross-host comparisons or full-product superiority.

Full parity and Masih's acceptance remain unfinished. docs/PARITY.md owns coverage. Source9b5eafc/run39721548486 completed FAILURE: formatting/Clippy,209 Rust executions/202 unique,10 early and20 optimized focused workflows passed;43 full workflows passed and2 failed (portable-color command selection, explorer trailing-root/overscan click). CPU/comparison/package skipped. Its optimized7fc13350 executable remains evidence, not installed. The unchanged-binary explorer reproduction/calibration is reports/explorer-trailing-after-9b5eafc/verification.json. The next local candidate fixes the shared inline renderer for Masih's reported rgba/span/table fallback defect, promotes exact palette names, calibrates that real file click and uses the shared visible name Rustidian.46 full/22 focused/12 early workflows are prepared but uncompiled; no new pass is implied. The old app's native positive/negative rendering evaluation is reports/inline-styles-missing-9b5eafc/verification.json.

README and docs/brand/ use original generated/SVG Studio bookmark-R artwork and prominently identify a development preview with missing global/local graph, split/window/stacked tabs, full settings and existing JavaScript plugins. The repository remains private with no public release. Logo proof is reports/rustidian-logo/; it is an agent draft for user feedback. Graph is next after the formatting blocker; an actual1.14.4 audit is in progress. Font-control commit2d427aa is separate and uncompiled; shared-document ownership across duplicate views is also unfinished. No browser/plugin runtime was added. Native Rust versus existing JavaScript community-plugin compatibility remains an open decision.

Reference uses a read-only copy of the installed1.14.4 app update in a disposable profile on installer1.13.7. Actual General settings confirms1.14.4; the isolated inventory records30 plugins/213 commands/20 visible settings categories, daily/template flows and51 date formats. Earlier1.13.7 audits remain historical evidence; do not equate versions.

Two copied-vault previews are user-owned: reports/workspace-feedback-a104d89/preview.json and reports/feedback-9febfbb/preview.json. Never drive, mutate, close or clean either preview or its profile/vault. Masih accepted earlier Approved spec navigation and scrolled checkbox clipping; acceptance is not a whole-app claim. Existing JavaScript plugins do not execute here. Preserve selected generated/SVG Studio artwork, exact demo Home bytes including trailing space, real vault/profile and unrelated assets/app-icon work. Elpis activation belongs to its recovery coordinator.

All builds/heavy checks run in private MasihMoafi/rust-notes Actions, with one-day artifacts and no persistent Rust cache. Local target is absent; its old17GB was moved to recoverable Trash, not claimed as physical reclamation. Preserve selected artifacts, rollback and raw evidence; trash obsolete owned rebuildables and check actual free space. The overnight guard forbids proxy downloads: use strict direct requests, preserve shared routing and the1GB VPN reserve. See AGENTS.md and ES.md.

## What this file is for

Orient the next agent without repeating discovery. Keep implementation evidence in `ES.md` and product requirements in `docs/PLAN.md`.
