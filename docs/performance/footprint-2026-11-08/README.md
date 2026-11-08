# Rustidian and Obsidian: local measurements

Measured on 8 November 2026. Runtime cells are medians of three warm, serial
runs per app; package and installed-file sizes were measured separately.

| Measurement | Rustidian | Obsidian | Method |
| --- | ---: | ---: | --- |
| Linux download | 28.09 MiB `.deb`; 26.99 MiB `.tar.gz` | 108.02 MiB AppImage | Download bytes / 1,048,576; Rustidian files match release SHA-256 checksums. The installed Obsidian image matches the official asset's byte count. |
| Debian package's declared installed size | 53.48 MiB | — | Rustidian 0.1.6 `.deb` metadata: 54,764 KiB, an estimate rather than allocated disk blocks. |
| Installed files, excluding rollback copies | 58.60 MiB | 108.18 MiB | Allocated blocks from `du -B1`; executable, launcher, desktop entry and installed icons; app data and shared system libraries excluded. |
| Installed files, including Rustidian rollback copies | 168.69 MiB | 108.18 MiB | Same method, including Rustidian's existing rollback directory. |
| Startup to visible Home | 1.12 s | 1.36 s | Process launch to first captured 1440 × 960 frame containing the Home text; OCR sampling and window setup included. |
| Startup to usable Home | 3.35 s | 4.81 s | Process launch to OCR recognition of text typed through simulated X11 key events **and** exact bytes saved to disk; includes two screenshot/OCR detections and the input sequence. |
| One open note: process-tree PSS / RSS | 97.73 / 146.33 MiB | 284.02 / 588.88 MiB | Sum `/proc/<pid>/smaps_rollup` for each app and its descendants after 30 s idle. |
| Ten open note tabs: process-tree PSS / RSS | 97.80 / 147.56 MiB | 284.18 / 587.35 MiB | Same process-tree sums after opening ten distinct notes and settling for 30 s. |
| Idle CPU, percentage of one logical CPU | 7.37% | 1.10% | App and descendant user + system CPU time sampled over 60 s, after the one-note 30 s settle. |

