# Build a fast, familiar Markdown notebook

This guide is the route from a clean checkout to Masih's working Rust Notes.
Use `ES.md` for the current handoff and `docs/PLAN.md` for detailed contracts and
historical experiments. Keep this guide about decisions and reproducible work;
keep chronological observations in the evidence log.

## Destination and current evidence

Build a native Rust desktop editor for ordinary Markdown folders. Preserve the
Obsidian workflows Masih uses: files, tabs, links, checklists, highlighting,
search, Ctrl+P, editable titles and useful settings. Every visible action must
work. Prioritize safe, fast everyday editing over adding more panels.

Graph exploration, drawings such as Excalidraw, daily notes/templates and broad
plugin/settings replication are optional. Existing Obsidian JavaScript plugins
do not execute in this application. Masih decides whether the result is good
enough; CI alone does not establish an identical Obsidian or the fastest editor.

Runtime candidate `4dbb842a013f8400fbaab8c831d6c7e38ba445cd`, built by
[Actions39608458012](https://github.com/MasihMoafi/rust-notes/actions/runs/39608458012),
passes128 Rust executions, Clippy, all focused UI sections and six CPU
conditions. Its exact copied-plan link and scrolled checklist also pass locally.
Binary SHA256 is `8bd8663257d5cc8a9c17a45af66c68edc8a203654eca44e06a40c573d7304e4e`.
Its observer-free comparison passes in Actions39627064113; the full GUI run
stopped at a reader-width fixture that left Outline open. That setup is now
explicit and passes locally. The next complete reuse run is recorded in `ES.md`.
Installation is pending those gates. The previous fully verified release is `21a1271`;
`reports/release-verification.json` records accepted checks, not user acceptance.
Reports are ignored: download Actions evidence when starting from a fresh clone.

## Protect the user's work and machine

- Never automate writes to `/home/masih/Desktop/o/Main Vault`, original project
  documents or the real Obsidian profile. Test disposable copies with isolated
  `XDG_STATE_HOME`, `XDG_DATA_HOME` and `XDG_CONFIG_HOME`; keep private copies out
  of GitHub uploads. Do not automate a preview while Masih is using it.
- Preserve `demo-vault/Home.md` byte for byte, including `How are you?! ` and its
  trailing space. Preserve unrelated changes and concurrent `assets/app-icon/`.
  Withdrawn Downloads images and agent-icon work from another chat are excluded.
- Run Rust compilation, tests, Clippy, the full GUI suite and performance matrices
  in GitHub Actions. Never recreate local `target/`. Local formatting, source
  checks and small focused checks on a downloaded executable are allowed.
- Keep the workstation disk guard enabled. Check actual free space on home and
  temporary filesystems. Use an owned temporary directory on a healthy filesystem
  when `/tmp` has a quota; never override reserves. Move obsolete owned output to
  recoverable Trash, preserve selected artifacts and raw evidence, and report
  actual `df` space rather than claiming Trash reclaimed it.
- Reserve1,000,000,000 bytes of provider allowance. Local network counters do not
  prove remaining provider quota. Prefer direct GitHub access and preserve shared
  routing; unsetting proxy variables does not bypass an enabled TUN. Stop our VPN
  transfers at the reserve. Connection changes require the user's existing
  conditional authorization to apply, not merely a wish for faster downloads.
- Use the authorized private `MasihMoafi/rust-notes` repository and `gh` for remote
  operations. Upload reviewed sources/assets only. No force pushes, arbitrary
  publication, services/autostart, OS migration or Elpis activation changes.

## Establish the reference and the failing behavior

Read `VISION.md`, `AGENTS.md`, this guide, the newest `ES.md` checkpoint and
`git status` before editing. Verify the actual running executable and its SHA;
a source commit or old chat title can identify the wrong app.

Masih supplied eleven Obsidian screenshots under
`/home/masih/Pictures/Screenshots/`, dated21 October08:13:12 through08:14:46.
They identify Obsidian1.14.4. The isolated installed reference used by
`scripts/obsidian-reference.py` is1.5.12: retain that distinction. Its `navigation`
and `menus` sections record real interactions, CSS geometry and screenshots on
sample data. Never use the user's real Obsidian profile for automation.

Compare the same task, mode, theme, window, zoom and display scale. The desktop
preview has2760x1800 physical pixels for1380x900 logical pixels; probe coordinates
are logical. View the actual output before claiming a visual match. If root
ImageGrab fails under Xwayland, capture only the app's own client drawable with
python-xlib `get_image(..., X.ZPixmap, 0xffffffff)` and decode BGRX with Pillow.

Write the observable failure and a meaningful positive/negative evaluation first.
Prove that the evaluation rejects the old executable. Do not substitute a model
assertion for a real click, saved file, screenshot, undo or restart.

## Keep the smallest useful architecture

| Responsibility | Existing implementation and reason |
| --- | --- |
| Native shell and commands | `src/app.rs`, `src/window.rs`, `src/commands.rs`; eframe/egui0.37, one available-command registry, real custom window controls |
| File safety and navigation | `vault`, `document`, `operations`, `paths` and `tab`; ordinary UTF-8 files, guarded writes/renames, independent tab history/undo |
| Editing and reading | `editor_ui`, `editor`, `preview`, `table_ui`; one source-preserving edit path, cached parsing/layout, native interactive decorations |
| Properties, search and panels | `metadata`, `properties_ui`, `query`, `search`, `file_tree`; background indexing and virtualized visible rows |
| Appearance and artwork | `theme` plus embedded fonts and `assets/icons`; shared typography, hover and generated/SVG Studio icons |
| Diagrams and syntax | `diagrams`, `syntax`, `code_ui`; native workers/caches without browser or JavaScript runtime |
| State and observation | per-vault session outside the vault; `ui_probe` only in explicit checks, disabled during normal use and timed comparisons |

Use stable private tab IDs plus document paths for editing/scroll state. Index
row ends once and binary-search decoration positions; do not repeatedly scan
from the first line. Cache parsing by document changes and render visible rows.
Keep code/quote/task/image offsets synchronized with the edited source in the
same frame. See `vendor/UPSTREAM.md` before changing vendored behavior.

Share the vault exclusion policy between index and watcher: dotfiles/directories,
`node_modules`, `__pycache__`, `target`, `vendor`, `dist` and `build`. Hide these
without deleting them; an ordinary `Cache notes.md` must remain available.

Use built-in image generation followed by SVG Studio for new artwork. Preserve
selected raster originals, prompts, genuine SVG paths and proofs. Reuse selected
assets with the same meaning, never hand-draw replacement paths. Inspect the
actual tinted icon in the app, not only its black source proof.

## Implement and verify in this order

| Slice | Required user flow and failure controls |
| --- | --- |
| Safe notes and tabs | Open a file, type, save exact UTF-8, undo, restart; refuse stale external overwrite, retain dirty text, keep undo isolated across vaults and preceding-tab closure |
| Familiar shell | One action per icon, compact tabs/dialogs, visible hover, file menu, real minimize/maximize/restore/drag/resize; Close and Trash must respect cancellation and recovery |
| Navigation and titles | Ordinary wiki/Markdown clicks in Reading and Live,150ms held click, drag-selection negative, encoded paths/anchors, Back/Forward, Ctrl-click and queued Ctrl+Enter; title rename must update links safely |
| Markdown and checklists | Nested/Unicode tasks, checked fill/strike, click/save/undo in Live and Reading, literal code/YAML/Source negatives, visible tables and Mermaid, scrolled decoration clipping |
| Commands and settings | Useful fuzzy Ctrl+P, preserved text selection through a modal,14px interface dialogs, real persisted settings, empty/error states; unavailable actions stay unavailable |
| Highlighting and panels | Yellow `==...==` plus Cyan/Green/Orange/Red/Yellow portable mark colors; save/reopen, Source/code negatives, undo/redo, working outline/backlinks/outgoing/bookmarks |
| Speed and delivery | Keep Masih's approved300px wheel notch with no smoothing tail; measure the exact optimized app, then show it early and install only the verified release |

Reference geometry includes18px prose,18px task boxes,9px task-label gap and3px
spacing between tight task rows. Loose lists retain paragraph spacing. Explorer
text is13px; notes and dialog typography stay independent. Preserve the actual
measured values in `theme` and layout code rather than compensating for unknown
DPI by changing global zoom.

Mermaid uses [mermaid-rs-renderer0.3.1](https://github.com/1jehuang/mermaid-rs-renderer),
without CLI/PNG features. A lazy worker converts SVG labels to paths using bundled
fonts and keeps a bounded16-entry source/theme cache. Preserve natural dimensions
and block placement; hovering/scrolling must not increase the render count.
Invalid syntax retains source access. Source mode remains editable Markdown.

## Build and inspect the exact executable

From a clean authorized machine:

```sh
gh repo clone MasihMoafi/rust-notes
cd rust-notes
git status --short
cargo fmt --all -- --check
gh workflow run native.yml --repo MasihMoafi/rust-notes
gh run list --repo MasihMoafi/rust-notes --workflow native.yml --limit 3
gh run view RUN_ID --repo MasihMoafi/rust-notes --log
```

The workflow pins Rust1.95.0 and Linux dependencies, uses two build jobs, no
persistent Rust cache and one-day artifacts. It runs formatting, Clippy, behavior
checks, one optimized app/benchmark build, focused/full native checks, CPU and
external comparison gates. Independent GUI failures are collected serially;
pipefail and the final status must still reject any failed case.
Publish focused logs and failed screenshots before the long full suite, even
when a focused check fails. On reuse, retain source identity/checksums rather
than uploading another copy of the same executable.

For a harness-only correction, reuse the already compiled app and benchmark:

```sh
python3 scripts/reuse-actions-candidate.py --source-commit SOURCE_SHA --check-source
gh workflow run native.yml --repo MasihMoafi/rust-notes   -f reuse_run_id=SOURCE_RUN -f reuse_commit=SOURCE_SHA
```

The guard permits only declared harness/docs changes. Never restore locally:
restore mode creates `target/release` and belongs on the runner. A runtime change
needs a fresh optimized Actions build. Keep one candidate active at a time.
The restored source artifact also contains historical report sidecars. Attribute
Rust/build evidence to the source run; count GUI/performance evidence only when
the current run actually invoked that section and produced its passing log.
Do not promote an inherited result file as a fresh verification.

After checking route, quota and storage, download one needed artifact with `gh`.
If its downloader stalls, `gh api repos/MasihMoafi/rust-notes/actions/artifacts/ARTIFACT_ID/zip`
can write a checked ZIP into ignored `reports/` on the healthy home filesystem.
For a large evidence archive, `gh api -H "Range: bytes=START-END"` can
retrieve its ZIP directory and selected compressed entries. Verify each extracted
entry with ZIP CRC before using it; this recovered601KB of diagnostics from a
29.8MB artifact without another app download. Keep source/checksum receipts.

Validate ZIP/tar member paths, reject links/escapes, verify candidate/release and
bundle SHA256, and retain source-run identity. A candidate may be previewed on
copies; it is not an accepted installation while release gates are failing.

Use focused native checks locally only when they resolve a real remaining doubt:

```sh
python3 tests/gui_smoke.py --binary reports/VERIFIED/runtime/rust-notes --section navigation
python3 tests/gui_smoke.py --binary reports/VERIFIED/runtime/rust-notes --section task-clipping
```

Use `--section scrolling --scroll-document COPY.md` for the user's dense wrapped
note. The script reads/copies it into a private fixture; keep its content local.
Preserve every failed log, screenshot, input and exact binary identity. Do not
repeat a passed matrix merely to select a faster sample.

## Measure equivalent work and preserve failures

Use the [experiment workflow](../../skills/experiment-workflow/SKILL.md).
Inspect primary Rust implementations, licenses and pinned releases. The initial
relevant comparator is [Ferrite v0.3.0](https://github.com/OlaProeis/Ferrite), a native
Rust/egui Markdown editor under MIT. Zed/GPUI and Helix are useful architecture
references with different interaction/features, not equivalent speed baselines.
Ferrite's [line cache](https://github.com/OlaProeis/Ferrite/blob/v0.3.0/src/editor/ferrite/line_cache.rs)
retains unchanged visible-line layouts; consider that technique only after a
measured local bottleneck justifies it, with attribution for adapted code.

`scripts/compare-editors.py` compares exact optimized binaries serially on one
runner:103,300-byte corpus, Source/raw editing, wrapping,18px text, identical
software-Xvfb viewport, isolated profiles, autosave off. Verify settings through
the UI, quit/relaunch, and disable the Rust Notes probe before timing. Fonts,
feature work and save durability differ and must remain explicit limitations.

Time insertion through exact expected disk bytes, including the common50ms
input gap and subprocess/polling overhead. Send one S press/release with40ms
interval while Control remains held through byte acknowledgement; release it in
finally before another input. Keep five warmups and25 samples per app in each
of two alternating rounds; retain all samples, failures, median and p95. A lost
save is failure, not a fast result. Both median and p95 must meet the comparison
gate. This measures combined edit/save response, not display latency or the
world's fastest editor. Diagnostic tracing cannot produce an accepted release.

Keep internal CPU work separate:20/200/500-block corpora, five warmups,20 idle
and20 edited samples,25 verified insertions, unfolded/mostly folded conditions.
Targets are idle median<=5ms and edited median<=16.7ms. Different runner CPUs
cannot establish before/after speedups. Hover's150ms gate includes native input
and screenshot polling; scrolling checks distance, direction and no delayed tail.

## Failure lessons to apply before shipping

- **Scrolled tasks:** TextEdit's full text clip is not the parent scroll clip.
  Intersect both for checkbox culling and painting. The99 baseline painted822
  accent pixels over chrome;4dbb842 paints0. Require remaining visible tasks and
  unchanged Markdown, not merely a correct parsed marker.
- **Held Live links:** pressing a rendered link changes the editing line before
  release. Capture its target on press; follow only a completed click in the same
  tab. A150ms held click must open the file, while dragging selects text without
  navigation. A quick click alone missed the real failure.
- **Queued modifiers:** snapshot event modifiers before the editor consumes
  Ctrl+Enter; frame-current Control can already be released. Retain strict
  same-frame typing/save and completion checks rather than adding input retries.
- **Focus and undo:** request focus after widget registration/modal closure, then
  stop reclaiming it. Window activation is separate from editor memory focus.
  Cancel dirty Close, require automatic editor focus, undo/save exact original
  bytes; never click the editor to conceal lost focus. Fresh probes belong to
  one process. Preserve stable tab IDs across neighboring-tab deletion.
- **Input simulation:** xdotool's combined Ctrl+S releases Control before S;
  frame-current shortcut matchers can miss it. Retain private-X11 RECORD evidence
  and plain-S negative control. A verified UTF-8 clipboard plus Ctrl+V avoids
  scratch-key remapping losses in Persian fixtures; it does not prove IME/bidi.
- **UI acknowledgement:** Escape runs before settings rendering. Observe one
  checkbox click's persisted value before closing the modal. Filtering can change
  modal geometry: require settled bounds before a single setup gesture and
  retain its pointer/geometry; keep this setup outside speed measurements. Collapse expanded
  full-suite explorer folders once before timing a root-file hover. No repeated
  toggles, weakened thresholds or save retries. Close a visible right pane before
  a fixed920px reader check; resizing back to a wide window can restore it. Keep
  the exact geometry assertions after establishing that layout.
- **Native placement:** use painted galley coordinates for overlays and natural
  dimensions for diagrams. Inspect the complete image; an offscreen sliver or
  parser/probe success is not successful rendering.
- **Recovery:** confirmed active-file Trash opens a blank workspace; deleting a
  background note preserves the active tab. Reset restart fixtures through real
  Close actions and require exact tab order/modes plus recoverable deleted bytes.

## Deliver the change

Show a preview of the exact optimized executable on disposable copies early.
Keep it available for feedback with probe I/O disabled. Verify the user's actual
link form and scrolled checklist, inspect screenshots, and preserve originals.

When all required gates pass, verify `release.sha256`, source commit, bundle and
runtime hash. Preserve a rollback executable, atomically replace the default app,
and retain the existing launcher, MIME associations, default vault and concurrent
launcher artwork. Never close the user's running document to install. Verify the
next launcher path resolves to the exact new bytes; an already running old
process remains old until reopened. Record the installation and practical limits
in `VISION.md`, `ES.md` and release metadata, then remove obsolete owned rebuildable
output while retaining selected artifacts and raw evidence. Check actual disk
space. Full visual bidi/desktop IME and complex folding remain unresolved; do not
convert an essential-workflow release into a claim of complete Obsidian parity.
