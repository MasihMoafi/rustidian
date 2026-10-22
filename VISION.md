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

Full parity and Masih's acceptance remain unfinished. docs/PARITY.md owns coverage. c7/run39734732429 completed FAILURE:227 Rust executions/220 unique, all12 early,21/22 focused and45/46 full workflows pass; page-preview save and Reading callout default-state failures remain mandatory repairs. CPU gates and the bounded Ferrite comparison pass. Exact optimized c7 passes the reported global styled-table defect on a private copy of the actual52KB legacy note and the sanitized wide-table regression; original bytes/mtime are unchanged. It is NOT installed. Evidence and all retained failure identities are in ES.md.

Masih clarified that the urgent "new" file/folder issue means opening EXISTING items via native File actions. Uploaded combined8e70bf3/run39758705958 adds Open file/Open folder pickers with shared CLI root policy, licensed general color emoji, native global/local graph views and the reviewed save/session/Undo guards. Formatting/Clippy pass; Rust checks caught changed ordinary-text metrics in the color-font fallback, under repair. Native and optimized verification have not run for this source. A bounded paired startup comparison is complete and retained under docs/performance/startup-2026-10-22; release preparation proceeds in parallel. No graph or emoji delivery claim until exact Actions/native checks pass. Personal AGENTS is open in a user-owned Rustidian preview for Masih to edit himself and must never enter Git. Matilda/rustidian holds unpublished marketing drafts; the renamed MasihMoafi/rustidian repository remains private. Existing JS plugins are unsupported; no browser/plugin runtime was added. ES.md records protected previews, exact current candidates and integration state.

Reference uses a read-only copy of the installed1.14.4 app update in a disposable profile on installer1.13.7. Actual General settings confirms1.14.4; the isolated inventory records30 plugins/213 commands/20 visible settings categories, daily/template flows and51 date formats. Earlier1.13.7 audits remain historical evidence; do not equate versions.

Two copied-vault previews are user-owned: reports/workspace-feedback-a104d89/preview.json and reports/feedback-9febfbb/preview.json. Never drive, mutate, close or clean either preview or its profile/vault. Masih accepted earlier Approved spec navigation and scrolled checkbox clipping; acceptance is not a whole-app claim. Existing JavaScript plugins do not execute here. Preserve selected generated/SVG Studio artwork, exact demo Home bytes including trailing space, real vault/profile and unrelated assets/app-icon work. Elpis activation belongs to its recovery coordinator.

All builds/heavy checks run in private MasihMoafi/rustidian Actions, with one-day artifacts and no persistent Rust cache. Local target is absent; its old17GB was moved to recoverable Trash, not claimed as physical reclamation. Preserve selected artifacts, rollback and raw evidence; trash obsolete owned rebuildables and check actual free space. The overnight guard forbids proxy downloads: use strict direct requests, preserve shared routing and the1GB VPN reserve. See AGENTS.md and ES.md.

## What this file is for

Orient the next agent without repeating discovery. Keep implementation evidence in `ES.md` and product requirements in `docs/PLAN.md`.
