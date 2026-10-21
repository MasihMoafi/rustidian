# Build a fast, familiar Markdown notebook

This is the route to Masih's product, including what we would do differently if
starting again with only this guide. It is the current execution guide; older
feature contracts and measurements remain in [PLAN.md](PLAN.md). Current work
and exact evidence are recorded in [ES.md](../ES.md).

## Destination, as clarified on 21 October 2026

Make editing ordinary Markdown files fast, safe and comfortable. Preserve the
parts of Obsidian that help Masih work: a familiar file/sidebar/tab layout,
proper checklists, readable text, highlighting, links, search, a useful Ctrl+P
palette, and essential settings. The starting point is useful; Masih has not
accepted the result as finished.

Graph exploration, drawings such as Excalidraw, broad plugin compatibility,
daily notes/templates and replication of every settings page can wait. Revisit
them when they improve a real workflow. Existing Obsidian JavaScript plugins
do not execute in this native Rust application.

## Begin from the evidence

1. Read `VISION.md`, `AGENTS.md` and this guide. Check `git status` before editing;
   other work may be in progress. Read the current checkpoint instead of starting
   another implementation from an old chat title or summary.
2. View all eleven screenshots supplied on 21 October, under
   `/home/masih/Pictures/Screenshots/Screenshot From 2026-10-21 08-13-12.png`
   through `08-14-46.png`: 08-13-12, 20, 22, 25, 27, 29, 31, 38, 50, 55 and 08-14-46. The settings screenshot identifies Obsidian 1.14.4; older installed 1.5.12 benchmarks remain historical. They show the note surface, command palette, explorer
   controls, file menu and settings. The palette is a primary workflow; optional
   plugin commands visible there are not automatically requirements.
3. Exercise the same task in the currently built app, using a disposable vault
   and isolated XDG state/data. Compare the same mode, theme, window size, zoom
   and display scale. Never infer matching font sizes from screenshots at
   different scale factors.
4. Write the observable gap and acceptance check before changing code. Show a
   small real improvement early, then apply Masih's feedback before expanding.