The Rustidian downloads are from the [public 0.1.6 release](https://github.com/MasihMoafi/rustidian/releases/tag/v0.1.6), published 4 November 2026. The executable measured here is **0.1.7, an unpublished release candidate**, SHA-256 `cf548e35d9fe23986eab1022e5011cd24b70273ae9a534871ce6c7a4ddbe20aa`. It replaced the candidate used in the earlier size-only report; the installed sizes above were remeasured.

Obsidian is the [1.5.12 Linux AppImage](https://github.com/obsidianmd/obsidian-releases/releases/tag/v1.5.12), published in April 2024; its SHA-256 is `aa65d9992a7b60f676936fbc80d616f45cf9b3468c4ab60730123b727f4cf2ee`. This older version, different built-in features and package formats limit the comparison; these measurements do not establish feature parity.

## Method and limits

Both apps opened the same disposable copy of `demo-vault/` on one private 1440 × 960 Xvfb display, using Mesa software rendering and separate private HOME and XDG profiles. The original vault has four Markdown notes; six short fixture notes were added to the copy so the ten tabs contain distinct notes. There were no community plugins. Both apps used dark themes; their fonts and built-in features differ. Each application had one untimed warmup, then three measured pairs with the first app alternating by pair. Each run began with one Home tab and restored the original Markdown bytes before launch.

The visible endpoint uses the existing screenshot/OCR approach in `docs/performance/startup-2026-10-22/`; the usable endpoint adds simulated X11 keyboard input and requires both screenshot recognition and exact saved file bytes. Recognition and input add overhead, so these times are not intrinsic paint or keyboard latency. The visible detection intervals in these runs span 0.23–1.00 s between the last negative capture (or launch, if none) and first positive capture. The ten open paths were verified in each app's saved workspace after its memory sample. Host load varied between launches, so these startup figures do not establish an isolated speed comparison.

The sandbox does not expose `/dev/fuse`, so Obsidian was run from a one-time extraction of the exact installed AppImage. Its startup excludes normal AppImage mount overhead. Xvfb and measurement tools are excluded from both apps' process totals. PSS apportions shared resident pages, so other processes sharing those pages can change it; summed RSS can count shared pages more than once. CPU samples are spaced about 0.5 s apart; a short-lived child that appears and exits between samples may be missed. The full procedure and executable measurement harness are in [method.md](method.md).

## Raw size data

| Item | Bytes | SHA-256 or source |
| --- | ---: | --- |
| Rustidian 0.1.6 `.deb` | 29,456,822 | `cf49f8b0cb03af8b1065522c19f95d70ac91ad238ef666b2223e7f87754ef55f` |
| Rustidian 0.1.6 `.tar.gz` | 28,303,985 | `debec817b677a762b5dd435390d14b1500049560bc3473b5fc6845f08e33968c` |
| Rustidian 0.1.6 `.deb` declared installed size | 56,078,336 | Debian metadata: 54,764 KiB. |
| Obsidian 1.5.12 AppImage | 113,266,922 | Image hash above; byte count matches official release metadata. |
| Rustidian 0.1.7 active installation, allocated | 61,444,096 | App directory excluding `rollback`, launcher, desktop entry and six icons. |
| Rustidian rollback directory, allocated | 115,441,664 | `du -B1` on its rollback directory. |
| Obsidian installation, allocated | 113,430,528 | Installed AppImage, launcher, desktop entry and icon. |

## Raw GUI runs

PSS / RSS columns are MiB; parentheses give the number of processes. Startup columns are seconds; app CPU is a percentage of one logical CPU. Host busy gives whole-host utilization over one second before launch and during the 60-second idle interval. All ten paths matched the expected vault files, and all input checks passed.

| Run | Visible | Usable | One note PSS / RSS (processes) | Ten notes PSS / RSS (processes) | App CPU % | Host busy %: launch / idle | Max °C | Details |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Rustidian 1 | 0.46 | 2.72 | 77.09 / 146.39 (1) | 78.36 / 147.76 (1) | 7.44 | 13.8 / 5.9 | 62 | [JSON](raw-measured-1-rustidian.json) |
| Obsidian 1 | 1.37 | 4.81 | 284.54 / 587.03 (5) | 283.41 / 581.05 (5) | 1.11 | 14.2 / 6.4 | 60 | [JSON](raw-measured-1-obsidian.json) |
| Obsidian 2 | 1.36 | 4.84 | 283.77 / 591.97 (5) | 284.74 / 591.21 (5) | 1.06 | 23.4 / 14.6 | 61 | [JSON](raw-measured-2-obsidian.json) |
| Rustidian 2 | 1.12 | 3.35 | 97.73 / 146.27 (1) | 97.80 / 146.17 (1) | 7.30 | 11.7 / 15.8 | 61 | [JSON](raw-measured-2-rustidian.json) |
| Rustidian 3 | 1.12 | 3.36 | 98.00 / 146.33 (1) | 99.22 / 147.56 (1) | 7.37 | 16.4 / 9.0 | 59 | [JSON](raw-measured-3-rustidian.json) |
| Obsidian 3 | 1.36 | 4.76 | 284.02 / 588.88 (5) | 284.18 / 587.35 (5) | 1.10 | 9.7 / 7.5 | 59 | [JSON](raw-measured-3-obsidian.json) |

## Machine and run conditions

Ubuntu 26.04.1 LTS; Linux 7.0.0-42-generic; x86-64; Intel Core i9-11900H, 8 cores / 16 threads; 30.5 GiB RAM; `powersave` governor. Each measured launch began with at least 8.47 GiB available; the lowest sampled available memory during the idle windows was 8.13 GiB. The 1-minute host load at launch ranged from 1.79 to 5.77; host busy time during the 60 s CPU windows ranged from 5.9% to 15.8%. The highest sampled CPU temperature in completed runs was 62 °C. Other sessions shared the host; the local PID namespace cannot show every host process. Three runs do not characterize all workloads.

The [event log](round2.log), [vault manifest](vault-manifest.json), and linked JSON files retain run conditions, process-level PSS/RSS, CPU intervals and opened paths. The [frame audit](screenshot-audit.json) records dimensions and hashes of the captured frames. A setup failure caused by the private temporary socket path is preserved in [the failed-attempt log](round2-failed-attempt.log). An earlier session was stopped at 72 °C; [its log](round2-thermal-attempt.log) and [raw records](thermal-attempt/) are kept separate, and none of its samples are included in this table. 15 measured attempts were stopped at or above 65 °C (highest trigger 74 °C); those attempts were discarded and retried after cooling.

## README-ready sentences

On one shared Ubuntu machine, the Rustidian 0.1.7 candidate used a median of 97.73 MiB of proportional process memory with one note open in a ten-note demo vault, compared with 284.02 MiB for Obsidian 1.5.12 across three warm runs. During 60-second idle windows on the same software-rendered display, median CPU use was 7.37% of one logical CPU for Rustidian and 1.10% for Obsidian.
