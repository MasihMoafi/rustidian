<div align="center">

<img src="docs/brand/rustidian.svg" alt="Rustidian bookmark R logo" width="112" height="112">

# Rustidian

**Obsidian, but fast. A native Rust Markdown notebook.**

[Status](#development-status) · [Quick start](#quick-start) · [Features](#why-rustidian) · [Performance](#performance) · [Roadmap](docs/PARITY.md)

Development preview · Full Obsidian parity is unfinished

</div>

A familiar vault workflow, from the file explorer and `[[wiki links]]` to live editing and backlinks. Open an ordinary Markdown folder and keep your notes portable.

## Development status

Rustidian is a work in progress toward a complete, fast Obsidian-style app. The current build covers a subset of that goal.

| Area | Current state |
| --- | --- |
| Markdown editing, links, tasks, tables, Mermaid | Implemented workflows with ongoing parity work |
| Vaults, daily notes, templates, workspaces, recovery | Implemented workflows with remaining gaps |
| Global and local graph | Native views implemented; combined candidate verification pending |
| Canvas, Bases, math, full settings and hotkeys | Missing or incomplete |
| Existing Obsidian community plugins | Unsupported; compatibility design remains open |

Editing, navigation and note safety were built first. That order is documented in [the plan](docs/PLAN.md); the [coverage ledger](docs/PARITY.md) records the full scope and remaining work.

![Current Rustidian development build, still displaying its former Rust Notes name](docs/images/rust-notes.png)

## Why Rustidian?

- **Native throughout.** Rust powers the interface, text editor, Markdown renderer, search, and vault operations.
- **Write your way.** Live editing, source and reading modes, folding, formatting, find/replace, and undo. Tables, callouts, images, syntax highlighting, and native Mermaid diagrams bring your notes to life.
- **Follow your ideas.** Wiki links, backlinks, tags, aliases, properties, bookmarks, a quick switcher, and vault-wide search keep your knowledge connected.
- **Return to your rhythm.** Daily notes, templates, pinned tabs, named workspaces, recent vaults, and tab restoration after a clean exit.
- **Keep control of your files.** Plain UTF-8 Markdown and text, atomic saves, external-change protection, recovery snapshots, and recoverable Trash. Workspace state lives outside the vault.

## Quick start

The current preview is built and verified on **Linux x86_64 / X11**.

1. Open [Native Rust Actions](https://github.com/MasihMoafi/rustidian/actions/workflows/native.yml) and choose a **passing full verification run**.
2. Download the final `rust-notes-linux-<commit>` artifact and extract its ZIP. Use the `rust-notes-linux-x86_64.tar.gz` inside; earlier candidate and debug artifacts are for testing.
3. Extract the application bundle and run it from that directory:

```sh
tar -xzf rust-notes-linux-x86_64.tar.gz
./rust-notes
```

The included `demo-vault` opens by default. Keep it beside the executable, or pass your own folder or document:

```sh
./rust-notes "/path/to/vault"
./rust-notes "/path/to/note.md"
./rust-notes "/path/to/document.txt"
```

The File menu provides **Open file…** and **Open folder…**; the separate vault chooser keeps recent vaults and creation.

Artifacts expire after **one day** and currently require access to the private repository. If none remain, a maintainer can start a new full run from the workflow page.

Edits autosave after 600 ms of inactivity; **Ctrl+S** saves immediately. If another program changes a note, Rustidian retains your buffer and refuses to overwrite the external change. Use **Save a copy** from the command palette to preserve your version.

| Shortcut | Action |
| --- | --- |
| Ctrl+P | Command palette |
| Ctrl+O | Quick switcher |
| Ctrl+Shift+O | Open an existing file |
| Ctrl+Alt+O | Open an existing folder as a vault |
| Ctrl+N | New note |
| Ctrl+E | Switch reading/editing |
| Ctrl+Shift+F | Search the vault |
| Ctrl+S | Save |

## Performance

A [verified same-host comparison](https://github.com/MasihMoafi/rustidian/actions/runs/39704845915) measured serial edit/save acknowledgement:

| Editor | Median |
| --- | ---: |
| **Rustidian (measured as Rust Notes)** | **86.49 ms** |
| Ferrite 0.3.0 | 520.46 ms |

**About 6× faster in this edit-and-save test.** Both editors used the same 103,300-byte document in source mode on one Linux/Xvfb host. Four alternating rounds collected 100 timed samples plus 20 warmups. Timings cover synthetic input through observing the exact saved bytes, including keyboard-driver and file-polling overhead. Overall speed varies by workload.

A separate [paired startup comparison](docs/performance/startup-2026-10-22/README.md) measured a median first-note capture of **0.24 s for the Rustidian c7 development preview versus 3.91 s for Obsidian 1.14.4**. It used ten warmed same-host pairs, 100 synthetic notes, Xvfb and screenshot/OCR sampling. This measures a small preview workload; the exact tested versions, original screenshots, failures and limits accompany the results.

The linked run contains the verification evidence. See the [comparison script](scripts/compare-editors.py) for the method and the [workflow](.github/workflows/native.yml) for the checks.

## Project status

Rustidian is an **independent native implementation**, unaffiliated with Obsidian. Full feature and visual parity is the goal and remains unfinished. Existing Obsidian JavaScript community plugins do not execute here.

Graph parity, Canvas, math, sync, broader window/tab behavior, and other platforms still need work. The [coverage ledger](docs/PARITY.md) records implemented workflows and remaining gaps; the [development guide](docs/GUIDE.md) explains how to continue the project.

## Contributors welcome

I started Rustidian because I never found a note editor I liked that felt fast. Obsidian still felt slow to start after several optimization attempts, so I began building the native Rust notebook I wanted to use.

Help shape it early: Rust/egui development, Markdown editing, file safety, accessibility, platform support and reproducible bug reports all matter. See [CONTRIBUTING.md](CONTRIBUTING.md) and the [coverage ledger](docs/PARITY.md).

[MIT license](LICENSE) · Independent project, unaffiliated with Obsidian.
