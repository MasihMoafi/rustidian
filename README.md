<div align="center">

# ⚡ Rust Notes — Obsidian, but Fast

**A blazingly fast Markdown notebook, written in Rust.**

[Quick start](#quick-start) · [Features](#why-rust-notes) · [Performance](#performance) · [Roadmap](docs/PARITY.md)

Linux preview · Active development

</div>

A familiar vault workflow, from the file explorer and `[[wiki links]]` to live editing and backlinks. Open an ordinary Markdown folder and keep your notes portable.

![Rust Notes desktop preview](docs/images/rust-notes.png)

## Why Rust Notes?

- **Native throughout.** Rust powers the interface, text editor, Markdown renderer, search, and vault operations.
- **Write your way.** Live editing, source and reading modes, folding, formatting, find/replace, and undo. Tables, callouts, images, syntax highlighting, and native Mermaid diagrams bring your notes to life.
- **Follow your ideas.** Wiki links, backlinks, tags, aliases, properties, bookmarks, a quick switcher, and vault-wide search keep your knowledge connected.
- **Return to your rhythm.** Daily notes, templates, pinned tabs, named workspaces, recent vaults, and tab restoration after a clean exit.
- **Keep control of your files.** Plain UTF-8 Markdown and text, atomic saves, external-change protection, recovery snapshots, and recoverable Trash. Workspace state lives outside the vault.

## Quick start

The current preview is built and verified on **Linux x86_64 / X11**.

1. Open [Native Rust Actions](https://github.com/MasihMoafi/rust-notes/actions/workflows/native.yml) and choose a **passing full verification run**.
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

Artifacts expire after **one day** and currently require access to the private repository. If none remain, a maintainer can start a new full run from the workflow page.

Edits autosave after 600 ms of inactivity; **Ctrl+S** saves immediately. If another program changes a note, Rust Notes retains your buffer and refuses to overwrite the external change. Use **Save a copy** from the command palette to preserve your version.

| Shortcut | Action |
| --- | --- |
| Ctrl+P | Command palette |
| Ctrl+O | Quick switcher |
| Ctrl+N | New note |
| Ctrl+E | Switch reading/editing |
| Ctrl+Shift+F | Search the vault |
| Ctrl+S | Save |

## Performance

A [verified same-host comparison](https://github.com/MasihMoafi/rust-notes/actions/runs/39704845915) measured serial edit/save acknowledgement:

| Editor | Median |
| --- | ---: |
| **Rust Notes** | **86.49 ms** |
| Ferrite 0.3.0 | 520.46 ms |

**About 6× faster in this edit-and-save test.** Both editors used the same 103,300-byte document in source mode on one Linux/Xvfb host. Four alternating rounds collected 100 timed samples plus 20 warmups. Timings cover synthetic input through observing the exact saved bytes, including keyboard-driver and file-polling overhead. Overall speed varies by workload; equivalent-feature Obsidian timings remain unmeasured.

The linked run contains the verification evidence. See the [comparison script](scripts/compare-editors.py) for the method and the [workflow](.github/workflows/native.yml) for the checks.

## Project status

Rust Notes is an **independent native implementation**, unaffiliated with Obsidian. Full feature and visual parity is the goal and remains unfinished. Existing Obsidian JavaScript community plugins do not execute here.

Graph, Canvas, math, sync, broader window/tab behavior, and other platforms still need work. The [coverage ledger](docs/PARITY.md) records implemented workflows and remaining gaps; the [development guide](docs/GUIDE.md) explains how to continue the project.
