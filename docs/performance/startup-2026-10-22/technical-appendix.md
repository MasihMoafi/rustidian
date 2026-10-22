# Technical appendix

This handoff describes only the optimized Rustidian c7 development preview
tested on 22 October 2026. It is not evidence that a later release has the same
startup time or that the preview has passed complete release verification.

## Exact applications

| Item | Identity |
| --- | --- |
| Rustidian source | `c7c53209ad0b8e3dc633d169da9a1342a6a22d38` |
| Rustidian executable SHA-256 | `6f3a583db0c4e3448893e2d9b0f249d53954df33550b81d21d9dad930959c129` |
| Rustidian package | 0.1.0; optimized development preview |
| Obsidian app | 1.14.4 update, installer 1.13.7, Electron 43.3.0 |
| Obsidian installer SHA-256 | `e0d8e0a611624de8c9c7dcd8a9e648279fb0a0d552faa1312b7e4f3a5fa72663` |
| Obsidian update SHA-256 | `146ef8470d8cbb4a6db209bdc19bf2bad13350444186c0fd93c31087b142b92f` |

[Compact identity and original measurement digests](identity.json) ·
[Settings and synthetic input recipe](settings.json) · [Measurement protocol](protocol.md)

## Timing and limits

The first positive screenshot detected the note at median 0.2422 s for Rustidian
and 3.9075 s for Obsidian across ten balanced warmed pairs. The Rustidian range
was 0.2349–0.2873 s; the Obsidian range was 3.6937–4.2068 s. Rustidian was already
visible in its first post-window frame: its time is an upper observation bound,
not exact paint latency. Obsidian's median last-negative/first-positive gap was
0.3172 s. Even using the conservative endpoints, the smallest observed lead
was 3.1407 s. This is sampling evidence, not a population confidence interval.

Input became visible and the exact save was confirmed at median 1.1763 s and
5.2136 s respectively. This includes the shared detector, focus/click, typing and
save confirmation. It must not be described as intrinsic keyboard latency.
All 32 selected-run launches saved identical expected Home bytes; the other
99 original notes stayed unchanged. All 20 measured launches succeeded.

Both apps used 100 notes / 8,135 bytes. The synthetic input manifest SHA-256 is
`7ccf4135cb4c7986cc6a16bc5b4283bfe443d9e862ad4caac204b271e42bc5db`. CPU: Intel Core i9-11900H, 16 logical CPUs, powersave governor;
Linux x86_64; private 1440×960×24 Xvfb with software rendering. Unrelated apps
remained running. No cold OS-cache claim is made. Other differences include
Rustidian's explicit-file launch versus Obsidian's restored workspace and an
Obsidian diagnostic port whose startup cost was not isolated. Built-in feature
implementations differ even with the same active editing/recovery workflow.
Window resizing was requested within the timer, but some native first-note
captures precede completion of resizing from its initial 1380×900 window to
1440×960. The original full-display images retain those margins. Later input/save
frames show the fitted window. This is another limit on equivalent presentation.

## Included evidence

[raw.csv](raw.csv) contains all 32 selected-run rows: 2 first-profile launches,
10 warmup launches and 20 measured launches. Only the two screenshot-path
columns were omitted for this public copy; every measurement and status value
is unchanged. All failures/outliers in the selected measured sample are retained
(there were none). Earlier setup failures and the mismatched preliminary run
remain retained locally; their logs and profiles are deliberately absent here.

[The report](report.html) opens inspected original pairs 1 and 10 in both note
and input stages. Captures happen at each app's own endpoint, not simultaneously.
The input image precedes the later save-confirmation time shown in its caption.
[The chart](images/all-ten-pairs.png) plots all ten measured pairs.
[A preceding Obsidian loading frame](images/pair-01-obsidian-before-note.png)
shows the note was still absent in that capture.
[Readiness controls](controls.json) include seven negative and two resumed
positive observations, with their original synthetic screenshots.

Every included PNG preserves its original bytes. [The manifest](manifest.json)
records content-file hashes; the manifest itself and final check receipt are
excluded to avoid recursive hashes. [Handoff checks](handoff-checks.json)
record the final path/link/image/browser audit. This folder contains no app
binaries, vault copies, private profiles, application logs or local paths.
