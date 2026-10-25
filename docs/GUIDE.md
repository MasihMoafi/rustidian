# Build and deliver Rustidian from a clean checkout

Build a native Rust notebook for ordinary Markdown folders, with Obsidian’s
familiar workflows and faster equivalent work. Start with the user’s important
jobs: open a file/folder/vault, edit, save, follow links, create notes/folders,
search with Ctrl+P/Ctrl+O, check tasks and recover from mistakes. A similar screen
is insufficient: actual input, exact saved text and safe cancellation must work.

The destination includes complete Obsidian behavior and appearance. An early
alpha can ship before that destination; label it honestly. [PARITY.md](PARITY.md)
is the single feature ledger and [PLAN.md](PLAN.md) owns acceptance contracts.
[VISION.md](../VISION.md) identifies the project; [ES.md](../ES.md) owns the latest
installed source, active work and protected previews. Read them and `git status`
before resuming. Never infer delivery from a source commit or chat title.

## 1. Protect data and identify the real app

The public source repository is `MasihMoafi/rustidian`. `reports/` contains local
raw evidence and private copies; it is ignored and must stay out of uploads.
Personal AGENTS.md, actual note contents and copied-vault screenshots are private.
Stage explicit project files only. Reviewed private source/Actions work has been
authorized in this session; public publishing needs separate authorization.

On Masih’s workstation, never automate writes to Main Vault, the real Obsidian
profile or original project documents. Use copied notes and isolated
`XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_DATA_HOME` and `XDG_CACHE_HOME`. Once a
preview is handed to Masih, its entire vault/profile/window is his: identity and
screenshots may be read, but never send input, close, overwrite or clean it.
Preserve unrelated work, selected artwork and the exact bundled `demo-vault/Home.md`
bytes, including its trailing space. Clean only owned rebuildable outputs with
recoverable Trash. Never remove selected app artifacts or another effort’s work.

The legacy launcher is `~/.local/bin/rustidian`; the installed executable
is `~/.local/lib/rustidian/rustidian`. Inspect the launcher, hash that executable
and read the actual process’s `/proc/<pid>/exe`. A still-running old process does
not update when the on-disk executable changes. Keep the last verified executable
and its receipt as rollback before replacing anything.

Keep the disk guard enabled and check actual home/temporary free space. Rust
compilation, Cargo tests/Clippy and heavy matrices stay in Actions. No local
`target/`. Run bounded native GUI checks locally on the downloaded exact app with
one job and `nice -n 19`; announce background apps first. Before checks, run this
as a separate command:

```sh
ps -eo args | rg -c '[B]loodborne'
```

Empty stdout/exit1 means no game. Do not place the literal game name in another
part of the same shell command: the parent command line then matches itself.

Downloads use `gh` directly with proxy variables removed for that command. Keep
shared VPN/routing intact and preserve the 1GB provider reserve; traffic counters
do not establish remaining provider allowance. Honor stricter active transfer
restrictions. Inspect artifact sizes first and follow the global download rules.
No new services, autostart, OS migration or Elpis/account repair belongs here.

## 2. Establish the reference and a failing evaluation

Masih’s screenshots identify Obsidian1.14.4. The installed1.13.7 AppImage can run
a read-only copy of its installed1.14.4 update in a disposable profile. Verify the
actual General/version screen, archive hash, active theme, zoom and enabled core
plugins; never equate installer filename with running version. The historical
reference script has another default, so pass both paths explicitly.

`scripts/obsidian-reference.py` provides actual reference sections; inspect its
arguments and use the relevant existing section. Other retained audits and their
raw evidence are indexed in PARITY.md/ES.md. Reports do not arrive with a fresh
clone: retrieve available Actions evidence or rerun the required evaluation on
synthetic data. Never invent missing evidence from an old summary.

For each gap, write one observable acceptance case before editing. Exercise the
reference and the old native executable on matching disposable inputs. Include a
meaningful negative: a missing target, cancellation, invalid bytes, conflicting
save, drag instead of click, unfocused editor or unknown command. Prove the old
behavior fails for the intended reason while ordinary editing still works.
Record source/executable hashes, actual actions, window size and exact file bytes.

Match logical dimensions, DPI, mode and theme before measuring visuals. This
workstation can have two physical pixels per logical pixel. Root screenshots
can fail under Xwayland: capture the identified client drawable with python-xlib
instead. Inspect the actual output; naming a screenshot “light” proves nothing.

## 3. Keep one simple native implementation

