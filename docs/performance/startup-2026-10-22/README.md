# The same note, from launch to usable input

On 22 October 2026, the optimized Rustidian baseline showed the matching note
sooner in all ten warmed pairs on this host.

| Median of ten launches each | Rustidian | Obsidian 1.14.4 |
| --- | ---: | ---: |
| First complete matching note frame | **0.61 s** | **3.77 s** |
| Input visible and exact save confirmed | 1.58 s | 5.08 s |

Five warmup pairs preceded ten measured pairs with balanced launch order.
Both apps used matching copies of 100 synthetic notes, no community plugins,
a 1440×960 virtual display and software rendering. All measured launches
accepted physical input and saved exact text; all nine readiness controls passed.

| Rustidian: note detected at 0.59 s | Obsidian: note detected at 3.75 s |
| --- | --- |
| ![Rustidian showing the synthetic Home note](images/pair-01-rustidian-note.png) | ![Obsidian showing the same Home note](images/pair-01-obsidian-note.png) |

These are original captures at each app's own observation time. The
[interactive comparison](report.html) opens both launch orders and input stages.

![All ten measured pairs](images/all-ten-pairs.png)

This is a small warmed workload on one shared host. Screenshot/OCR, resizing,
focusing, typing and confirmation add measurement overhead. Folder expansion
defaults differ. The measured source is the installed baseline in
[identity.json](identity.json), before the later graph update. This does not
establish cold startup, physical-display latency or full-feature speed.

[Raw measurements](raw.csv) · [Protocol](protocol.md) · [Settings](settings.json) ·
[Controls](controls.json) · [Exact versions and limits](technical-appendix.md)
