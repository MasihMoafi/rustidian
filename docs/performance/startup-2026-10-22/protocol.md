# Paired startup protocol — declared before execution, 22 October 2026

Question: on this host, how long does each exact app take from process launch
to displaying the same editable note, and then to accepting and saving physical
keyboard input? This informs only a bounded startup claim for this workload.
Either app may win; no full-product speed or feature-parity conclusion follows.

Rustidian is the optimized source12c2e241e6f2f3222840551a7aaf25969b5cb031
from normal Actions39790060786. Before execution, bind the binary checksum to
the retained exact-file candidate verification and its ordinary-file/source
checks. Refuse a functional/debug executable or a different source/run. Store
the exact binary/archive identity in run identity.json before timing.
Obsidian uses the installed 1.13.7 AppImage with a read-only copy of the installed
1.14.4 update ASAR in its private profile. Record actual runtime version as well.

Inputs: 100 deterministic synthetic Markdown files, one Home note with the body
marker STARTUP VISIBLE NOTE. Record per-file hashes, total bytes and aggregate
manifest hash. Each app gets its own copy, private HOME/XDG/cache/profile, no
community plugins, dark theme, editing mode, one Home tab, file explorer open,
right sidebar closed for the warmed sample. No real user app/profile is driven.
Built-in implementations and unavoidable profile differences remain confounders.

Use one private 1440×960×24 Xvfb display, serial application launches, software
rendering, and no CPU affinity/governor changes. Record CPU/OS/display, app
commands/environment, governor, thermal/load observations before and after every
launch. Existing unrelated host activity is retained and recorded. Network name
resolution is disabled for Obsidian; no downloads, remote publication, builds,
dependency installs or routing changes are authorized or required.

Record the first observed launch of each app separately before calibration.
These are not OS-cold-cache observations and are excluded from warm results.
Prepare matching persisted layouts after those observations, preserving any
failed setup and every deviation. Then run five warmup *pairs*, excluded from
summary, followed by ten measured pairs. Measured odd pairs run Rustidian then
Obsidian; even pairs reverse order. Use at least 0.5 s after complete shutdown
between launches. Reuse each app's private warmed profile and reset exact Markdown
bytes before every launch. Keep all failures and outliers; no selective retries
or exclusions from the measured ten pairs. Summarize failures separately and
successful paired deltas, medians, ranges and nearest-rank p95. Do not infer
population-level statistical certainty from ten pairs.

The timer starts immediately before Popen of the actual launch command, including
AppImage launcher work. Window discovery/move/size are included. A mapped window
or native probe is never sufficient readiness evidence. Both apps use the same
full-display screenshot and Tesseract OCR detector on crop [340,80,1400,700].
Poll serially without overlap, at most every 50 ms after the previous OCR ends.
The visible endpoint requires the exact normalized note-body marker AND a fully
painted1440x960 application area. Synchronize the requested window size/position;
the timer includes this setup. Retain an initial default-size frame as a negative,
even when it already contains the note marker. Retain the preceding negative capture, every OCR text
and capture/OCR timestamp, and the first positive full-size screenshot. Report
the last-negative/first-positive sampling bracket; OCR processing delays are
explicit measurement overhead, not a claim of exact presentation latency.

After visible detection, focus the private window, click its editor, press Ctrl+End,
type INPUT PROBE ACCEPTED physically with xdotool, and press Ctrl+S. The usable
endpoint requires the input marker in a newly captured screenshot AND the exact
expected Home bytes on disk. Record both endpoints and injection/completion
times. This end-to-end endpoint includes OCR, focus, click, typing and saving;
it is not intrinsic input latency. Use the same detector and input sequence for
both apps. A 30 s timeout, exit or missing exact save is a failure, never readiness.

Before measured observations, retain blank-screen and missing-marker negative
controls, plus an actual visible-but-SIGSTOP-frozen app input negative control for
each app. Frozen input must not be reported ready; after SIGCONT the physical
queued input must produce the expected visible marker and exact saved bytes.
Record mutations of the predicate that accept these negatives as a guard that
the controls would detect an unsafe window-only/missing-token detector.

Save raw CSV/JSONL, original screenshot pairs, controls, exact harness and a short
readable report with the local experiment. Visually inspect the retained app pair and
control frames. A result is reportable only if controls, exact identities, inputs,
five warmups, ten balanced pairs and actual screenshots are present. Incomplete
or failed work remains explicitly incomplete. Parent owns any publication claim.

This is the same predeclared100-note design as the earlier c7 comparison.
Do not pool old and current-version measurements. Either application may win;
release verification and speed evidence are separate. No run is authorized by
a debug artifact, a passing aggregate Rust result or a stale candidate receipt.


Before run-002, visual review of retained run-001 caught native first frames at1380x900 before the requested resize; its usable frames were1440x960. Keep all32 rows as diagnostic evidence, not a matched-viewport speed claim. The corrected detector records painted bounds and requires the intended full frame for both apps. Inputs, five warmup pairs, ten measured pairs, order and input/save controls are unchanged.