| Owner | Existing path and reason |
| --- | --- |
| Shell and commands | `src/app.rs`, `window.rs`, `commands.rs`; one command registry and shared controls |
| Files, editing safety and tabs | `vault`, `document`, `operations`, `paths`, `tab`; ordinary UTF-8, atomic writes, per-tab history |
| Markdown and interaction | `editor_ui`, `editor`, `preview`, `table_ui`; source-preserving edits and shared inline rendering |
| Search and panels | `metadata`, `properties_ui`, `query`, `search`, `file_tree`; background indexing and virtualized rows |
| Graph | `graph` and `graph_ui`; cached topology, bounded layout, real navigation, shared sliders |
| Appearance | `theme`, embedded fonts and `assets/icons`; one control/style per role |
| Diagrams and code | `diagrams`, `syntax`, `code_ui`; native workers and caches |
| Persistence and observation | Per-vault state outside notes; UI probes only in explicit checks |

Inspect the source tree for exact filenames rather than creating parallel owners.
Use the locked eframe/egui APIs; current0.37 uses `scope_id`, `Id::unique`, typed
byte indices, `Frame::NONE` and current style accessors. Confirm API names in the
locked dependency before a cloud build. Never suppress a compiler warning to
hide an incompatible API.

Repair shared owners. Styled spans/highlights must render consistently in
paragraphs, tables, callouts and preview cards; Source/code intentionally retain
literal syntax. Task and table widgets inherit their document/card clipping.
Update the actual Markdown markers through the normal safe edit/Undo path.
Cache parsing and layout, bound per-frame work, virtualize long lists and keep
filesystem/index/diagram work away from typing. Existing Obsidian JavaScript
plugins do not execute in this app; a compatibility runtime needs an explicit
architecture decision, not an inactive settings page.

Use the existing OSS research and attribution in PLAN.md/README. Yazi/Helix/Ferrite
informed individual decisions; language, popularity and open-source availability
do not prove a speed win. Read the relevant implementation and license before
borrowing a component. Preserve one document model when adding shared views;
independent tab identity, cursor/mode/history and shared text ownership are
separate concerns. Shared duplicate-document views remain unfinished.

## 4. Iterate on real workflows

Implement one reported gap through its existing owner. Keep the original failing
user action unchanged. Verify ordinary input, failure/cancel paths, exact saved
bytes, Undo/Redo, restart and per-vault isolation as applicable. Then inspect both
themes and a compact window on the same optimized artifact. Retain the previous
failure; do not silently rewrite it as a pass.

Use `tests/gui_smoke.py` for actual native controls and `tests/*_behavior.rs` for
storage/parser/model contracts. Audit changed tests with the existing junk-test
skill. Keep one strongest boundary per contract; remove duplicate invocations,
source-text assertions and tests that only repeat implementation. A repeated
check needs a distinct state/lifecycle risk. Do not delete a real regression to
make a suite green.

Important lessons with direct consequences:

- **Fast slider clicks:** a drag-only rail can lose a full press/release within one
  frame. Extend the actual rail’s native response with click sensing and preserve
  its ID; focus alone and the composite union rectangle did not fix the second
  click. Keep the original quick-click regression.
- **Live control geometry:** hover can legitimately widen a menu item. Before
  mousedown require the original pointer to remain inside the current live target,
  rather than requiring identical rectangle tuples. Keep the off-target negative.
- **Selected controls:** ribbon and explorer instances of one action share the
  dialog’s real active state. Give distinct probe identities to distinct buttons;
  repeated tooltip text can identify only the last control.
- **Queued input/focus:** preserve modifier state through actual key press/release.
  Keep Unicode mappings alive on the private display. After cancelling a chooser,
  type, save and Undo immediately; Escape alone misses lost editor focus.
- **Per-vault acknowledgements:** settings files accumulate after switching. Wait
  on the session containing the current fixture note; do not require one JSON file
  in the whole state directory. Retain real two-vault switching as a precondition.
- **Preview history:** closed-note task changes should not silently become a newly
  opened editor’s Undo history; an already-open dirty target must share its real
  buffer and preserve separate Undo operations. Save a graph-backed note by its
  actual file path, not the current view kind.
- **Showing an app:** a prepared UI probe can precede a mapped native window. Wait
  for the client and loaded title, verify its actual executable, activate once,
  then hand it off. Never drive it afterward.
- **Cleanup:** `/tmp` may reject GIO Trash and cross-filesystem directory moves.
  Move only a stopped owned runtime to an owned home path with `mv --no-clobber`,
  then `gio trash` it. Preserve raw failures and confirm actual free space.

## 5. Build once and inspect exact evidence

Review the diff and source privacy before an authorized upload. Use `gh` for all
remote operations; never raw Git push/force. Query the exact full SHA before
manual dispatch so an automatic push run is not duplicated:

