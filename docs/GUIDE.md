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

Masih's latest21October instruction expands the target to every Obsidian feature,
with matching behavior and appearance and faster interaction. This supersedes the
earlier optional-feature scope. Follow the single [coverage ledger](PARITY.md):
open the reference on disposable data, exercise one workflow, reproduce it here,
check correctness/appearance/speed, then repeat. Existing Obsidian JavaScript
plugins do not execute here yet. Masih decides acceptance; CI alone establishes
neither full parity nor the fastest editor.

Optimized source048fced is installed after complete verification in
[Actions39658870633](https://github.com/MasihMoafi/rust-notes/actions/runs/39658870633)
with harness e326f14:133 Rust executions/127 unique,115 full native GUI groups,
all32 sections, six CPU conditions and the normal Ferrite comparison pass.
The exact executable/bundle hashes, raw evidence and preserved b9 rollback are
in reports/release-verification.json. No local Rust compilation was performed.

Masih accepted the copied-plan link and scrolled checkbox fixes in preview4dbb842.
That older candidate's full suite exposed a now-fixed table-edit probe crash;
preserve its failing evidence. The prior21a1271 receipt is retained in
reports/github-21a1271/release-verification.json. User acceptance stays scoped
separately from CI. Reports are ignored: download Actions evidence when starting
from a fresh clone.

A chooser cancellation check must type immediately after Escape, save, undo and
verify the original bytes. Merely checking that Escape changed nothing misses
lost keyboard focus: source696938b passed its earlier focused suite but failed
this added real-input check. Preserve that negative result and include it in the
next complete Actions gate; do not install the known failing candidate.

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
- An active overnight guard can forbid every proxy download even with quota available. Honor that restriction: use direct requests and keep the guard enabled.
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
They identify Obsidian1.14.4. Current paired audits explicitly pass
`--appimage /home/masih/.local/opt/Obsidian/Obsidian-1.13.7.AppImage` to
`scripts/obsidian-reference.py`. The script's historical default points to1.5.12;
never silently substitute it or equate either version with the screenshots.
Its sections record real interactions, geometry and screenshots on sample data.
Never use the user's real Obsidian profile for automation.

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

For the expanded target, follow PARITY.md after preserving each accepted slice.
Daily notes/Templates use src/note_aids.rs and the existing safe file/editor path.
Reproduce their reference with `python3 scripts/obsidian-reference.py --section
inventory --appimage /home/masih/.local/opt/Obsidian/Obsidian-1.13.7.AppImage`.
This reference disables automatic updates, uses a private profile/X server and
in-process software GPU, and writes reports/obsidian-parity-reference.json.
Its settings are a separate Electron page/window: inspect that new debug target,
not the document page. Normalize the palette's split category punctuation when
checking its selected command. Inspect getMode() before toggling into editing.
Keep the local-clock RFC3339 inputs alongside Moment outputs; parseZone timezone
names differ from normal local template variables. Retain the fixed-offset
failure evidence, not an unexplained special case in the Rust implementation.

The current note-aids source is uncompiled. Its focused native check uses
`--section note-aids`; the full collector retains all prior32 sections and adds
this one. Require exact saved source, undo/redo, cancellation, missing-template
errors, external-change rejection and per-vault settings. The fixture
tests/fixtures/obsidian-date-formats.json contains51 observed English cases.
Do not call full template/daily parity complete while PARITY.md still lists gaps.

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

For named workspaces, run the same reference script with `--section workspaces`.
The persisted layout records inactive tabs before their document views load:
`leaf.getViewState().state.file` is present while `leaf.view.file` can be absent.
Compare stored file order, modes, active file and widths, then click restored tabs.
Keep the failed loaded-view assertion and the passing actual-click evidence.
Store only layout facts; loading a named workspace must not reset unrelated
settings or silently discard dirty documents. PLAN.md owns the acceptance contract.

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
The complete native stage retains every named workflow in tests/gui_smoke.py.
Its fresh gui-all-workflows.json lists each completed workflow and a positive
behavioral check count; a function completing zero checks must fail. Verify that
the list matches the exact committed harness before installing. On a failure it
retains that workflow's screenshot, controls, recent
notes and traceback, closes menus without saving/discarding, and continues
the later workflows while the app remains alive. An app crash prevents later
workflow execution and retains its exit log. Any failure still rejects packaging.
Calibrate this collector with all-success, early-failure/later-execution and
cancellation cases; isolate later failures before treating them as product bugs.

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

## Vault chooser contract

Match the isolated Obsidian1.13.7 Home/Create captures in
`reports/obsidian-vaults-reference-{home,create}.png/json`: recent folders on the
left, Create/Open on the right, then a name and parent location for creation.
The user selected these flows as the next parity gap; the official
[Manage vaults guide](https://obsidian.md/help/manage-vaults) confirms the core
opening/creation/list behavior. These captures differ from the
1.14.4 screenshots; do not silently equate reference versions.

`src/vaults.rs` owns the recent list in
`XDG_DATA_HOME/rust-notes/vaults.json` (or `~/.local/share/rust-notes/`). Keep
per-vault sessions in their existing XDG state files. Canonical paths distinguish
equal names; missing roots remain removable. Removing a list entry preserves the
folder. Atomic failed list writes retain the prior in-memory list; corrupt state
is reported, never overwritten silently. Creation uses one validated name and an
existing parent, refuses collisions, and creates only that empty directory.
Hold a stable sibling lock file around reading the latest list, changing it and
atomically replacing the JSON. The standard library's
[file lock](https://doc.rust-lang.org/std/fs/struct.File.html#method.lock) is
available in the pinned toolchain. Reload the small list on opening the chooser;
cached windows must preserve other windows' additions and removals.

`src/vault_manager.rs` renders the chooser using existing generated controls.
Prelayout the recent button's two-row name/path galley before passing it to a
button; the widget's default truncation can hide the path. Verify actual dark
and light screenshots. New-note prompts must select the suggested basename
once before typing, preserving a context folder. Exercise immediate typing
without Ctrl+A so tests cannot conceal a broken default selection.
Browse polls an owned native Zenity child, never blocking the UI while choosing;
closing cancels/reaps it. Zenity is installed on this Ubuntu machine and declared
in the Actions dependencies. Manual path entry remains available when a picker
cannot start. Do one index on the background worker after selecting a root;
metadata validation is not a reason to scan the vault on the UI thread. Preserve
the current launcher/default, dirty-work guard and per-vault tabs/settings.
For individual file arguments, select the closest containing registered root
before falling back to the file's parent. A nested note must retain root-level
links and must not turn its subfolder into another recent vault. Check component
boundaries so a vault named `Vault` does not claim files under `Vault-other`.

Bound separator height inside an automatically sized modal. Filling the
available height feeds the previous modal size back into the next layout and
can push repeated openings offscreen. Keep recent-row widths within their
column including the More control. Require the chooser bounds and its Close
control to remain visible on every opening, not just the first screenshot.

A rapid simulated key stream can race GTK's native location completion and
append a stale suffix to a valid path. Select the location field and paste the
whole path through the real clipboard, then activate the native OK action.
Enter can navigate into a populated folder instead of confirming it. Hold each
shortcut modifier through its key release with `native_input.modified_shortcut`.
Copy the native location entry back and require the exact intended path before
OK; this distinguishes a focused entry from file-list search or relative path
input. Assert the exact opened root and retain a picker screenshot. Local positive/negative calibration must distinguish this
input problem from a Rust vault failure.
While a native picker is open, a global Enter can reach it even after attempting
to focus its parent. For the busy-parent negative, temporarily unmap the picker,
assert parent focus and require a native receipt of the physical key before
asserting no submission. Remap/refocus the picker for its real selection flow.
Direct XSendEvent keys are ignored by egui-winit and cannot prove this guard.
A bounded clipboard wait must still match the exact expected path and retain
failed targets/values; never accept a different root to make the check pass.


Run the focused `--section vaults` and retained full regression through Actions.
Require real picker selection, creation/collision/cancel, recent switching and
restart, unsaved-switch cancellation and non-destructive list removal. The before
candidate lacks the visible profile control and fails this check; source edits
are not an after result. Inspect both-theme screenshots before showing them.

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
  and plain-S negative control. For Unicode typing, map the fixture's symbols once
  on the private Xvfb display before starting the app, and restore them after it
  exits. A100ms typing delay does not prevent xdotool from restoring a temporary
  keymap before a delayed event loop decodes it. Prove this with the same paused
  executable: transient mappings lose Persian/Greek/ZWNJ text; held mappings
  preserve exact bytes. Keep this negative/positive calibration separate from
  real IME and bidirectional-text acceptance.
- **Palette input:** wait for the real focused query field and the requested
  first result before the driver's Enter. Separately queue shortcut, query and
  Enter while the app is paused, checking a valid action and an unknown command.
  This preserves fast-input coverage without confusing unready test setup with
  a runtime defect. Record focus, command rows and timestamps when diagnosing.
  Give intentional unavailable-command cases an explicit negative expectation:
  image tabs must hide Source editing and Save a copy, and Enter must leave them
  inert. Prove the same driver rejects that negative expectation for a registered
  command. A helper that insists every query have a selectable action invalidates
  the existing read-only negative checks; do not remove or skip them.
- **Conflict copies:** Save a copy preserves the dirty original. Verify the
  copy's exact bytes, then explicitly Discard the original before loading a
  layout that replaces it. Never treat making a copy as permission to discard.
- **UI acknowledgement:** Escape runs before settings rendering. Observe one
  checkbox click's persisted value before closing the modal. Filtering can change
  modal geometry: require settled bounds before a single setup gesture and
  retain its pointer/geometry; keep this setup outside speed measurements. Collapse expanded
  full-suite explorer folders once before timing a root-file hover. No repeated
  toggles, weakened thresholds or save retries. Close a visible right pane before
  a fixed920px reader check; resizing back to a wide window can restore it. Keep
  the exact geometry assertions after establishing that layout. Normalize old
  table viewport positions for the new34px client titlebar; preserve the old
  table sizes, spacing, reference positions and tolerances.
- **Native placement:** use painted galley coordinates for overlays and natural
  dimensions for diagrams. Inspect the complete image; an offscreen sliver or
  parser/probe success is not successful rendering.
  A table cell can replace document bytes after its parent layout was created;
  probe ranges against that layout's parsed source, not the newer document.
- **Recovery:** confirmed active-file Trash opens a blank workspace; deleting a
  background note preserves the active tab. Reset restart fixtures through real
  Close actions and require exact tab order/modes plus recoverable deleted bytes.

A settings click is complete when its exact vault session acknowledges the
value. Use a normal held click on settled controls, then wait before closing
settings. With multiple vaults, retain the first session path after switching
has persisted it and acknowledge that file; unrelated sessions cannot prove
that the active checkbox changed.

For a new-tab/close evaluation, explicitly create its origin and ensure its
target is not already open. Earlier workflows can leave many unrelated tabs;
use real Close/New tab actions to establish the pair, preserving all navigation
and file-byte assertions. A focused pass with one tab does not establish that
the fixture is valid in the complete suite.

If cargo test has already produced target/debug/rust-notes, retain that existing
app in functional-native-$SHA before optimized compilation. Do not add a debug
build just to create it, and never use it for release latency or installation.
This permits early focused interaction checks without extra Rust compilation.
For a new workflow that has not passed native interaction checks, run its one
focused section against that existing debug app before optimized compilation.
Keep its failure images/JSON even if compilation is skipped. Retain the same
section in the optimized focused and full suites; an early pass is not a release.

The optimized app should be downloadable immediately after compilation from
application-native-$SHA. Focused reports and the final passing release remain
separate artifacts; downloading an early app is not an installation receipt.
This permits small local disposable checks while the full suite runs on Actions.

For reference recovery checks, the current-file command selects the note, while
Settings → Snapshots → View initially needs a file selection. Its history dialog
closes through .modal-header-button, not the usual .modal-close-button. Exercise
real controls before interacting with settings behind it. Show changes needs
multiple saved versions; use the real interval setting0 in a disposable profile,
then change the file, select the older size/date and check exact Copy/Restore
bytes. Scope confirmation clicks to the top dialog: Settings remains below it
and exposes a different Clear button with the same text. Five-minute/7-day defaults must remain separate from that test setup.
Recovery also captures externally modified notes that never had an editor tab.
Exercise two distinct versions, copy each and verify browsing leaves the newer
file unchanged. Use watcher paths and already indexed saved bytes for native
capture; keep new events separate from the paths covered by an in-flight index.
Bound queue submission per frame and retry retained payloads after backpressure.
An asynchronous version query must clear the previous text immediately. On a
read error, disable Copy/Restore rather than showing old content under a new
selection. Check this with a genuinely unreadable private history file, a seeded
clipboard negative and successful exact Copy after access returns. Wait for the
loaded version's identity, not just a generic ready flag, before inspecting it.
Inspect the reference's actual size labels: short snapshots use bytes, so looking
for a decimal KB label can falsely report a missing version. Preserve the failed
driver evidence instead of attributing that selector failure to snapshot loss.
Treat modal geometry as behavior too: loading a file or showing a Copy notice
must not move controls beneath the pointer. Reserve the dialog's height before
asynchronous content arrives. Bound the body UI separately from a fixed status
viewport: a vertical separator otherwise fills the outer height and pushes
notices below it, expanding even a UI with an explicit height. Assert the file field stays at the same bounds
after copying. This assertion fails on the first compiled recovery candidate;
the negative receipt is in reports/recovery-layout-6b4b34a-before. A multiline
version label needs wrapping: single-line truncation hid its file-size line.
Check both lines in the native screenshots, as well as exact clipboard bytes.
Before local GUI checks, verify the actual interpreter can import PIL,
Xlib.display and six. Different installed Python versions have different module
paths. Always stop the owned private X server if keyboard initialization fails.
Keep a failed Restore warning visible across background history refreshes.
The b4e0d5c candidate preserved an external replacement but its next automatic
reload erased the warning. Clear it on an explicit file/version selection or
successful restore, rather than on every reload. Verify both preserved disk
bytes and the still-visible warning; neither alone proves usable conflict handling.
Keep a displayed history list stable while snapshots arrive. The isolated
1.13.7 live-update check held two displayed versions unchanged and exposed the
third captured version after reopening; Copy still returned the selected text.
Load only content/diff on a version click, and refresh metadata when choosing a
file or reopening. After Save-and-restore, reopen before enumerating newly
recorded versions. Assert an existing row's bounds survive a background capture.

For unique-note collision checks, sample the reference's current time prefix
before each command and avoid the minute boundary. Two different timestamps do
not prove collision handling. The1.13.7 default created adjacent minute-shaped
filenames while both sampled prefixes were identical, preserving the first file.
Random-note checks should start from a blank tab so doing nothing cannot pass;
an empty-vault negative must verify no note opens or is created. Retain observed
reference errors separately from the safe empty outcome.
Unique-note settings are read from zk-prefixer.json. In the observed1.13.7
workflow, a missing output folder or template creates no file, and a repeated
literal format reports failure rather than appending an invented numeric suffix.
The internal-link command uses the same collision lookup but does not require
the creation folder/template to exist; it inserts only the basename and positions
the caret before the closing brackets. It is unavailable in Reading. Verify a
single undo/redo and type an alias at the resulting caret. Wait for the reference
editor's actual file bytes before treating an autosave delay as lost edits.

Exercise pinned-tab close actions separately from ordinary close. In1.13.7,
the first Ctrl+W unpins the active tab without closing it; the second closes it.
Pinned navigation creates/chooses another tab, and Close all other tabs preserves
pinned views. Retain per-vault/session/named-layout pin state and compare the
actual independent groups; a source/preview split is a different workflow.

Measure the actual menu rows, not just their parent container. The1.13.7 file
menu container is15px, while its items are13px and about24.9px high. Match the
ordered actions and test each action before mapping similarly named features;
the menu's version history is not evidence of local snapshot recovery.
Make a copy is immediate: append a space and the first available positive
number to the complete basename, in the same folder. A numbered copy gets its
own suffix (sample2 becomes sample2 1), and images are copied as bytes. Keep the
explicit Save a copy recovery dialog separate. Use atomic create-without-replace
and update the index without rereading the vault. The1.13.7 reference initially
copies CRLF exactly but later normalizes the earlier copy after another opens;
record those action-time bytes when testing Move cancellation. Native copies
preserve their exact source bytes.


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

### Audit file moves by resolved destinations

The actual `file-move` audit now passes eight actions on1.14.4, including
current-folder no-op, fuzzy selection/cancellation, filtered Escape, existing,
root and nested destinations, plain-Enter creation and collision numbering.
The previous1.13.7 five/six/seven-case reports remain in their dated evidence
folders. `obsidian-file-move-reference.json` binds the current app archive hash
and its version observed through actual General settings.

- Match the separate `Move file to...` picker: `Type a folder`, `/`, fuzzy folder
  choices, arrows/Enter, Escape and the new-folder row. With no matches, plain
  Enter creates the offered folder; Shift+Enter also creates one. With fuzzy
  matches, Enter chooses the selected existing folder. Moving an
  inactive file keeps the active document. Cancellation preserves exact bytes.
- With the reference's default automatic-link setting false, necessary rewrites
  offer Always update/Just once/Do not update. Just once leaves the preference
  false; Always update persists true. A still-valid unique basename needs no
  rewrite/prompt. Assert actual resolved paths and preserved aliases/labels:
  the reference may shorten a rooted link to its unique basename.
- An occupied destination does not reject or overwrite: it chooses the first
  available numbered filename. The normal Rename action has its own contract;
  do not change its collision semantics to implement Move. Verify the moved
  content, occupied bytes, open views/history and every affected link.
- Creation/error/cancellation need contained paths and guarded atomic moves.
  Check hidden/linked/missing parents, concurrent destinations and dirty/external
  changes before calling the native workflow equivalent. Choosing the current
  folder closes the reference picker without notices, path or byte changes.

Keep physical relocation separate from optional reference rewrites: the file is
already moved when the three-button prompt appears. Retain the original reference
snapshot so an external edit during the prompt is rejected by ordinary guarded
save. Check each old target after relocation and omit rewrites that still resolve
to the intended file, including valid short names in mixed reference documents.
Keep editing/scroll identity with the same file across relocation; navigation to
a different document needs a different identity so undo cannot leak. Folder
matching must preserve dotted names; do not pass folders through a file matcher
that removes extensions. Use the existing bounded, nonlinked settings reader.

For the screenshot version, copy the already installed `obsidian-1.14.4.asar`
read-only into the disposable profile with `--app-asar`; never copy the real
profile database or change its files. Verify the displayed General version,
not an app/API field that can be undefined. Settings is a separate native window
and CDP page: observe that target, close the actual window, then restore the note
page before continuing. Preserve the three failed identity/location observations
so a future agent does not repeat them. This update needs no download or local
build. Cache folder matches until input changes; virtualize visible rows and
verify the actual1.14.4 dialog position rather than inheriting1.13.7 geometry.

Next native Move implementation is committed only in an isolated local worktree;
it has not compiled. Ten FS behavior cases, search/tab identity cases and ten native Move assertions
plus startup are prepared. Current normal9feb release stays independent. The Move preference does
not establish general Rename preference parity; keep that remaining gap visible.

### Escape belongs to the editor before focus navigation

The512 early native check exposed Escape dropping focus after a unique-link edit.
Egui applies focus-navigation keys during begin-pass, before widget `consume_key`.
Set the main TextEdit EventFilter explicitly for Tab, both arrow axes and Escape;
its default filter has both arrow flags false. Keep the real focus/typed-alias
assertion and exact independent link/label undo/redo, rather than retrying focus.
Audit the phase: Obsidian places the initial link caret before `]]`, but redo can
place it after the completed link. Do not infer redo selection from the initial
command. Raw reference observations and the failed native controls stay in reports.

The actual1.13.7 `page-preview` audit also records Reading, Live Preview and Source
with plain/Ctrl hover. Reading shows the target card without Ctrl; editing modes
require it. Pointer-away dismissal preserves the active note and all original
bytes. Keep mode/modifier and dismissal negatives when implementing the native
card; a text tooltip or navigating the active tab is not equivalent.

The early debug correctness gate collects all five independent workflow results
before failing, retaining section-specific screenshots. Keep that aggregate
failure as a hard prerequisite for optimized compilation. One broken workflow
must not prevent discovery of unrelated failures in the already compiled app;
this avoids rebuilding merely to reach the next new check.

### Acknowledge saved bytes and the correct session checkpoint

A created-note guard must establish the actual editor focus/title, type once,
save once and observe the exact initial-plus-typed bytes before switching away.
An immediate snapshot can still contain the new file's header before its queued
edit is saved. Preserve the typed-byte acknowledgement and compare those same
bytes after switching and a rejected vault-name collision; do not weaken the
content check or retry input. The Cea failure and corrected focused run retain
both sides of this case.

Live pin state comes from the actual tab/pin controls. Persisted pin state must
be checked after a real clean Ctrl+Q or vault switch, then checked again in the
restarted UI. SIGTERM is not a clean quit, and this version does not write a
session for every navigation. Keep saved flags/order and live glyph assertions;
abrupt-exit layout durability is a separate incomplete requirement.

Isolate GTK as well as the Rust app during native tests. The workstation's GTK4
picker can use the shared desktop portal even with a private DISPLAY. Force
GDK's X11 backend, disable portal use and give test children an unavailable,
short task-specific session-bus address. A real picker window must appear on
the private display before driving it. These overrides belong only to test
children; preserve Masih's desktop session, services and normal app environment.

Check the rendered theme as live UI state as well. The pin fixture initially
labelled two identical light captures as dark/light because it read a stale saved
preference. `set_theme` samples the unobscured ribbon before/after the actual
palette toggle and keeps the observations; inspect both distinct screenshots.
A filename or a serialized dark flag is not visual evidence.

### Include pin state in named-layout verification

Source9feb added `SavedTab.pinned`, emitted even when false. The older workspace
exact-dictionary assertion failed on the correct saved path/modes plus
`pinned:false`; its immutable negative and observed bytes remain under
reports/workspace-pin-schema-9febfbb/. Keep the exact path/view/editing checks and
include the new field. Verify a real pin-true save/load and reversed pin-false
save/load against the visible tab glyphs, with original note bytes preserved.
All10 corrected focused checks pass on the unchanged optimized9feb app. The next
harness-only Actions run can reuse that exact app and CPU example, keeping every
focused/full/performance gate without compiling Rust again.

### Treat native1.14.4 colors as their own Markdown syntax

The isolated six-command native-highlight audit saves `==🔵text==` for Blue,
`==🟢text==` for Green, `==🟠text==` for Orange, `==🟣text==` for Purple,
`==🔴text==` for Red and `==🟡text==` for Yellow. Reading hides the marker and
renders a `mark` with `data-highlight` set to the color name. Literal inline code
stays literal; one undo and redo preserve exact bytes. The report under
reports/obsidian-1.14.4-highlights/ retains computed colors, markup, real Source
selection and restored original notes. Do not equate this with the older HTML
Highlightr colors. Native compatibility remains unfinished: audit toggle/remove,
changing an existing color, Live/caret, link/code/fence and light appearance
before implementing it; preserve Masih's Cyan alongside the new colors.

### Set up keyboard checks with a real document

The old full fixture requested Source editing immediately after creating an empty
tab. Actual1.14.4 exposes the analogous mode command on a Markdown view and hides
it on EmptyView; both sides are observed in the isolated empty-commands audit.
Keep that availability negative. Prepare Source on a real disposable anchor,
then quick-open the target and exercise immediate editing, reading, tab switches
and queued input. All8 corrected focused checks pass on the same optimized9feb
app. Never fix this fixture by exposing an inactive command or dropping typing
assertions. Preserve the failed cloud snapshot and the actual reference rows.

The full suite also has17 initial smoke checks before its grouped workflows.
Keep gui-all-baseline.json and verify its rows plus all per-workflow counts;
startup alone is not the baseline. This fixes the release check's earlier count
assumption without discarding any real check.


### Preserve the native color edit, caret and theme semantics

The1.14.4 toggle/caret/render audits now retain exact source and real selections.
Repeating Blue removes its mark. Changing a full selection to Purple replaces
its marker. A caret anywhere inside the word applies or changes the whole word
without selecting it; removing the color keeps that caret at the same content
position. Obsidian positions count UTF-16 units (a color marker adds4 units);
our native character cursor counts3 characters. Compare the resulting text and
next typed character, not those incompatible numeric coordinate systems.

A partial color change splits its neighbors: `==🔵TA====🟣RGE====🔵T==`.
Reading renders three adjacent marks. Accept the paired two-delimiter boundary
while continuing to reject odd `===` runs, code, escaped delimiters, metadata
and destinations. Keep exact undo/redo and clipboard-selected text assertions.
Actual Live leaves a visible Purple emoji at this adjacent boundary; record
this reference limitation rather than inferring normal six-color rendering.

Six native colors have distinct dark/light computed shades at0.3alpha. Preserve
semantic color identity in cached parse spans and resolve its theme when painting;
do not cache one theme's RGBA or perform Oklch conversion every UI frame. The
observed values, fixed conversion and original screenshots are in the expanded
reference evidence and isolated color preparation. Keep portable Highlightr/Cyan
commands and source untouched. Cross-span/multiline selection and full IME/bidi
editing still need paired cases; the prepared native implementation is uncompiled.

Page preview is independently observed on1.14.4 as a452px card. Reading accepts
plain or Ctrl hover; Live/Source require Ctrl. Moving away dismisses it without
changing the active note or any bytes. Preserve that six-case matrix. Interactive
card task/link/open behavior still needs an audit before implementation; no native
card has been delivered yet.


After interacting with a preview task, the checkbox holds focus. In the actual
1.14.4 reference, pointer-away and Escape keep that card open; an outside click
dismisses it. Check the focused element and real dismissal path. The task edits
the target while source stays active; the corner Open link and an internal link
replace the current tab. A missing note shows Click to create, without creating
anything on hover. The first rejected pointer-away assumption and corrected
physical-input audit remain under the two page-preview supplemental directories.

On the shared desktop, xdotool search combines multiple criteria with OR unless
`--all` is given. Verify the exact `_NET_WM_PID` as well as the window title before
reading a candidate screenshot; the first9feb preview lookup captured the older
preview. No input was sent. Both records are retained and the corrected image is
inspected. A handed-off preview and its whole private profile/vault become
user-owned: never drive, close, rewrite or clean them afterward.


Validate a move's parent before collision numbering, rather than calling a
file-path validator on its first occupied basename. That validator correctly
rejects a final symlink for ordinary Rename; folder Move must instead leave that
symlink in place and try the next numbered basename. Keep linked-parent refusal.
The new uncompiled filesystem case preserves a regular external symlink, a
dangling external symlink, an occupied directory, exact source bytes and updated
references. Do not weaken the older Rename guard to make Move pass.


Recognize empty native color marks as well. In1.14.4, `==🔵==` is an invisible
empty Blue Reading mark, not an unmatched opener. A repeated toggle removes it;
typing at its retained caret fills it. Keep its neighbor's color separate and
verify exact undo/redo. Rejecting the stripped empty content broke this in the
prepared parser; its correction and two new Rust cases are local/uncompiled.
The actual availability audit also confirms color commands are absent in Reading
and Empty views and present in Source; do not infer that from another command.

Before a UI compile, check the actual vendored toolkit signatures. In egui0.37,
Context exposes global_style(), and TextEdit.frame accepts Frame::NONE, not a
boolean. The first combined Move/color candidate failed these two calls in
Clippy; keep that raw failure and preserve every behavioral gate on the corrected
normal build. Borrow Path::new for path comparisons instead of allocating PathBuf.

A preview card shares the target's existing editor and undo history. Actual1.14.4
shows unsaved target text, propagates task changes to the other tab, and reverses
two task toggles then the prior typing as three undo steps. It refreshes external
changes while open. A new tab can inherit Source mode: select Reading explicitly
before testing unmodified hover. The failed observer and corrected shared-buffer
audit are retained; no native preview is approved by those reference observations.

Use real saved documents for navigation/move fixtures. Document::new correctly
represents unsaved content; expecting Tab.open to navigate away from it weakens
the safety contract. The identity fixture now moves actual temporary files, reloads
the saved result, then navigates. Run Cargo tests with --no-fail-fast to collect
all target failures in one Actions build, preserving every retained test.

For a folder-qualified quick-switch query, assert the basename in the OS title;
the title does not display the entire query. In the actual1.14.4 Move picker, /
ranks first even when other matching folder paths contain slashes. Explicit folder
queries prefer an exact row, while an empty query keeps the current folder first.
Preserve both cases and the actual Enter operation in the native eval.

After a Reading/Source round trip, the first color Undo exposed a cursor-only
text-widget snapshot. A diagnostic second press proved the cause, not acceptance.
Undo and Redo now skip same-text snapshots inside the existing event handler,
with real single-action and queued typing/key tests. Do not retry the acceptance
keypress or process Undo before pending text events.

The six colored native marker characters need real color glyphs. Existing fonts
rendered boxes. Extract just the installed Noto Color Emoji CBDT PNG bytes with
fontTools, retain their source hash/metrics/licenses, and use egui's cached native
GlyphRasterizer with is_color=true. Decode lazily and override only those clusters.
Bundle font licenses and preserve narrow archive extraction guards. Verify actual
Source pixels as well as stored Unicode and Reading highlights; all remain
unverified until the corrected Actions app executes these cases.

Headless input uses the actual egui0.37 event stream: Event::ModifiersChanged,
not the removed RawInput.modifiers field. Use Id::unique for a global widget ID;
Id::new is deprecated and the Clippy gate rejects it. Check those signatures
in the vendored source before writing a new native input eval.

A font helper used only by theme::install_fonts belongs inside the shared theme
module. Several standalone benchmarks include theme.rs directly. A new root
module declared only by main and the editor benchmark broke the explorer target;
placing it under theme keeps every consumer consistent without repeated setup.

A file query is an ordered sequence. Nucleo Pattern::new splits words into
independent atoms: Closed C can reuse the first C in Closed A and select it.
The actual1.14.4 reference returned only Closed C for both original/lowercase
queries. Keep the failing native screenshot/driver and reference rows, then use
one fuzzy Atom for the whole filename query. Preserve abbreviations, aliases,
Unicode, empty/limited results and folder-root ordering with real chooser checks.
Commands have a separate matcher; do not change it without its own paired cases.

Observe tab restoration order rather than assuming it. Actual1.14.4 appends a
restored middle tab to its group, Close all other tabs keeps pins, and closing
the final pinned tab first unpins it. The first original-position assertion
failed and remains saved. A separate corrected reference passes all12 actions.
Current9feb Ctrl+Shift+T incorrectly opens a blank tab through the broader Ctrl+T
handler. Any future undo-close handler must run before that broader shortcut,
with actual middle/final close, pinned protection and file-byte checks.

Before a focused native check, import Pillow, Xlib and six with the exact Python
interpreter that will run it. This host's /usr/bin/python3 is3.14 and its current
six module is absent; python3 is the configured3.13 interpreter with six.
The existing pure-Python Xlib is under ~/.local/lib/python3.12/site-packages;
append only that known path for the local disposable driver, then import all
three dependencies before creating Xvfb. Do not install packages or disable
stable key mapping merely to get past a dependency failure. Keep CI's declared
Python/tool setup unchanged unless its actual checks fail.

Inspect popup typography before continuing an expensive suite. Source63321cf
passed199 Rust executions and all7 early flows, yet Update links inherited the
32px document heading. Its frozen debug images reject that visual result.
Current1.14.4 measurements for both themes are560×201.25,16px inset,15px title
and body/19.5px line,13px buttons30px high,15px corners, right-aligned choices,
Always update initially focused and a close glyph. Match measured widget bounds
and sample real normal/hover pixels; all file/link/Enter/close cases still run.
Cancel a known rejected candidate after saving evidence; one new combined run
retains every gate. The expanded44/19/9 workflow registry has a90min cap.

Do not invent preview Undo semantics. A target already open shares its editor
buffer and prior typing history with task changes in its card. A closed target's
card writes its file, but its first editor starts without those task changes in
Undo. The latter was verified for plain and properties notes only after actual
focused typing and Undo proved the keyboard path. An earlier assumed positive
failed and stays saved. Native recording is therefore restricted to an already
open target. Check both modes with metadata bytes and the old unsaved-buffer
case; never turn a wrong expectation into an implementation requirement.

A Move-update prompt requires a link that actually becomes invalid after moving.
A shortest-name link can remain valid and correctly skip the prompt. Use an
explicit folder-qualified fixture and expand its real folder before testing the
close button; do not change resolution behavior to satisfy an invalid fixture.
Keep focused flows independent of earlier tabs and use the exact aliased link
probe the runtime exposes. A smaller passing preview sequence cannot replace a
longer failing open-buffer/Undo sequence: retain it as a diagnostic, preserve the
failed single action and inspect the document/card before and after that action.

The full preview sequence caught a real sizing leak that the short diagnostic
missed: egui Area restores its last size, then ScrollArea caps its viewport to
that available height. After a short properties card, a longer card's checkbox
was below the visible content despite an exposed probe. Give the parent its
current maximum before laying out the scroll area, while retaining automatic
shrink and the existing height cap. Check the actual checkbox against the card's
inner bounds before the original single click; then verify unsaved text, manual
save and separate task/typing Undo. Never fix this by retrying the click.

Match a negative driver's probes and query behavior to its exact baseline app.
Source9feb exposes actual tab-full headers but predates the tab-count metric;
its old fuzzy matcher also reuses the B in Bulk for Bulk B. Use distinct query
words and count its real headers before asserting a new command is absent.
Retain failed setup attempts separately from a demonstrated missing feature.

For bulk tab closing, current1.14.4 closes eligible views in visual order, skips
pinned/current views and restores them in reverse close order at the group end.
The four-view audit records D/B, then B/D/C after reordered restoration. The
palette hides Close all other tabs when only the selected view and pinned
neighbors remain, including an active blank view. Native close must also retain
the app's explicit conflict choices: validate only closing notes, close none
when a save fails, and never replace newer disk bytes on Discard/restore.

Native GTK focus setup must finish before the busy-parent negative. A real
source8 CI failure occurred before its Enter: asynchronous unmap left focus at
the X root. Focus the picker once and let activation settle, synchronize its
unmap, then focus the parent and record each actual map/focus state. Keep the
single Enter, parent-observed metric and blocked-parent assertions. The exact8
CI debug app passes all7 vault groups with this calibrated setup; no app focus
behavior was changed. Preserve the old failed screenshot and raw log.

Document font controls require their own scope. Actual1.14.4 changes document
text by1px across Reading/Live/Source and other notes, keeps tab/status fonts
unchanged, clamps10–30 and restores the change after normal quit/relaunch.
Measure real glyph/line bounds and text Undo, not only a preference value.
Retain current baseline fonts at size18, scale inside document/card UIs and
include code, title, property inputs, tables and task hit boxes. Do not change
interface zoom. Import a guarded ordinary appearance.json once, preserve its
bytes and let an absent field in an old native session keep that imported
default. Native state owns later changes; failed writes must roll back and
retries clear their error. The current isolated font implementation remains
uncompiled. The first theme capture silently kept dark mode; its filenames
are not light-mode evidence. Corrected Toggle light/dark observations and a
normal-restart audit are retained under reports/obsidian-1.14.4-font-controls-*/.

Use plain proof words when measuring shared editing. Actual1.14.4 Source
automatically turned a newly typed `_FIRST` into `_FIRST_`: active buffer and
saved file matched, so this was successful input plus auto-pairing. Retain the
raw observation, use alphanumeric text for the separate buffer/Undo audit and
track automatic delimiter completion as its own parity case. A fixed autosave
sleep or an unexamined literal expectation cannot establish an input failure.

For duplicate-note observations, assert the focused visible pane is actually
Source before typing. Quick switching or splitting can create a Reading leaf;
its hidden editor may keep an old model. Compare visible buffers, disk and
independent cursors. Undo can group physical keystrokes differently: record a
bounded real Undo/Redo chain and verify original bytes rather than assuming
one whole word per Undo. The14-action1.14.4 audit and immutable earlier setup
failures live in reports/obsidian-1.14.4-shared-document-history/.

### Compound inline formatting must not switch the entire renderer

Trigger: a table mixes a colored/bold span, an rgba mark with padding/color-inherit
and ==highlight==. The exact9b5 app passes an ordinary native-table control, then
loses table controls and displays literal attributes for the reconstructed case
(reports/inline-styles-missing-9b5eafc/). Unsupported inline HTML triggered a
whole-document fallback. Parse inert presentation styles once for both renderers;
keep code/YAML literal and Source editable. Test actual colored table pixels in
both themes, nested callouts, native hover cards, exact Undo/Save and source bytes.
Source changes alone are not proof; require the exact Actions app.

For long virtual lists, a registered probe can refer to an overscan row hidden
under a fixed toolbar. Full-suite trailing root notes also change the final
child's distance from the scroll bottom. Reproduce with80 trailing roots using
the unchanged binary, scroll physically until the whole target row is visible,
then retain the original click/title assertion. See the negative, intermediate
clipped-click failure and positive in reports/explorer-trailing-after-9b5eafc/.
