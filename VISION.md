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

## Honest state — 22 October2026,01:15UTC

Optimized source048fced is installed, verified by normal Actions39658870633/harness e326f14:133 Rust executions/127 unique,115 full native GUI groups across all32 sections, six CPU conditions and the normal Ferrite comparison pass. It includes the vault chooser/native Browse/safe creation/shared recents, registered-file roots, link/checklist fixes and the earlier editing/safety features. Exact binary/bundle hashes and raw evidence are in reports/release-verification.json and reports/github-e326f14/. The exact former b9 executable remains the rollback. The21a receipt is historical under reports/github-21a1271/.

The103,300-byte editor CPU medians are8.7885ms unfolded and10.6105ms mostly folded; all six conditions verified25 insertions each. On the same Actions host, the bounded synthetic edit/save comparison measured median90.3ms/p95107.9ms versus Ferrite0.3.0's524.3ms/543.1ms. These describe the declared workloads, not display latency, performance across hosts or full-product superiority.

Full parity and Masih's acceptance remain unfinished. docs/PARITY.md owns the coverage ledger. Exact optimized source9febfbb includes daily/templates, named workspaces, recovery, unique/random notes, pinning and immediate file copies; it is not installed. Its normal Actions39698974654 passes172 Rust executions/166 unique and all five early workflows. Fourteen of15 optimized focused sections pass; Workspaces fails an obsolete expected dictionary that omits the newly saved pinned:false field. The immutable negative records the actual correct path/modes/flag. All10 corrected focused workspace checks, including actual pinned/unpinned layout restoration, pass locally on the unchanged optimized app. All7 focused optimized tab checks and eight inspected screenshots are retained. Normal full40/portable now completed with38 passing and two fixture failures: that workspace schema and a Source-mode request on a blank tab. Actual1.14.4 confirms mode commands are hidden there. All8 corrected focused keyboard checks pass on the unchanged app. Six CPU conditions and the Ferrite comparison pass. Complete one corrected harness-only reuse verification of every retained gate before installation. ES.md owns exact identities, raw failures and current status.

Independent next Move picker/backend/menu/palette source is committed only on work/file-move-20261022 in /home/masih/Desktop/p/.worktrees/rust-notes-file-move (7847469/69f4428/1bf4bcd). It is uncompiled/unuploaded. Ten FS behavior cases, search/tab identity cases and ten Move assertions plus startup are prepared; keep it isolated until the current release verification is resolved. General Rename preference, split groups/windows/stacked tabs, complete context menus/settings and many core/community features remain unfinished.

Reference now uses a read-only copy of the already installed1.14.4 app update in a disposable profile; actual General settings confirms1.14.4 on installer1.13.7. The new isolated inventory passes30 plugins/213 commands/20 visible settings categories, daily/template flows and51 date formats. Eight paired Move observations and six native highlight syntax/Reading/undo observations pass. Earlier1.13.7 audits remain historical evidence; do not equate versions. Native support for the six new emoji-marker highlight syntaxes remains missing.

An a104d89 copied-vault preview was handed to Masih (reports/workspace-feedback-a104d89/preview.json); never drive, mutate or close it. The former048fced preview exited. Masih accepted earlier Approved spec navigation and scrolled checkbox clipping; acceptance is not a whole-app claim. Existing JavaScript plugins do not execute here. Preserve the selected generated/SVG Studio artwork, exact demo Home bytes including trailing space, real vault/profile and unrelated assets/app-icon work. Elpis activation belongs to its recovery coordinator.

All builds/heavy checks run in private MasihMoafi/rust-notes Actions, with one-day artifacts and no persistent Rust cache. Local target is absent; its old17GB was moved to recoverable Trash, not claimed as physical reclamation. Preserve selected artifacts, rollback and raw evidence; trash obsolete owned rebuildables and check actual free space. An active overnight guard forbids proxy downloads: use strict direct requests, preserve shared routing and the1GB VPN reserve. See AGENTS.md and ES.md.

## What this file is for

Orient the next agent without repeating discovery. Keep implementation evidence in `ES.md` and product requirements in `docs/PLAN.md`.
