# How the startup test was measured

This is a public synopsis of the procedure declared before the measurements.
It omits local file locations. The declaration and executed harness digests are
in [identity.json](identity.json); their local originals remain retained.

1. Give each app a private profile and an identical copy of 100 synthetic notes.
   Use a dark theme, Live editing, Home open, a 300-pixel file explorer, a closed
   right sidebar, no community plugins and five-minute/seven-day file recovery.
   The complete input recipe and final settings are in [settings.json](settings.json).
2. Run the apps one at a time on the same private 1440 × 960 Xvfb display with
   software rendering. Leave unrelated host applications and CPU settings alone.
   Wait at least 0.5 seconds after all owned app processes stop between launches.
3. Keep the first launch of each private profile separate. Then run five warmup
   pairs and ten measured pairs. In measured pairs 1,3,5,7,9 Rustidian goes first;
   in pairs 2,4,6,8,10 Obsidian goes first. Reset the exact note bytes before every
   launch. Keep failures and outliers; never replace a measured observation.
4. Start the clock immediately before launching the app. Include its launcher,
   window discovery and resizing. For both apps, capture the whole display and
   read the note region [340,80,1400,700] with Tesseract, English, page mode 6,
   one OCR thread. Wait 50 milliseconds after each unsuccessful OCR pass.
5. Detect the note only when its body shows STARTUP VISIBLE NOTE. Keep the first
   matching frame and the preceding nonmatching frame. A window alone cannot
   establish readiness. The sampling interval bounds observation precision.
6. Focus the app, click its editor, press Ctrl+End, type INPUT PROBE ACCEPTED
   through xdotool, then press Ctrl+S. Require the marker in a new screenshot
   and the exact expected Home bytes on disk. Report the later confirmation
   time separately from the input-visible frame time. This includes OCR,
   focusing, clicking, typing and saving; it is not intrinsic input latency.
7. Fail a launch if the app exits, no matching note appears, or the exact save
   is absent within 30 seconds. Report medians, ranges and nearest-rank p95;
   with ten observations that p95 is the observed maximum. Do not imply
   population-level statistical certainty.

The readiness checks include seven negatives: a blank display, the wrong note
in each app, a note without the input token in each app, and each visibly open
but frozen app. Two resumed positive controls then prove queued physical input
becomes visible and saves exactly. See [controls.json](controls.json).

Two early attempts failed only in auxiliary version lookup after note/input
checks succeeded. A complete preliminary attempt exposed enabled Canvas/Bases
and mismatched recovery settings. Those attempts remain retained locally.
The final configuration corrected those differences before this selected run.
None of its ten measured pairs failed or was omitted. Raw data include its
two first-profile launches, ten warmup launches and twenty measured launches.

Window resizing was requested inside the timer. Some native first-note frames
precede completion of resizing from its initial 1380×900 window; original
full-display captures retain those margins. Input/save frames show the fitted
window. This remains a display/workload difference in the first-note endpoint.

Rustidian started with an explicit Home-file argument; Obsidian restored Home
from its workspace. Obsidian's diagnostic port was enabled for observational
identity checks; its overhead was not isolated. Built-in implementations differ.
The host was busy. No OS cache was dropped. This small synthetic workload and
software display do not establish physical-desktop, large-vault, all-feature or
future-release performance. The tested Rustidian binary is a development preview.
