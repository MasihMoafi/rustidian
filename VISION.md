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

The ordinary launcher now uses optimized source12c2e241e6f2f3222840551a7aaf25969b5cb031, installed22October after exact-binary verification. reports/release-verification.json owns identity, rollback and all49 native workflow contracts (251 groups),260 Rust executions/249 unique,25 focused flows,3 preliminary flows and six CPU conditions. Actions39790060786 retains its actual failure: its preview test rejected two saved vault settings files even though the active value was correct. All48 other full workflows passed. The corrected preview acknowledgement passed the original14 groups locally after a real two-vault switch on the unchanged optimized executable; its reproduced before failure and after pass remain in reports/preview-session-verification. No Rust rebuild or duplicate heavy matrix was needed.

The verified installed update is open on a separate125-note vault copy: reports/feedback-graph-12c2e24/preview.json records the actual process/executable and protected handoff. The graph, existing-file/folder/vault workflows, global inline-formatting/emoji fixes and persistent creation controls are delivered for early-alpha review. A matched100-note warm-start comparison measured median complete note frames at0.61s Rustidian versus3.77s Obsidian1.14.4; reports/startup-current-candidate/run-002/report.md states the detector/software-display/default limitations and links raw pairs. This is workload-specific evidence, not a fastest-editor claim.
Masih accepted the visibly improved copied-vault graph in the new functional preview. Its exact executable, protected window and copied-note identities live in reports/feedback-graph-b283c7a/preview.json; it is DEBUG, not installed or benchmark-approved. That older preview and all other handed-off windows and their vault/profile files belong to Masih: read-only identity/screenshots only. The actual Obsidian1.14.4 reference was exercised in isolated profiles; docs/PARITY.md records remaining feature gaps. Full parity and final acceptance are unfinished; existing JavaScript plugins do not execute here.

The repository is private. Private vault copies and personal AGENTS stay local and excluded from upload; reports/private-note-audit.json records the exact-file/history check. Private release/Matilda/website preparation remains unpublished. Masih will activate his media agent after the tested alpha is ready; do not equate that milestone with complete Obsidian parity.

Rust builds/heavy matrices stay in Actions; focused native GUI checks use disposable local files. No local target exists. Preserve the selected executable, rollback, raw evidence, original demo bytes and unrelated assets/app-icon. Keep direct-only transfers, shared routing and the1GB VPN reserve. ES.md owns current work and exact candidate transitions.

## What this file is for

Orient the next agent without repeating discovery. Keep implementation evidence in `ES.md` and product requirements in `docs/PLAN.md`.
