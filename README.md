# Rust Notes

A working native Rust Markdown notebook with an Obsidian-style layout and Masih's charcoal/teal theme. Its UI, text editor, Markdown renderer, search, and vault operations are Rust. There is no Electron, embedded browser, JavaScript UI, or server.

This project is still being completed. The authoritative scope and next stages are in [the plan](docs/PLAN.md); current verification is in [ES.md](ES.md).

## Run

From the extracted Actions application bundle:

```sh
# Included sample vault
./rust-notes

# Your existing Markdown folder
./rust-notes "/home/masih/Desktop/o/Main Vault"

# An individual Markdown or plain-text document
./rust-notes "/path/to/document.md"
./rust-notes "/path/to/document.txt"

# Set the vault used when launching without a file
RUST_NOTES_DEFAULT_VAULT="/home/masih/Desktop/o/Main Vault" ./rust-notes

# Index a vault without opening a window or modifying files
./rust-notes --check-vault "/path/to/vault"
```

Builds now run in the private [GitHub Actions workflow](https://github.com/MasihMoafi/rust-notes/actions/workflows/native.yml). Download the application artifact from a passing run, extract `rust-notes-linux-x86_64.tar.gz`, and run `./rust-notes` beside its included `demo-vault`, or pass your own vault path. Local Rust build output has been removed; do not rebuild locally. CI avoids persistent build caches and keeps its artifacts for one day.

The included sample is deliberately separate from the real vault. Opening a vault does not write note content. Edits autosave after 600 ms of inactivity; Ctrl+S saves immediately. External conflicts retain the buffer and refuse an overwrite. Workspace layout is stored outside the vault under XDG_STATE_HOME (or ~/.local/state/rust-notes).

## Working now

- Resizable file explorer, tabs, outline, backlinks, and outgoing links.
- Image-generated, SVG Studio-traced icons, with distinct glyphs for notes, tabs, reading, editing, and menus. [Artwork and proof](assets/icons/README.md).
- Dark and light themes; the right sidebar collapses at compact widths.
- Source-preserving inline Markdown editing, source mode, reading preview with proper font weights, split view, formatting, undo/redo, and find/replace.
- Native YAML properties, aliases/tags and advanced vault search; local/remote images in reading and live views, width hints, attachment refresh, and two-way task checkboxes.
- Read-only image tabs from the explorer, quick switcher and wiki/Markdown links, with Fit/100% zoom, refresh, history, restart restoration, guarded rename/link updates and recoverable Trash.
- Filename/content search, quick switcher, command palette, new notes, and vault switching; keyboard opening and switching accept immediate typing.
- Empty tabs, current-tab file opening, independent tab histories/modes and active-tab reveal through overflow. The header has one mode button and a three-dot menu.
- Background indexing, native file watching, clean-buffer reload, and tab/layout restoration after a clean exit.
- Wiki links such as `[[Home]]` and `[[Notes/Idea|label]]`. Ambiguous names require paths.
- Atomic saves, permission preservation, external-change checks, new-note collision refusal, and unsaved-close protection. Right-click a folder to rename or move it to an existing parent inside the vault; nested notes/images and open-tab history follow the move. Existing destinations, linked folders and stale move plans are refused.
- Nested heading folds in reading, Source and Live modes. Editing folds preserve source and undo, skip hidden rows during navigation, and reveal anchor/search targets. Live/source quote, list, Setext and root fenced-code rendering follows the measured reference.
- Cached native syntax colors match the measured dark/light editing palettes; reading/editing code panels share a generated Copy control, with measured reader padding and mode-specific colors.
- Native tables with aligned columns, inline formatting, wiki links and horizontal scrolling. Live cells support Tab/Shift+Tab/Enter, shared undo, row/column changes, alignment and sorting while preserving Markdown.
- Native Mermaid fenced diagrams in Live and Reading views, cached by source and theme and rendered away from the UI thread. Clicking a Live diagram exposes its editable Markdown source.
- Thirteen callout types and their aliases, formatted titles, nested folding and source-preserving type changes. Reading callouts retain native links, tasks, code, tables and images; long nested titles wrap in both themes.
- File bookmarks with custom titles and ordering, immediate per-vault persistence, guarded rename tracking, and removable missing targets. Bookmarking and removing shortcuts preserve file content.
- Paste and selection edits have independent undo boundaries; Ctrl+S includes text received in the same frame. Wiki completion uses text queued before Tab/Enter and preserves Enter after a closing delimiter or an unmatched query.
- Embedded Inter prose, Noto Arabic fallback and DejaVu Sans Mono code fonts; their licenses are retained under assets/fonts. The pinned text-backend changes and remaining bidi limits are documented in [vendor provenance](vendor/UPSTREAM.md).
- Plain UTF-8 `.md` and `.txt` files; hidden/dependency folders and child symlinks are not indexed.

| Shortcut | Action |
| --- | --- |
| Ctrl+O | Quick switcher |
| Ctrl+N | New note |
| Ctrl+T | Empty new tab |
| Ctrl+S | Save |
| Ctrl+E | Switch reading/editing |
| Ctrl+W | Save and close; failed saves retain the tab |
| Ctrl+B / Ctrl+I | Bold / italic selection |
| Ctrl+F | Find and replace |
| Ctrl+Tab | Next tab |
| Ctrl+Q | Quit with unsaved-work protection |
| Ctrl+P | Commands, including Save a copy and Open vault |
| Ctrl+; | Add property |
| Ctrl+Shift+F | Vault search |
| Ctrl+Z / Ctrl+Y | Undo / redo while editing |
| Alt+Left / Alt+Right | Back / forward through visited notes |

If another program changes a note, save refuses to overwrite it and retains your buffer. Use **Save a copy** to preserve your version, or review the external change before discarding anything.

## Remaining work

Exact full-product parity remains in progress. Remaining areas include complex-document folding, math, graph/canvas, file recovery UI, bookmark groups and non-file targets, daily notes/templates, plugin equivalents, and optional sync/mobile. Persian rendering and exact text editing have focused checks; full visual arrow navigation and desktop IME composition remain unverified. Existing Obsidian JavaScript plugins do not execute here.

Indexing runs outside rendering, with native file notifications. Release measurements, declared method and limits are in [the plan](docs/PLAN.md) and [desktop benchmark](reports/desktop-benchmark.json). The local comparison uses installed Obsidian 1.5.12 with a different feature set; it does not establish equal-feature performance. The deterministic 5,000-note run is in [vault benchmark](reports/vault-benchmark.json). GUI verification currently covers Linux/X11.

## Checks

```sh
gh workflow run native.yml --repo MasihMoafi/rust-notes
gh run list --repo MasihMoafi/rust-notes --workflow native.yml --limit 3
```

Actions runs formatting, Clippy, Rust checks, release builds, native GUI checks and serial editor CPU gates. It tests a copied executable beside a disposable sample vault, including launch without a vault argument, and saves screenshots, logs and raw measurements in the short-lived artifact. It never modifies the real vault. [Run 39540024536](https://github.com/MasihMoafi/rust-notes/actions/runs/39540024536) passes 113 Rust executions/108 unique, 96 full native GUI groups and all six CPU conditions for source `21a1271`. The exact downloaded release also passes eight local bookmark groups without rebuilding.