The last verified app source is `21a1271`, built by
[Actions run 39540024536](https://github.com/MasihMoafi/rust-notes/actions/runs/39540024536).
It passes 113 Rust test executions/108 unique, 96 GUI groups, and six CPU
conditions. Those are evidence, not user acceptance or a competitor speed result.
`reports/release-verification.json` identifies the exact local executable and
checksums; `reports/github-21a1271/` retains the raw evidence.

## Learn from Rust editors before adding machinery

Use primary repositories and pinned releases. The initial shortlist is:

| Project | Why inspect it | Limitation of the comparison |
| --- | --- | --- |
| [Ferrite](https://github.com/OlaProeis/Ferrite), v0.3.0 | Native Rust/egui Markdown editor; closest initial comparison for source editing, wrapping and large documents; repository MIT license | Its speed is not independently established here. Published Linux release is about 18 MB compressed. Check the exact release's source, modes and configuration. |
| [Zed](https://github.com/zed-industries/zed) | Rust/GPUI editor; useful input, text-buffer and rendering architecture | A general editor with a different feature set. Check component licenses before reuse. |
| [Helix](https://github.com/helix-editor/helix) | Rust text editing and incremental syntax techniques | Terminal interaction is not equivalent to a graphical Markdown notebook. |
| [Markion](https://github.com/willmove/markion) | Native Rust/GPUI source-backed Markdown workflows; repository MIT license | Packaging, maturity and measured performance need verification. |

Ferrite’s [version 0.3.0 changelog](https://github.com/OlaProeis/Ferrite/blob/v0.3.0/CHANGELOG.md) describes a rope-backed buffer, virtual scrolling, viewport-aware syntax highlighting and a bounded line cache. Treat those as candidate techniques, not independently verified speed results. Start by measuring the editor core before selecting or replacing a text widget; preserve Unicode, undo and safe-save behavior if adopting a technique.

The inspected [buffer](https://github.com/OlaProeis/Ferrite/blob/v0.3.0/src/editor/ferrite/buffer.rs) wraps Ropey and exposes character/line operations. The [line cache](https://github.com/OlaProeis/Ferrite/blob/v0.3.0/src/editor/ferrite/line_cache.rs) retains content-based galley entries, adjusts capacity to the visible line count, and invalidates edited ranges while preserving unchanged lines. If large-note edits miss the target, measure parsing/layout work first; consider these techniques with Unicode and undo checks before changing frameworks.

Do not call one the fastest because of its language, popularity or README.
Inspect a small relevant implementation and measure the actual workflow. Prefer
adapting a proven local technique to replacing the application framework. Record
the exact source and license for any code copied or adapted, and retain notices.

## Smallest architecture that serves the work

- Keep UTF-8 `.md` files as the canonical document. Preserve byte content,
  frontmatter, links, CRLF and Unicode when a command changes unrelated syntax.
- Use the existing `eframe`/`egui` interface and native renderer. The pinned
  `vendor/egui` and `vendor/epaint` changes are explained in `vendor/UPSTREAM.md`.
  Do not replace this stack before a measured bottleneck justifies it.
- Keep parsing and decorations cached by document changes. Render the visible
  portion efficiently; do not add a parser scan to every idle frame.
- Route formatting, checklist changes, completion and highlighting through the
  existing TextEdit selection, undo and save behavior. Reading interactions may
  update their own Markdown marker; Source always retains literal syntax.
- Use the existing per-vault session outside the vault for settings and layout.
  Do not write app state or test data into the user's notes or Obsidian profile.
- Keep safe atomic saves, permission checks, stale-file rejection and retained
  dirty buffers. Speed improvements must preserve these protections.
- Use image generation followed by SVG Studio for new icons. Reuse existing
  selected assets with the same meaning; do not redraw icons in code.

## Implementation order and evidence

1. **Essential editing surface.** Fix Live checklist display and toggling,
   including nested tasks, code/YAML negative controls, save and undo. Make
   creation dialogs use interface typography rather than document-heading
   typography. Keep the Markdown text size independent from dialog sizing.
2. **Ctrl+P and essential settings.** Centralize real available commands, enable
   fuzzy discovery, show shortcuts, preserve keyboard selection and scrolling,
   and keep unavailable actions out. Expose useful settings through working
   controls; do not fill a window with nonfunctional plugin/theme options.
3. **Highlighting and Markdown commands.** Preserve the exact source around a
   selection, render standard `==highlight==` in Live and Reading, retain literal
   Source/code content, and verify undo/redo and persisted text. Masih selected
   Cyan, Green, Orange, Red and Yellow. Use portable hex-background `<mark>`
   tags for colored highlights and `==` for yellow; preserve selection through
   Ctrl+P, native rendering, save/reopen and undo/redo. These are native commands,
   not a claim that arbitrary Obsidian community plugins execute.
4. **Familiar panels.** Make the existing files, search, bookmarks, outline,
   backlinks and outgoing links easy to find. Prefer actual working panels to
   extra toolbar symbols. Review the result against the supplied screenshots.
5. **Measure and optimize.** Run the comparison below, inspect the cause of a
   failed speed target, change the specific bottleneck and repeat with evidence.
6. **Review a usable build.** Open only a verified executable with a disposable
   vault. Show its screenshots and identify unverified behavior. Apply feedback.

For each behavior, establish a failing evaluation before the fix where possible,
plus a valid positive and an invalid/irrelevant negative case. Verify the actual
native click/typing path, not just a helper function or source assertion.

## Performance protocol

Question: does Rust Notes complete the same Markdown editing workflow at least
as quickly as the closest usable Rust editor without losing correctness?
Use the [experiment workflow](../../skills/experiment-workflow/SKILL.md).

Compare exact release binaries serially on one Actions runner with the same
Xvfb resolution, software renderer, corpus, editing mode and isolated profiles.
Record CPU, OS, toolchain, versions, source/binary hashes and configuration.
Use the existing 103,300-byte diverse note as the first representative input;
keep startup, indexing, source editing and live rendering separate.

For a common external measure, time synthetic insertion and an explicit Save chord
until the exact expected bytes reach the disposable file. Include the same50ms typing-to-Save gap, then hold Control through S's press/release with a40ms key interval. Start the shared Save gesture asynchronously so the timer stops as soon as exact bytes are available; finish key release before another sample. Include the typing gap and dispatch time; do not subtract them or label the result pure editing latency. Warm up five
times and retain 25 individual samples per app, median and p95. Alternate app
order in two rounds and retain failures rather than replacing them with faster
runs. Save completion is a combined edit/save measure, not display latency;
record differences in durability and save behavior. A completed window alone
does not prove the editor is ready, and an internal CPU timer is not comparable
to another app's external response timer.

Aim for Rust Notes median and p95 no higher than the comparator in this declared
workflow; report the observed ratio and uncertainty instead of rounding a loss
into a win. If the comparator cannot run correctly, record the reason and leave
the comparative result unverified. Do not remove safety checks to win a timing.
Measure broader "fastest editor" claims only with a broader equivalent study.

The exact comparison procedure is `scripts/compare-editors.py`; its `--self-check` verifies rejection of unchanged text. The workflow downloads the pinned archive into runner temporary storage, verifies the published SHA-256, and retains only logs, screenshots and raw measurements in `reports/editor-comparison/`, not the third-party executable.

Continue the existing internal CPU checks independently: 20/200/500-block
corpora, five warmups, 20 idle and 20 edited samples, 25 verified insertions,
unfolded and mostly folded modes, idle median <=5 ms and edited <=16.7 ms.
Historical releases used different CPUs; they cannot establish a speedup.

## Build, verify and keep the machine usable

Use the private repository `MasihMoafi/rust-notes`. No local Rust builds, tests,
Clippy or new `target/`. Local source editing, formatting and small Python/Node
checks are allowed. Use `gh` for remote GitHub operations and only upload this
project's reviewed sources/assets. Do not upload real vaults or local reports.

From a clean machine, clone the authorized private repository with `gh repo clone MasihMoafi/rust-notes`, then work in that checkout. The checked-in workflow declares the runner/toolchain/dependencies; do not install a local Rust build tree to reproduce the CI result.

```sh
git status --short
cargo fmt --all -- --check
gh workflow run native.yml --repo MasihMoafi/rust-notes
gh run list --repo MasihMoafi/rust-notes --workflow native.yml --limit 3
gh run view RUN_ID --repo MasihMoafi/rust-notes --log
```

Inspect the actual step logs, exact test counts and native evidence; a green
shell step can hide a failed process behind `tee` unless pipefail is enabled.
Download one needed passing artifact after the quota/storage check, validate
paths and SHA-256, and verify the exact release with disposable files. See the
local `reports/check-provider-quota.py`, when available, and release verification
metadata. Reports are ignored and will not exist in a fresh clone; use the
configured provider's allowance information to apply the same reserve policy.
Never store provider URLs or credentials in the guide, logs or repository.

Keep Actions artifacts one day and avoid persistent Rust caches. Preserve at
least 1 GB of provider allowance; that is a transfer policy, not a provider-side
cap. Keep only the needed local executable/bundle and raw verification evidence.
Move obsolete rebuildable output to recoverable Trash, then check `df`: Trash
retention can delay physical reclamation. Do not delete a directory merely named
`target`; crate source trees can legitimately contain that name. Preserve selected
app artifacts and never clean outputs during an active build. No VPN routing,
service, startup or Elpis selector/activation changes belong to this work.

## Lessons that change the route

- **Modal input:** request stable input focus before processing events, retain it
  until settled, then stop reclaiming it. Test fast F2/name/Enter and a title
  dialog opened through a menu. Avoid interrupting composition to claim focus.
  Returning from an unsaved-work guard needs the same deferred editor focus:
  the old `633bb9b` release kept the buffer but lost Undo after Escape. Disable
  autosave through real settings, type, cancel Close, require editor focus,
  then Undo/save and compare exact original bytes. Let modal layers finish
  closing; do not hide the failure by clicking the editor. Keep ordinary and
  unsaved dialogs on the shared interface typography.
- **Changed rows:** a state mutation after painting can leave stale interactive
  bounds. Request repaint and test the actual rendered order before clicking;
  retaining the correct model alone does not prove correct navigation.
- **UI typography:** document heading metrics belong to notes. Do not let them
  become the default size of dialog titles or filename input.
- **Checklist completeness:** Reading tests did not cover Live Preview. Verify
  every mode the user actually uses, with code/frontmatter negatives.
- **Native overlay positions:** galley cursor rectangles are local to the text.
  Translate them by the painted galley position before clipping or placing
  checkboxes. A parsed task alone does not prove a checkbox is on screen.
- **Formatting through a modal:** keep the note's selection while it owns focus,
  carry it through Ctrl+P with the target tab/path, and use the normal undo path.
  The direct shortcut can work while the palette loses the selection; test both.
- **Multiline input fixtures:** simulated Return invokes Markdown editing rules.
  Write literal Markdown fixtures to a disposable vault, then prove native edit,
  save and undo before testing the feature; do not weaken assertions around
  unintended list continuation.
- **Unicode simulation:** use the verified native input delay for Persian when
  testing the simulator, and preserve exact bytes. A passing glyph-count test
  does not establish visual bidi affinity or desktop IME composition.
  [xdotool temporarily remaps scratch keys](https://github.com/jordansissel/xdotool/blob/main/xdo.c)
  for unmapped Unicode; a CI bookmark entry lost letters even at100ms while the
  same exact app passed locally. Use verified UTF-8 clipboard content plus real
  Ctrl+V for the bookmark persistence check, retaining ordinary typed input and
  the exact saved/reopened Persian assertion. Do not call this an IME test.
- **Evidence and acceptance:** show the built app early. Keep one accepted release
  until its replacement passes checks; do not spend a long feature build before
  asking the user to assess the important visible changes.

The next agent should read the newest `ES.md` checkpoint, inspect `git status`,
and perform the next unchecked step above. Do not restart discovery or silently
restore the older goal of replicating every Obsidian feature.

## Navigation and checklist fidelity checkpoint, 21 October

Masih's latest feedback is decisive: the app must behave and look like Obsidian;
passing feature tests is insufficient if ordinary links do nothing. Reproduce
ordinary clicks in the default Live view as well as Reading, Ctrl-click into a
tab, history, encoded relative paths and heading scrolls. Test a working wiki
link before the failing Markdown link so coordinate errors cannot masquerade as
a resolver bug. Route destinations through one application handler; a suffix
check such as `ends_with(".md")` misroutes `note.md#heading` to a browser.

`python3 scripts/obsidian-reference.py --section navigation` uses an isolated
installed Obsidian/profile/sample vault and preserves raw checkbox CSS metrics,
click results, version identity and screenshots. Current local executable is
1.5.12; the user's screenshots are1.14.4, so keep that distinction explicit.
The measured 18px boxes use a solid accent checked fill, not an outline-only
check. Native pixel assertions must inspect the checked box, and click/save/undo
must still preserve the exact note. Preserve concurrently committed standalone
file/default-vault support while correcting these app interactions.


### Interaction feedback and native diagrams — 21 October

The next candidate targets Masih's direct feedback: every button (including tab close), folder/file and palette row needs visible pointer feedback; wheel navigation needs immediate movement; the inline title must be editable; tables and Mermaid must render in Reading and Live without altering Markdown. A built app must prove each interaction; a source search or a new probe alone is not acceptance.

`reports/obsidian-hover.json` measures installed Obsidian 1.5.12 in a disposable profile: dark icon/close/file hover uses RGB 34/41/44 and a 7px radius; close text becomes 233/229/220. The corrected selector excludes the root row; regular folders use the same hover background and brighter text. User screenshots show Rust Notes tab close and collapsed folder rows lacking feedback.

The old `b9d68d2` executable fails actual pointer-hover pixels and actual inline-title rename in the new native evaluations. Its navigation checks pass; its checkbox check catches an accent mismatch (101/183/182 instead of measured 23/127/131). Source changes correct that control independently of link colors.

Scrolling uses an explicit app option: 150 pixels per line-wheel event (300 pixels per physical X11 wheel notch), no smoothing tail and no animated programmatic scrolling. The vendored egui default stays smoothed at 40 pixels; a focused test checks both paths. Native scrolling must verify actual offset, direction, repeated input and unchanged note bytes. Hover timings include xdotool dispatch and screen-pixel polling, so do not call them internal frame times.

Mermaid uses [`mermaid-rs-renderer` 0.3.1](https://github.com/1jehuang/mermaid-rs-renderer) with CLI/PNG features disabled. One lazy worker renders SVG and converts its labels to paths using bundled Inter/Arabic fonts, away from the UI thread; it does not add system-font scanning to startup. A bounded 16-entry source/theme cache serves Reading and Live; hover and scrolling must not increase its render count. Invalid syntax keeps the literal source available. `Source editing` always shows Markdown. No browser or JavaScript renderer is introduced.

New native sections: `hover`, `title`, `scrolling`, `diagrams`. They run in Actions before the full regression/CPU/comparison gates. These changes remain unverified until the exact candidate passes and its screenshots are inspected.

Native inspection of `c6aa9e1` (Actions 39565788794) passes hover, safe title rename with actual rewritten-link clicks, and immediate scrolling (200 actual pixels per physical Xvfb notch, no delayed tail). It passes initial diagram plumbing but visual inspection rejects its Live placement (off-screen to the left) and Reading size (upscaled to full width). Preserve native SVG dimensions and exempt block diagrams from the inline-image x-offset; require visible bounds and natural size in the native evaluation. Do not confuse a visible sliver with rendered diagram acceptance.

The installed executable matched a debug candidate SHA, explaining why debug results must not be presented as release speed. Install only optimized `release.sha256` artifacts after verification. The next candidate also aligns Reading checkboxes to the first text line, restores the measured 9px label gap and 3px tight-list spacing, while keeping blank-line paragraph spacing in loose lists. All those changes remain pending exact native validation.

The exact `88c55b6` debug binary locally passes md/txt open-and-reopen, the corrected checklist geometry (including loose-list paragraphs), essential editing/settings/panels, all five highlight colors and bookmarks. Its isolated diagram screenshot verifies native readable labels at natural size; use an explicit block x-coordinate, not an inline glyph caret plus width, and omit the code background when the block is rendered. Startup focus must persist through widget registration/sizing passes; tests must wait for real focus and clear stale probe files on process restart, while keeping saved-byte checks. The CI second-launch failure remains unaccepted until a replacement passes.

For a release comparison without a local build: `/usr/bin/python3 scripts/benchmark-desktop.py --binary reports/<verified-run>/runtime/rust-notes --build-label release`. It requires the exact downloaded executable, checks visible Home content, uses private vaults/profiles and three interleaved warm samples per app, and records source corpus, binary SHA and process-tree PSS. Do not report debug results as release measurements.

### Dense documents and optimized candidates — 21 October

The user's live-preview complaint is a 21,319-byte,127-line document with long wrapped paragraphs and lists. Do not replace this case with a short Reading-only sample. `tests/gui_smoke.py --section scrolling --scroll-document <path>` reads that path and copies its contents into the disposable vault; never automate edits in the original project or upload its private content. CI generates a mixed Markdown corpus. Record both views, first wheel response, distance, repeat/reverse behavior and unchanged bytes. Ctrl+Home in Live aligns the first editor row and can leave the inline title above the viewport; measure relative wheel offset. Use xdotool `click --delay 0` for latency, since its default adds100ms.

The88c55b6 debug baseline on the real document moves200px/notch in both views, without a tail; first response was232.6ms Reading and15.7ms Live on this local software renderer. These are single observed dispatch-to-probe samples, not internal frame times or an optimized release comparison. The new evaluation demonstrably rejects that payload. Actions now builds one optimized GUI candidate and applies all focused/full UI gates to that exact executable; compilation, tests and Clippy stay remote. Keep candidate SHA and failing diagnostics; only select/install the optimized payload after verification.

In private Xvfb, resize/move the window before assigning focus, verify X input focus, then verify editor focus without clicking it. A missing-control error must retain native/window/widget focus metrics and the screenshot; never turn a startup-byte mismatch into a passing check with a retry.

### Focus request completion

The optimizedaac91d3 candidate built correctly (binarySHA53ebc31ef762279b6e43a8978a28ddae7dd0a57eaa10298c4802c1cc23781931) but failed real startup typing/focus both in Actions39573001852 and locally. Probe showed native focus0, editor memory focus1, pending request1; bypassing the native-focus wait still failed exact saved bytes, so that gate must remain.88c55b6 passes the same harness. Complete the request after the actual TextEdit registers and owns memory focus; global window activation is separate and can follow later. Never keep requesting because `Response::has_focus()` includes global activation. Fresh probe files belong to one process. Native tests first prepare the UI, explicitly deactivate once, verify logical editor focus with no pending request, then activate and require actual focus and saved/reopened bytes. This is a real focus transition, with no editor click or retry.


### File menus, window controls and development clutter

Use the same visible row in the reference and Rust Notes. `scripts/obsidian-reference.py --section menus` records native Obsidian menu labels, grouping, icon geometry, 13px text, approximately25px rows, hover colors and recoverable deletion; retain its identity because installed1.5.12 differs from the user's1.14.4 screenshots. The current increment implements the essential file actions (new tab, copy, bookmark, copy path, rename and Trash) with compact icon rows and a red destructive action. It is not full pane/merge/OS menu parity; do not label it an identical product. No inactive imitation controls.

Share one `vault::is_ignored_path` policy between indexing and watcher refreshes: dotfiles/directories, node_modules, __pycache__, target, vendor, dist and build. Ordinary notes mentioning caches remain accessible. The native before check on94b21bc fails on the actual visible __pycache__ folder; model discovery checks preserve Unicode Markdown as a positive control. Never delete source/cache directories merely to hide them.

Window minimize/maximize/restore icons and Trash artwork follow built-in image generation then SVG Studio tracing. Retain prompts, source/traces and proof. A client title bar uses the same hover treatment as app controls; Close routes through the existing unsaved-change guard. Exercise actual Openbox maximize/restore, minimize/reopen, dragging and edge resizing in private Xvfb, as well as the saved-byte close negative. Check native theme-tinted controls, not only black SVGs on a proof sheet.

Deletion must keep the user in a blank workspace when the active file is removed; deleting a background tab must preserve the active tab identity. CI39577542515 caught the old unexpected First-steps fallback. The full native restart fixture must explicitly close tabs through real controls before arranging exactly Home/Read and Created-in-GUI/Source: rename/bookmark operations can already have persisted earlier layouts. Do not relax final exact path/order/mode/selection or reopened-byte checks to accommodate a dirty fixture. Raw local94b21bc run reached this strict session setup failure.

Native editor coordinates must stay within visible content. In Actions39595171306 the scrolled galley began at y81, so the old `top + 20` helper clicked y101 inside the header ending at y108 and removed editor focus. The unchanged99d60be binary reproduces the failure locally; checking automatic focus after the neighboring tab closes, then sending Undo/Redo without another click, passes exact source recovery. Clamp other deliberate editor clicks below the current note header. Keep the failed evidence; do not "fix" undo history or add a focus retry for a misdirected test click.

Clipboard checks must request `UTF8_STRING`, seed a different value first, click the real command once, and wait for the exact replacement bytes within a bounded deadline. Preserve query errors and the actual pointer on failure. Actions39595862642 stopped on the default xclip request (`target STRING not available`); this is not evidence that legacy STRING and UTF-8 clipboard targets are interchangeable. Local99 passes the explicit UTF-8 replacement check. Its recorded elapsed time includes the standard click helper waits and is not a response benchmark.

Harness-only Actions verification can restore the exact optimized application and CPU benchmark after checking source ancestry, unchanged runtime/build files, optimized build logs, hashes and demo bytes. Record the original binary source separately from the verification-harness commit. Run independent CPU and Ferrite measurements even if a GUI check fails, retaining every failure; packaging still requires all gates. A completed comparison with a slower median or p95 must fail the speed gate, not silently produce an accepted release.

Actions39596948260 passes all six CPU conditions on EPYC7763 (103,300-byte edited medians8.5565ms unfolded/10.036ms folded), but remains rejected: one older full-suite deletion assertion expected a neighboring note instead of the current blank-tab policy, and Ferrite failed its first saved input after Rust Notes completed30. Preserve both failures. Require the blank workspace, retained neighboring tab and exact Trash bytes. Establish X keyboard focus after startup/resize settles for both comparison apps, and retain failed input identity, source bytes, focus, pointer and screenshot before destroying profiles. A local one-input Ferrite setup succeeds; it does not replace the failed CI round or establish a speed result.

For a real Xwayland preview, capture its own client drawable with python-xlib `window.get_image(..., X.ZPixmap, 0xffffffff)` and Pillow's BGRX decoder when full-root ImageGrab fails. On this desktop the1380×900 logical window has2760×1800 physical pixels; preserve the native screenshot and account for scale before any input automation. Capture only this app. Leave the disposable review window available to Masih and never automate edits while he is trying it.

Actions39598261220 retains a second immediate-sequence Ferrite failure: the native title was dirty, X focus correct, but the original103,300 file bytes remained after input+Save. Do not count lost saves as fast or claim this proves the cause. The next declared comparison uses the same fixed50ms input gap for both applications, includes it in measured time, keeps the original corpus/alternating order/5warm+25sample protocol and preserves the immediate failures as rejected evidence. Native Rust Notes queued-input save checks remain strict and unchanged. Restore focused interaction checks on reuse too: the original99 run stopped at hover, so later title/scroll/diagram/task/highlight groups still need exact release verification.

The50ms gap did not resolve the Ferrite failure in Actions39599399184. That run also catches an intermittent Rust Notes second-launch explicit save: its screenshot proves the text is present and editor/native focus valid, but the tab remains dirty and disk original. Do not misdiagnose this as missing typing/focus or keep increasing waits. `scripts/record-keyboard.py` observes only S/Control/Shift events on the private Xvfb server, retaining actual key codes and modifier states. Its local calibration distinguishes genuine Ctrl+S (state4) from plain S (state0), with no input replay. The workflow's diagnostic-only mode runs the comparison observer without packaging or claiming a verified release; ordinary release gates remain required.

Actions39601381455 confirms xdotool's combined `ctrl+s` releases Control before S (Control up208830, S up208838). Ferrite v0.3.0's [`KeyBinding::matches`](https://github.com/OlaProeis/Ferrite/blob/v0.3.0/src/config/settings.rs) checks frame-current modifiers, explaining why a short released modifier can hide this gesture. Use the shared explicit physical Save sequence instead of increasing the typing pause: local RECORD verifies Control-down/S-down/S-up/Control-up, both S events with state4. Local Rust Notes.txt open/reopen passes that gesture. The strict paused-process same-frame save cases remain unchanged. Both apps use the same gesture in the declared comparison; retain all rejected earlier protocols. Diagnostic tracing is labeled separately from the ordinary comparison. Native stages collect independent failures serially and still return failure if any case fails; the collector's positive/negative pipeline checks pass.

Diagnostic39602495305 gets Ferrite through three correct saves, then sample d saves103,299 bytes instead of103,304, removing the final typed line. Its default Ctrl+D deletes a line and shortcut matching uses all pressed keys with current modifiers; coalescing ordinary d and the following Ctrl-down therefore explains this observation. The declared normal protocol combines the fixed50ms typing gap with the corrected held-Control chord for both apps. Preserve both earlier failed variants. Observe exact disk bytes while the chord runs, then join key release after stopping the timer so a fixed release wait cannot conceal editor response. Diagnostic runs cannot create release receipts, regardless of their conclusion.