```sh
gh run list --repo MasihMoafi/rustidian --workflow native.yml --commit "$commit" --limit 3
gh workflow run native.yml --repo MasihMoafi/rustidian --ref main
gh run view "$run" --repo MasihMoafi/rustidian --json databaseId,headSha,status,conclusion,jobs,url
```

Apply the direct-transfer wrapper/environment to remote commands. Run long jobs
in the background and do useful independent work. A stopped watcher is stale:
check fresh timestamps and actual job/step conclusions before reporting or
cancelling. Do not cancel because an old snapshot still names an earlier phase.

The optimized artifact is `application-native-<full-sha>`; the full evidence
artifact is `rustidian-linux-<full-sha>`. Fetch only required entries where
practical, check ZIP CRC/source/build log/checksums and compare every ordinary
bundled file with that source. “Functional” artifacts can be debug builds and
are not release or timing approval. Keep license files with the app.

A focused local GUI command on the exact downloaded executable is:

```sh
nice -n 19 python3 tests/gui_smoke.py --binary /absolute/path/to/rustidian --default-vault --section page-preview
```

The executable must be accompanied by its exact expected sample files when
required. The harness copies fixtures into isolated temporary directories and
writes raw evidence to reports; retain each owned attempt before another run can
overwrite its outputs. Never run a broad matrix locally or target a handed-off
window/profile. Keep the GUI lock and all resource guards.

Build success is not verification. Require the current workflow registry and
baseline, focused cases, Rust checks, inspected screenshots and performance
conditions. Retain raw failures. If a test-only defect is independently reproduced,
verify the narrow correction against the unchanged executable and preserve prior
passing contracts; do not rerun unrelated expensive checks merely for a green
badge. Record composed evidence honestly: the original CI conclusion stays failed.
Any app change, new failure or unresolved risk requires appropriate fresh checks.

## 6. Measure equivalent work

Use the experiment-workflow skill. Predeclare question, inputs, boundaries,
controls, samples, failure criteria and confounders before collecting results.
Keep startup, indexing, UI CPU work, visible input response and saved-file latency
separate. Run applications serially on matching inputs; record versions, hashes,
plugins, graphics backend, display scale and host conditions. Disable diagnostics
and UI probes during timing. Verify exact input/save behavior alongside speed.

`scripts/compare-editors.py` owns the bounded Ferrite source-edit/save comparison;
`scripts/check-editor-timings.py` validates folded/unfolded20/200/500-block CPU
conditions. Do not compare these internal CPU measurements with another app’s
visible response. Include diverse text and exclude warmups as declared.

The current paired startup result is
`reports/startup-current-candidate/run-002/report.md`:100 synthetic notes,10
balanced measured pairs,5 excluded warmup pairs, complete1440×960 note frames and
exact saves. Medians are0.61s Rustidian versus3.77s Obsidian1.14.4 for the sampled
first matching note frame. Input-visible plus exact-save medians are1.58s/5.08s.
Software graphics, a shared host, OCR/resize overhead and folder expansion differ;
this does not establish physical-desktop speed or fastest-editor status.

Requested geometry is not painted geometry. The first startup attempt accepted
an early1380×900 native frame before the requested resize. Keep that attempt as
diagnostic evidence; require the full matching painted window before accepting
readiness, then rerun all pairs. Keep frozen-input, wrong-note and blank-frame
negative controls. Preserve raw CSV, screenshots, detector/harness identity and
plots so the conclusion can be independently checked.

## 7. Deliver, preserve rollback and leave a truthful checkpoint

Verify the candidate descends from the installed source and retains its runtime
behavior. Validate the complete evidence before atomically replacing only the
intended executable. Preserve wrapper/desktop/icon identities unless a change was
explicitly included. Recheck installed hash and every protected file afterward.
Open the actual installed app on a prepared private copy, verify `/proc` identity
and loaded title, inspect its real screen and record the handoff. From then on,
leave that app and its files to Masih.

Current alpha identity/rollback is `reports/release-verification.json`; the actual
visible graph handoff is `reports/feedback-graph-12c2e24/preview.json`. The49 native
workflow contracts are verified through48 full cloud workflows plus the corrected
original preview flow after real two-vault switching on the identical optimized
app. The original failed CI run, before failure and after pass remain intact.
Do not call that a successful full CI run or complete Obsidian parity.

Update ES.md, VISION.md and the single parity ledger with actual delivery and
remaining gaps. Keep this guide a route to the product, not a chronological diary.
Check actual disk space, preserve selected binaries/raw evidence and leave no
local build cache. Tell Masih what is installed and ready for review, link one
useful artifact and keep the final message short. His feedback determines the
next increment. Public release, media and website actions need their own scope.
