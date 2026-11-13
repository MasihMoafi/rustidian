<div align="center">

# Rustidian

<table>
  <tr>
    <td align="center"><img src="docs/brand/themes/signature.png" width="112" height="112" alt="Rustidian signature pink logo"><br><sub>Main · Signature</sub></td>
    <td align="center"><img src="docs/brand/rustidian.png" width="112" height="112" alt="Rustidian teal logo"><br><sub>Teal</sub></td>
    <td align="center"><img src="docs/brand/themes/love-space.png" width="112" height="112" alt="Rustidian Love Space logo"><br><sub>Love Space</sub></td>
    <td align="center"><img src="docs/brand/themes/elpis-gold.png" width="112" height="112" alt="Rustidian Elpis Gold logo"><br><sub>Elpis Gold</sub></td>
  </tr>
</table>

**A native Markdown notebook for Linux. Your files, your folders, no account.**

Opens Obsidian-format vaults and started faster than Obsidian in our [paired test](docs/performance/startup-2026-10-22/README.md) (0.61 s vs 3.77 s, earlier build, one setup). Independent, not affiliated with Obsidian.

**[Install for Linux — download .deb](https://github.com/MasihMoafi/rustidian/releases/latest/download/rustidian-linux-amd64.deb)**

[Portable Linux download](https://github.com/MasihMoafi/rustidian/releases/latest/download/rustidian-linux-x86_64.tar.gz) · [Quick start](#quick-start) · [Report a bug](https://github.com/MasihMoafi/rustidian/issues/new/choose) · [Contribute](CONTRIBUTING.md)

Linux desktop · Plain files · Native Rust

</div>

I built Rustidian for myself because every note editor I tried felt too slow. **Speed and stability** guide the project: ordinary Markdown folders, native editing, linked notes and graphs.

**Try Rustidian for Linux. Contributors are welcome.**

<picture>
  <source media="(prefers-color-scheme: light)" srcset="docs/images/rustidian-light.png">
  <img src="docs/images/rustidian.png" alt="Rustidian showing its bundled Markdown notebook">
</picture>

## Quick start

**Requirements:** Linux x86_64 with **glibc 2.39 or newer** (Ubuntu 24.04 or newer; check with `ldd --version`) and an X11 or Wayland desktop. The Open dialogs use `zenity`, which the package installs; with the portable archive, install `zenity` with your package manager. Native UI checks use X11; Wayland is enabled, with broader coverage still open.

Download the **[Debian/Ubuntu package](https://github.com/MasihMoafi/rustidian/releases/latest/download/rustidian-linux-amd64.deb)** and [checksums](https://github.com/MasihMoafi/rustidian/releases/latest/download/SHA256SUMS), then run:

```sh
sha256sum --check --ignore-missing SHA256SUMS
sudo apt install ./rustidian-linux-amd64.deb
rustidian
```

**Other ways to install.** Snap: `sudo snap install rustidian`; the snap is confined, so it reads vaults in your home folder, and `sudo snap connect rustidian:removable-media` lets it open vaults on other drives. AppImage: download `Rustidian-0.1.6-x86_64.AppImage` from the [release](https://github.com/MasihMoafi/rustidian/releases/latest), run `chmod +x` on it and start it. Arch and Omarchy: `git clone https://github.com/MasihMoafi/rustidian && cd rustidian/packaging/aur-bin && makepkg -si` builds a native package from the release's exact executable.

The package adds Rustidian to your applications menu and opens a writable demo vault on first launch. Existing notes and settings are retained. The installer is built and checked on Ubuntu 24.04.

For a portable installation, download the [tar archive](https://github.com/MasihMoafi/rustidian/releases/latest/download/rustidian-linux-x86_64.tar.gz) and the same checksums:

```sh
sha256sum --check --ignore-missing SHA256SUMS
mkdir rustidian && tar -xzf rustidian-linux-x86_64.tar.gz -C rustidian
cd rustidian && ./rustidian
```

The bundled demo opens immediately. To add Rustidian to your applications menu, run `python3 install.py`; it installs for your user without root. You can also run it from the extracted folder:

```sh
./rustidian "/path/to/vault copy"
./rustidian "/path/to/note.md"
./rustidian --version
```

**Opens Obsidian-format vaults.** Try a copy of your vault first. Open the folder directly with **File → Open folder…**; no import or account is needed. Rustidian reads ordinary Markdown files and selected Obsidian settings. Community plugins do not run; Canvas, Bases and other gaps are listed below. Keep Obsidian available for other workflows.

**Why don't Obsidian plugins work?** Rustidian's interface is native Rust with egui/eframe. Obsidian plugins require Obsidian's JavaScript API and often its browser DOM and CSS. Running them needs a compatibility runtime or individual feature ports; copying plugin folders is insufficient.

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

## Why Rustidian?

- **Native throughout.** Rust powers the interface, text editor, Markdown renderer, search, and vault operations.
- **Write your way.** Live editing, source and reading modes, folding, formatting, find/replace, and undo. Tables, callouts, images, syntax highlighting, LaTeX math and native Mermaid diagrams bring your notes to life.
- **Follow your ideas.** Wiki links, backlinks, tags, aliases, properties, bookmarks, a quick switcher, and vault-wide search keep your knowledge connected.
- **Return to your rhythm.** Daily notes, templates, pinned tabs, named workspaces, recent vaults, and tab restoration after a clean exit.
- **Work beside an agent.** The Agent tab (Ctrl+Shift+A) runs Codex, Claude Code or Elpis in a terminal in your vault folder, whichever is installed.
- **Keep control of your files.** Plain UTF-8 Markdown and text, atomic saves, external-change protection, recovery snapshots, and recoverable Trash. Workspace state lives outside the vault.

## Development status

Rustidian is a work in progress toward full support for Obsidian-format vaults. The current build covers a subset of that goal.

| Area | Current state |
| --- | --- |
| Markdown editing, links, tasks, tables, Mermaid, LaTeX math | Implemented workflows with ongoing parity work |
| Vaults, daily notes, templates, workspaces, recovery | Implemented workflows with remaining gaps |
| Global and local graph | Native views verified; graph parity work continues |
| Canvas, Bases, full settings and hotkeys | Missing or incomplete |
| Existing Obsidian community plugins | Unsupported; compatibility design remains open |

The [coverage ledger](docs/PARITY.md) records implemented workflows and remaining work. Full parity and existing Obsidian plugin compatibility are unfinished.

## Performance

These measurements use older builds. The current release has not yet been benchmarked against Obsidian.

A [paired startup comparison](docs/performance/startup-2026-10-22/README.md) measured a median complete matching note frame at **0.61 s for the verified Rustidian baseline versus 3.77 s for Obsidian 1.14.4**. Ten balanced warmed pairs used 100 synthetic notes, a 1440×960 software display and screenshot/OCR sampling. The measured baseline precedes the later graph update. Exact versions, original paired screenshots, raw results and limits accompany this bounded result.

A [verified same-host comparison](https://github.com/MasihMoafi/rustidian/actions/runs/39817952278) measured serial edit/save acknowledgement:

| Editor | Median |
| --- | ---: |
| **Rustidian measured baseline** | **78.57 ms** |
| Ferrite 0.3.0 | 439.32 ms |

**About 5.6× faster in this edit-and-save test.** Both editors used the same 103,300-byte document in source mode on one Linux/Xvfb host. Four alternating rounds collected 100 timed samples plus 20 warmups. Timings cover synthetic input through observing the exact saved bytes, including keyboard-driver and file-polling overhead. Overall speed varies by workload.

The linked run contains the verification evidence. See the [comparison script](scripts/compare-editors.py) for the method and the [workflow](.github/workflows/native.yml) for the checks.

## Contributors welcome

Try one useful workflow: open a copied vault, edit and save a note, follow a link, then reopen it. [Report what broke](https://github.com/MasihMoafi/rustidian/issues/new?template=bug_report.yml) with `rustidian --version`, your Linux/display setup and a small example. For a difference from Obsidian, use the [workflow gap form](https://github.com/MasihMoafi/rustidian/issues/new?template=parity_gap.yml).

Rust/egui contributors can start with Markdown rendering, keyboard access or Linux/Wayland testing. [CONTRIBUTING.md](CONTRIBUTING.md) explains setup and checks; the [coverage ledger](docs/PARITY.md) records remaining work. If Rustidian is useful to you, a [GitHub star](https://github.com/MasihMoafi/rustidian) helps others discover it.

[MIT license](LICENSE) · [Third-party notices](THIRD_PARTY_NOTICES.txt) · Independent, not affiliated with Obsidian. Existing Obsidian JavaScript plugins are unsupported; full feature and visual parity remain unfinished.

<!-- more-projects:start -->
## More projects

- [Elpis](https://github.com/MasihMoafi/Elpis): a coding agent for the terminal and desktop, with explicit context control
- [rag-mcp](https://github.com/MasihMoafi/rag-mcp): local hybrid search over your documents, as a CLI and MCP server
- [Skills](https://github.com/MasihMoafi/skills): SKILL.md workflows for coding agents
- [WhisperType](https://github.com/MasihMoafi/WhisperType): local voice input for developers
- [Read Aloud](https://github.com/MasihMoafi/read-aloud): select text, press Super+R, hear it in a local voice
- [Love Spot](https://github.com/MasihMoafi/love-spot): a shared drawing board where characters talk, draw and compete with you
- [SVG Studio](https://svg.masihmoafi.com): high-quality image vectorization
- [Pin](https://github.com/MasihMoafi/pin): an always-on-top priority note, saved as plain Markdown
- [Clocki](https://github.com/MasihMoafi/clocki): a floating desktop timer for tracking focus hours

All of them: [masihmoafi.com](https://masihmoafi.com/projects)
<!-- more-projects:end -->
