# Startup comparison: exact scope

The measured optimized Rustidian baseline is the source and executable in
[identity.json](identity.json). It was installed after 48 complete cloud workflows
and a corrected preview test on the identical executable. Its Actions run retains
its original test-harness failure; it is not relabelled as a passing full run.
The later graph-label/color update has its own verification and was not used
for these startup observations.

Both apps opened identical copies of 100 synthetic notes (8,135 bytes), with
warm private profiles, no community plugins, dark Live editing, five-minute /
seven-day recovery and a 1440×960 software display. Actual Obsidian was 1.14.4
from a private copy of its update, with the 1.13.7 installed launcher. Native
folder-expansion defaults and restored Obsidian defaults differ. Rustidian used
an explicit Home argument; Obsidian restored Home. A diagnostic port enabled
version observation in Obsidian; its overhead was not isolated.

Two first-profile launches and five warmup pairs are excluded from the ten
balanced measured pairs. All 20 measured launches accepted physical input and
saved exact bytes. Seven negative and two resumed positive readiness controls
passed. All 40 measured note/input frames show a fully painted 1440×960 window.
A prior attempt caught smaller initial native windows; its 32 rows remain
retained locally and none are pooled into this result.

| Endpoint from process launch | Rustidian median (range) | Obsidian median (range) |
| --- | ---: | ---: |
| First full matching note frame | 0.609 s (0.586–0.637) | 3.775 s (3.733–3.875) |
| Visible input and exact save | 1.579 s (1.553–1.619) | 5.077 s (5.015–5.205) |

The host was a shared Linux x86_64 machine with an Intel i9-11900H, 16 logical
CPUs and powersave governor. Other applications remained running. Timing starts
before actual process launch and includes window discovery/resize, screenshots,
OCR, focus, typing and save confirmation. The median visibility sampling bracket
is about 0.356 s native / 0.331 s Obsidian. These observations do not measure
intrinsic paint or keyboard latency, OS-cold startup, a large vault, physical
screen latency or full-feature speed. No claim of fastest editor follows.

[Raw measurements](raw.csv) keep all 32 selected-run rows. Only local screenshot
paths were removed. The [interactive report](report.html) switches between
original pairs 1 and 10, in both launch orders and note/input stages. Screenshots
are each app's own endpoint, not simultaneous captures. Every exported PNG keeps
its original bytes. [Controls](controls.json), [settings](settings.json),
[declared protocol](protocol.md) and [file hashes](manifest.json) complete the
portable evidence. Private notes, profiles, logs and local file paths are excluded.
