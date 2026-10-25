# Obsidian parity coverage

## Identity and Linux trial — 25 October 2026

The Rustidian alpha.2 executable, command, desktop identity and packages use one name. [Actions verification](https://github.com/MasihMoafi/rustidian/actions/runs/40161237943) passes270 Rust checks, Clippy, real package/installer checks and54 native checks across9 optimized workflows. Migration retains existing session/recovery/recent-vault state and supports older running windows. The installer, version command, quick start and issue forms improve the trial/reporting path. Full Obsidian feature and visual parity remain unfinished.

Current contract,21October2026: reproduce every Obsidian feature and interaction
in native Rust, with faster equivalent work. This is the coverage ledger linked
from PLAN.md and GUIDE.md. A source implementation or passing suite is evidence,
not full parity or Masih's acceptance. Record reference version, actual actions,
positive/negative cases, visual review and exact Rust executable with each result.

## Reference and delivery

- User screenshots identify1.14.4; installed isolated reference is
  `/home/masih/.local/opt/Obsidian/Obsidian-1.13.7.AppImage`. Earlier captures used
  1.5.12. Keep version differences visible; inventory the actual running version.
- A read-only copy of the installed1.14.4 update now runs in a disposable profile
  on the1.13.7 installer. Actual General settings confirms1.14.4. The archive
  hash,8 passing Move actions and immutable inventory evidence are recorded in
  ES.md and reports/obsidian-1.14.4-inventory/:30 core plugin entries,213 commands,
  20 visible settings categories, daily/template flows and the51-format matrix.
  The earlier1.13.7 inventory remains unchanged as historical evidence.
- Official feature discovery: [core plugins](https://help.obsidian.md/plugins),
  [editing and formatting](https://help.obsidian.md/syntax),
  [plugin development](https://docs.obsidian.md/Plugins/Getting+started/Build+a+plugin).
  Discovery does not replace exercising the installed application.
- Optimized source12c2e24 is installed and open on a125-note vault copy.
  reports/release-verification.json records260 Rust executions/249 unique,
  all49 native workflow contracts/251 groups,25 focused flows,3 preliminary
  flows, six CPU conditions and the bounded Ferrite comparison. The original
  Actions39790060786 remains failed: its preview test incorrectly required one
  vault session. All48 other full workflows passed. The unchanged optimized
  app passes the corrected original14 preview groups after real two-vault
  switching locally, with the before failure retained. This composed evidence
  supports an early alpha; it does not establish complete Obsidian parity.
- Reference automation uses synthetic notes and a disposable profile only. Never
  automate Masih's real vault/profile or a preview handed to him.

## Shell, editing and files

| Workflow | Current evidence and remaining acceptance |
| --- | --- |
| Vault create/open/recent/switch/remove | Installed12c2e24 retains7 focused vault groups and the complete release coverage: native Browse, cancellation, dirty guards, concurrent recents, registered file roots and per-vault sessions. The new two-vault preview verification also checks actual switching and independent exact saved text. Compare separate-window and complete vault settings next. |
| Explorer, folders, rename/move/Trash | Installed12c2e24 covers existing-file/folder native pickers through File menu, palette and shortcuts (10 focused groups), alongside create, copy, rename/move, collisions, link updating and recoverable Trash. Compact Move controls, focus, both themes and exact bytes are verified. Actual reference menus remain broader: split/window opening, merge, version history, OS actions, complete sorting and drag/drop still need paired implementation. Excluded development folders stay hidden by default. |
| Tabs and history | Installed12c2e24 covers independent mode/history/undo, pin protection, clean restart, Undo close tab and Close all other tabs. All17 focused restoration groups pass, including order, blank/missing/external views, cursor/mode/history, pinned exceptions and Save/Cancel/Discard conflicts; original source negatives are retained. Both themes and compact/conflict captures were inspected. Actual1.14.4 tab/order/availability audits are in reports/obsidian-1.14.4-tab-actions/ and the related retained references. Split groups, stacked tabs, popouts, shared duplicate-document models, abrupt-exit durability and the complete25-row menu remain incomplete. |
| Live/Source/Reading Markdown | Installed12c2e24 renders tasks, editable tables, callouts, headings, links/images, syntax, Mermaid and colored emoji. The shared rgba/span/highlight renderer passes7 focused groups across paragraphs, tables, callouts and hover cards in both themes. Literal HTML remains visible in Source/code intentionally; the reported raw styled-table defect is fixed globally. Real private-copy and sanitized controls remain local. Live nested-callout table inset differs from Reading; full syntax/selection/clipboard/drag/indent/IME/bidi parity is incomplete. |
| Inline title, properties and aliases | Editable title/property interactions covered. Complete type editing, metadata UI and all alias/anchor cases need comparison. |
| Link navigation and completion | Held clicks, Ctrl+Enter/new tab, drag-selection negatives and root links are covered; Masih accepted Approved spec in4dbb842. Installed12c2e24 adds native hover cards under actual1.14.4 mode/modifier rules, relative/missing targets, task editing and exact saved-byte/Undo checks. The452px card is contained in both themes and compact windows. Complete URI actions and nested-card behavior remain unfinished. |
| Hover/focus/window controls | Generated controls, hover and real minimize/maximize/restore/close have native coverage. Audit every menu/focus/shortcut/compact state against current reference. |
| Settings, themes and hotkeys | Working theme/zoom/autosave plus available shortcuts. Full categorized settings, customization, CSS/theme compatibility and remappable shortcuts are incomplete. |
| Saving and external changes | Atomic/conflict-safe saves and per-vault sessions covered. Installed snapshot recovery passes all13 optimized focused checks and full regressions; concurrent-window editing policy and workspace interchange remain incomplete. |
| Attachments and non-Markdown | Native images and text documents covered. PDF/audio/video/web embeds, audio capture, attachment management and arbitrary file associations remain incomplete. |
| Formatting | Installed12c2e24 includes the five requested colored highlights, plus the observed six native marker toggles with13 focused groups, exact undo/redo and both-theme visual inspection. Cross-span/multiline/IME behavior still needs paired evidence. Document-only font controls remain on isolated work/font-controls-20261022, uncompiled/unuploaded; their reference audit and old missing-command negative are retained. The current heading baseline differs from Obsidian. Math, all footnote behavior and complete Obsidian-flavored Markdown remain incomplete. |

## Every core plugin

This list follows the observed core-plugin inventory, including disabled plugins,
and retains the historical Format converter row. Actual1.14.4 has30 core entries;
1.13.7 had31.
“Covered” below means existing native checks for the stated subset; every row
still needs the expanded paired reference audit before a parity claim.

| Core plugin | Native status / next observable check |
| --- | --- |
| Audio recorder | Missing; record, stop, attach, play and cancel/failure. |
| Backlinks | Linked-note coverage; unlinked mentions, filters and pane actions incomplete. |
| Bases | Missing; open/create .base, property-backed views, sort/filter/edit. |
| Bookmarks | File bookmarks/reorder/rename/restart covered; groups, headings, searches and other targets missing. |
| Canvas | Missing; native .canvas interchange, cards/groups/connectors, interaction and recovery. |
| Command palette | Available commands and fuzzy matching covered; full core-command registry, pinning and customized hotkeys incomplete. |
| Daily notes | Installed9febfbb passes the complete optimized release and8 focused note-aid checks covering date-named create/reopen, calendar action, configured folder/format/template and read-only settings import. Paired1.13.7 create/reopen reference passes. Startup, previous/next, date-property links and full settings parity remain incomplete. |
| Editing mode status | Native status bar shows the current mode; complete reference click/menu/accessibility behavior unverified. |
| File explorer | Covered subset; complete menus, sorting and drag/drop audit pending. |
| File recovery | Installed9febfbb passes all13 optimized focused checks and the complete corrected ecad9cb release, including exact Copy/diff/Restore, unopened notes, unreadable storage, dirty/conflict protection, deleted-note recreation, restart, vault isolation, Clear and both themes/compact controls. Paired1.13.7 reference covers these flows,5min/7day defaults and fixed displayed-version lists while new snapshots arrive. Broader retention/service equivalence still needs paired evidence. |
| Footnotes view | Missing panel; reference syntax/jump/backlink behavior unverified. |
| Format converter | Historical disabled plugin in1.13.7; absent as a plugin in1.14.4, which still exposes Frontmatter migration. Conversion behavior is unverified. |
| Graph view | Installed12c2e24 has optimized global/local graphs and passes9 focused groups plus the complete graph workflow: actual quick depth1/2/3 clicks, topology filters, pan/zoom, node navigation, both themes, restart and exact bytes. Persistent creation/sidebar markers pass3 focused groups. The actual installed125-note copy is open; its protected handoff is reports/feedback-graph-12c2e24/preview.json. Reference1.14.4 has139 nodes/181 directed links versus128/170 native on that copy because unresolved non-Markdown/image/folder targets differ. Dense labels can still overlap. Missing dotted-target creation, attachment semantics, full context menus, settings/camera persistence and animation remain incomplete; the synthetic reference follow-up retains their observed contracts. |
| Note composer | Missing; extract/merge with source/link preservation and undo/cancellation. |
| Outgoing links | Existing linked-target panel; unlinked mentions and complete actions incomplete. |
| Outline | Heading navigation/folding covered; full context/drag actions unverified. |
| Page preview | Installed12c2e24 passes15 focused groups and all14 workflow groups after actual two-vault switching: dirty targets, separate Undo, external updates, properties, relative/missing links, graph-backed note saves, both themes and compact geometry. The original CI failure and native before/after test-scope correction are retained in reports/preview-session-verification; no app assertion was removed. Shared styled-table rendering is covered. Nested cards/settings and duplicate-view shared editing remain incomplete. |
| Properties view | Existing note properties/query support; all-properties pane/type management incomplete. |
| Publish | Missing integration; requires supported service or an explicitly chosen equivalent. Publishing private notes requires authorization. |
| Quick switcher | Installed12c2e24 passes6 focused groups and full coverage for exact titles, multiword/Unicode aliases, abbreviations, empty chooser and shared-prefix negatives. Paired1.14.4 evidence retains the old false match. Broader ranking/modifier/empty-state equivalence remains incomplete. |
| Random note | Installed9febfbb passes all8 optimized creator checks and the complete corrected release, including blank-tab and empty-vault selection. Paired1.13.7 opens a valid note from a blank tab and creates no files in an empty vault; its failed-open/busy notice is retained in reports/obsidian-note-creators-reference.json. No full-product parity claim. |
| Search | Native queries/virtualized results/property search covered; full operators, saved query embeds and controls unverified. |
| Slash commands | Missing; typed slash chooser with normal-text/code negatives. |
| Slides | Missing; Markdown slides, presentation/navigation and export behavior. |
| Sync | Missing integration; service compatibility, offline/concurrent edits, encryption and recovery require evidence. |
| Tags view | Inline/property query support; dedicated nested tag pane and actions incomplete. |
| Templates | Installed9febfbb passes the complete optimized release and8 focused note-aid checks covering folder/chooser/ribbon, caret insertion, title/date/time tokens, format settings and undo. Paired1.13.7 insertion, unknown tokens and single undo/redo are retained with51 date-format observations. Frontmatter merging, localized formats and full settings parity remain incomplete. |
| Unique note creator | Installed9febfbb passes all8 optimized creator checks and the complete corrected release: both themes/compact settings, collision/template errors, link/Escape/caret/undo/redo and vault isolation. Paired1.13.7 evidence covers same-prefix minute advancement, folder/template/format, missing folders/templates, literal collisions and internal links without file creation. Remaining format/editor edge cases still need paired checks. |
| Web viewer | Missing; browser content conflicts with the current native-only runtime constraint and needs a concrete design decision. |
| Word count | Existing document counts; selection, CJK/Unicode and settings parity unverified. |
| Workspaces | Installed9febfbb passes all10 corrected optimized workspace checks and the complete ecad9cb release. Actual pin true/false restoration, named save/load/overwrite/delete/cancel, modes/order/sidebar widths and dirty-load guards are covered. Failed obsolete-schema evidence is retained; it was corrected and rerun before installation. Workspace interchange, multiple windows/groups and abrupt-exit durability remain incomplete. |

## Community extensions, services and platforms

The real vault's read-only plugin manifest lists Excalidraw, Highlightr, Color
Highlighter and agent-mcp. Five text-highlight colors exist natively. Other plugin
equivalents and arbitrary Obsidian JavaScript plugin compatibility are unfinished;
do not silently replace plugin execution with inactive settings. Discover plugin
APIs/dependencies before proposing a compatibility mechanism. Keep any architecture
conflict with the native Rust/no-browser constraint explicit.

Importer and Maps are officially maintained community plugins and remain in the
audit. CSS snippets/themes, .obsidian setting interoperability, Obsidian URI,
desktop/mobile platforms, accessibility, localization, updates, export/print,
account/service integration and extensions need their own paired workflow cases.
Linux is the currently built platform; other platforms are unverified.

## Increment order and acceptance

1. Preserve the installed12c2e24 update,9feb rollback, accepted scrolling,
   original files and all user-owned preview vaults/profiles.
2. Use the current guide and this ledger for the next reference workflow. The
   global inline fixes, actual graph and existing-file/folder paths are delivered;
   do not restart already completed work from old failure summaries.
3. Retain the remaining shared-view, graph-semantic, settings, extension/service
   and platform gaps. Masih sets priorities and decides acceptance.
4. Each increment needs exact executable identity, actual user actions,
   inspected visuals and equivalent-work measurements. Keep a CI failure visible
   even when a separately proven test-only correction completes verification.

Full parity is complete only when all applicable rows have paired behavioral,
visual and performance evidence and Masih accepts the result. Unsupported service
access or plugin/runtime conflicts stay visible until resolved.
