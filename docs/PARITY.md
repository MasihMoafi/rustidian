# Obsidian parity coverage

Current contract,21October2026: reproduce every Obsidian feature and interaction
in native Rust, with faster equivalent work. This is the coverage ledger linked
from PLAN.md and GUIDE.md. A source implementation or passing suite is evidence,
not full parity or Masih's acceptance. Record reference version, actual actions,
positive/negative cases, visual review and exact Rust executable with each result.

## Reference and delivery

- User screenshots identify1.14.4; installed isolated reference is
  `/home/masih/.local/opt/Obsidian/Obsidian-1.13.7.AppImage`. Earlier captures used
  1.5.12. Keep version differences visible; inventory the actual running version.
- Official feature discovery: [core plugins](https://help.obsidian.md/plugins),
  [editing and formatting](https://help.obsidian.md/syntax),
  [plugin development](https://docs.obsidian.md/Plugins/Getting+started/Build+a+plugin).
  Discovery does not replace exercising the installed application.
- Source048fced is installed after harness e326f14 normal Actions39658870633
  passed every gate:133 Rust executions,115 full GUI groups and six CPU conditions.
  All7 focused vault groups pass. ES.md owns exact current
  run/artifact identity; reports/release-verification.json owns accepted gates.
- Reference automation uses synthetic notes and a disposable profile only. Never
  automate Masih's real vault/profile or a preview handed to him.

## Shell, editing and files

| Workflow | Current evidence and remaining acceptance |
| --- | --- |
| Vault create/open/recent/switch/remove | Installed048fced passes7 focused groups and the complete release, including real Browse, cancellation, dirty guards, concurrent recents and registered file roots. Compare separate-window and vault-setting behavior next. |
| Explorer, folders, rename/move/Trash | Native behavioral coverage; preserve excluded development folders and recoverable Trash. Full context-menu, sort/filter and drag/drop parity still require a reference audit. |
| Tabs and history | Native per-tab mode/history/undo, ordinary/modified links and session restoration covered. Split groups, stacked tabs, popouts, pinning and tab menus remain incomplete. |
| Live/Source/Reading Markdown | Existing native tasks, tables/cell editing, callouts, headings, links/images, syntax and Mermaid have focused coverage. Complete syntax/selection/clipboard/drag/indent/IME/bidi parity remains incomplete. |
| Inline title, properties and aliases | Editable title/property interactions covered. Complete type editing, metadata UI and all alias/anchor cases need comparison. |
| Link navigation and completion | Held click, Ctrl+Enter/new tab, drag-selection negative and root links covered; Masih accepted Approved spec in4dbb842. Full hover previews and URI actions remain incomplete. |
| Hover/focus/window controls | Generated controls, hover and real minimize/maximize/restore/close have native coverage. Audit every menu/focus/shortcut/compact state against current reference. |
| Settings, themes and hotkeys | Working theme/zoom/autosave plus available shortcuts. Full categorized settings, customization, CSS/theme compatibility and remappable shortcuts are incomplete. |
| Saving and external changes | Atomic/conflict-safe saves and per-vault sessions covered. Snapshot recovery UI, concurrent-window editing policy and workspace interchange remain incomplete. |
| Attachments and non-Markdown | Native images and text documents covered. PDF/audio/video/web embeds, audio capture, attachment management and arbitrary file associations remain incomplete. |
| Formatting | Five highlight colors are implemented. Math, all footnote behaviors and complete Obsidian-flavored Markdown need direct reference checks. |

## Every core plugin

This list follows the official core-plugin inventory, including disabled plugins.
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
| Daily notes | Source written for date-named create/open, configured folder/format/template and read-only settings import; pending Actions. Paired1.13.7 create/reopen reference passes. Startup/previous-next/date-property links and ribbon/settings parity still incomplete. |
| Editing mode status | Native status bar shows the current mode; complete reference click/menu/accessibility behavior unverified. |
| File explorer | Covered subset; complete menus, sorting and drag/drop audit pending. |
| File recovery | Missing; periodic snapshots, browse/diff/restore, conflict and retention rules. |
| Footnotes view | Missing panel; reference syntax/jump/backlink behavior unverified. |
| Format converter | Legacy plugin present but disabled in the installed1.13.7 inventory; import/conversion workflows unverified. |
| Graph view | Missing; global/local graph, filters, search, groups and navigation. |
| Note composer | Missing; extract/merge with source/link preservation and undo/cancellation. |
| Outgoing links | Existing linked-target panel; unlinked mentions and complete actions incomplete. |
| Outline | Heading navigation/folding covered; full context/drag actions unverified. |
| Page preview | Missing complete hover preview/pinned interaction behavior. |
| Properties view | Existing note properties/query support; all-properties pane/type management incomplete. |
| Publish | Missing integration; requires supported service or an explicitly chosen equivalent. Publishing private notes requires authorization. |
| Quick switcher | Native open/create/link completion covered; complete ranking/modifier/empty-state audit pending. |
| Random note | Missing; nonempty/empty vault behavior. |
| Search | Native queries/virtualized results/property search covered; full operators, saved query embeds and controls unverified. |
| Slash commands | Missing; typed slash chooser with normal-text/code negatives. |
| Slides | Missing; Markdown slides, presentation/navigation and export behavior. |
| Sync | Missing integration; service compatibility, offline/concurrent edits, encryption and recovery require evidence. |
| Tags view | Inline/property query support; dedicated nested tag pane and actions incomplete. |
| Templates | Source written for folder/chooser, caret insertion, title/date/time tokens, format settings and undo; pending Actions. Paired1.13.7 insertion, unknown tokens and single undo/redo captured;51 date-format fixtures retained. Frontmatter merging, localized formats, ribbon and full settings parity still incomplete. |
| Unique note creator | Missing; configured time title/template, repeated-instant collisions and error paths. |
| Web viewer | Missing; browser content conflicts with the current native-only runtime constraint and needs a concrete design decision. |
| Word count | Existing document counts; selection, CJK/Unicode and settings parity unverified. |
| Workspaces | Only automatic session restore exists; named layouts/save/load/delete and safe dirty documents missing. |

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

1. Completed048fced vault release/install; retain its rollback, original data and
   accepted scrolling/navigation behavior while extending the product.
2. Capture the installed reference's command/plugin/settings inventory and real
   daily-note/template workflows. Implement these note-creation workflows through
   existing safe file/edit/undo paths; test missing folders/templates and repeated
   creation as well as normal insertion.
3. Repeat for named workspaces, recovery and the remaining editing/navigation
   gaps; then knowledge views, attachments, extensions/services and platforms.
   User feedback may change this implementation order; no row silently disappears.
4. For each increment run focused correctness checks on the exact Actions app,
   inspect both themes/compact states, preserve all prior workflows, and run the
   applicable serial performance gates. Compare equivalent saved output and
   latency on the same host; Rust alone proves no speed advantage.

Full parity is complete only when all applicable rows have paired behavioral,
visual and performance evidence and Masih accepts the result. Unsupported service
access or plugin/runtime conflicts stay visible until resolved.
