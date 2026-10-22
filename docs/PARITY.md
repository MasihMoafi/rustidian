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
- Source9febfbb is installed after corrected harness ecad9cb Actions39704845915
  passed every gate:172 Rust executions/166 unique,164 GUI checks across40 full
  workflows, all15 focused workflows and six CPU conditions. ES.md owns exact
  run/artifact identity; reports/release-verification.json owns accepted gates.
- Reference automation uses synthetic notes and a disposable profile only. Never
  automate Masih's real vault/profile or a preview handed to him.

## Shell, editing and files

| Workflow | Current evidence and remaining acceptance |
| --- | --- |
| Vault create/open/recent/switch/remove | Installed9febfbb passes7 focused groups and the complete release, including real Browse, cancellation, dirty guards, concurrent recents and registered file roots. Compare separate-window and vault-setting behavior next. |
| Explorer, folders, rename/move/Trash | Native behavioral coverage includes file-menu Open in new tab, Make a copy, Bookmark, Copy path, Rename and Delete. Actual1.13.7 file actions prove immediate numbered copy, full-basename suffixes and binary copies. A separate five-action Move audit passes filtered cancellation, existing/root/nested destinations, conditional link-update choices and automatic numbered destinations on collision, with exact bytes and resolved aliases/Markdown targets. Native immediate copy/menu/palette and atomic collision/concurrency/byte behavior pass the installed9feb optimized/full gates; all5 focused copy checks pass. Source63321cf passes199 Rust executions and all11 early Move groups, but its oversized link dialog was visually rejected and its optimized build cancelled. The measured typography/hover/focus/close correction is now in the combined local candidate; exact optimized/native/performance verification remains pending. It retains numbered collision handling and separate link-update choices. Preserve excluded development folders and recoverable Trash. Current1.13.7 file-menu audit records13 ordered actions with13px/24.9px rows; seven actions still need paired implementation/verification, including split/window opening, separate Move, merge, version history and OS opening. Folder menus, complete sorting/filtering and drag/drop remain incomplete. |
| Tabs and history | Native per-tab mode/history/undo, ordinary/modified links and clean-exit session restoration covered; abrupt-exit layout durability remains incomplete. Installed9feb passes all7 optimized pin checks and full regressions, including clean restart/per-vault flags, pin protection/persistence, its glyph/menu and five palette tab commands. Actual1.13.7 tab-actions audit passes pin/navigation and first-Ctrl+W-unpins/second-closes plus split/stack/group-close/undo-close/close-others with unchanged notes. Current1.14.4 now also passes12 actual tab actions with inspected split/stack/menu captures in reports/obsidian-1.14.4-tab-actions/. Native undo-close and Close all other tabs are integrated in9b5eafc and pass all17 early native groups; optimized compilation passes, complete verification is pending: contextual availability, blank/missing/external views, saved cursor/mode/history, pinned exceptions, observed bulk restore order and conflict Save/Cancel/Discard have prepared actual-window checks. The paired1.14.4 order/availability audits pass9/7 actions; the installed9feb missing-command negative passes its four-tab/pin/editor controls then fails on the absent command. All remain subject to the active normal45/20/10 Actions run39721548486 and optimized visual review. Split groups, stacked tabs, popouts and the complete25-row menu remain incomplete. |
| Live/Source/Reading Markdown | Native tasks, tables/cell editing, callouts, headings, links/images, syntax and Mermaid have focused coverage. Shared rgba/span/highlight fixc7 passes227 Rust executions and7 exact optimized native groups across Reading/Live/Source, both themes, paragraph/table/callout/hover and exact bytes; its full run failed two retained regressions and is not installed. The actual private legacy note and sanitized wide-table positive/negative are verified. The old native defect is proven. Existing Live nested-callout table inset differs from Reading. Complete syntax/selection/clipboard/drag/indent/IME/bidi parity remains incomplete. |
| Inline title, properties and aliases | Editable title/property interactions covered. Complete type editing, metadata UI and all alias/anchor cases need comparison. |
| Link navigation and completion | Held click, Ctrl+Enter/new tab, drag-selection negative and root links covered; Masih accepted Approved spec in4dbb842. A current1.14.4 six-case preview audit confirms the mode/modifier rules and452px card. Native hover cards and complete URI actions remain missing. Supplemental1.14.4 evidence covers task editing, focused-card dismissal, current-tab corner/internal links and a missing-note create card. |
| Hover/focus/window controls | Generated controls, hover and real minimize/maximize/restore/close have native coverage. Audit every menu/focus/shortcut/compact state against current reference. |
| Settings, themes and hotkeys | Working theme/zoom/autosave plus available shortcuts. Full categorized settings, customization, CSS/theme compatibility and remappable shortcuts are incomplete. |
| Saving and external changes | Atomic/conflict-safe saves and per-vault sessions covered. Installed snapshot recovery passes all13 optimized focused checks and full regressions; concurrent-window editing policy and workspace interchange remain incomplete. |
| Attachments and non-Markdown | Native images and text documents covered. PDF/audio/video/web embeds, audio capture, attachment management and arbitrary file associations remain incomplete. |
| Formatting | The five user-requested highlight colors are implemented. Actual1.14.4 also exposes Blue/Purple and six native toggle commands; their six marker-based syntaxes, Reading and exact undo/redo now pass an isolated1.14.4 audit in reports/obsidian-1.14.4-highlights/, but native parsing/toggles and real Source marker glyphs pass all13 debug GUI groups in Actions39713215822, with exact images/identity retained; the next combined source still needs optimized/full/performance verification; repeated/partial/caret behavior and both themes now have actual1.14.4 evidence. Cross-span/multiline/IME parity remains unverified. Document-only font controls are prepared on the isolated work/font-controls-20261022 branch, uncompiled/unuploaded. Actual1.14.4 direction/mode/global-note/theme/boundary/restart audits and an installed9feb missing-command negative are retained;8 new native groups await a later normal Actions candidate. The existing heading baseline differs from the latest reference and is not silently changed by relative sizing. Math, all footnote behaviors and complete Obsidian-flavored Markdown remain incomplete. |

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
| Graph view | Native global/local model, bounded layout and panes are implemented in the current uncompiled candidate. Actual1.14.4 global/local audit covers query/groups/pan/zoom/navigation, linked local depth1..5 and direction/neighbor controls. The exact old app passes editing/palette controls then fails the missing graph action. New native checks cover reference counts, physical filters/zoom/pan/node clicks, linked local pane, same-path Graph/document restart and exact bytes. Combined Actions and optimized visual/performance verification remain mandatory. Local settings/camera persistence, animation and complete graph/attachment/alias parity remain incomplete. |
| Note composer | Missing; extract/merge with source/link preservation and undo/cancellation. |
| Outgoing links | Existing linked-target panel; unlinked mentions and complete actions incomplete. |
| Outline | Heading navigation/folding covered; full context/drag actions unverified. |
| Page preview | Installed9feb lacks cards. Source9b5 passes14 early native groups,20 optimized focused workflows and the complete Page Preview full workflow, including unsaved targets, separate Undo, external updates, relative/missing links, both themes and compact geometry. Its overall45-workflow run failed elsewhere; no release approval. Exact1.14.4 mode/modifier/task/corner/link/height evidence is retained. The next global inline fix adds matching styled-table hover coverage. Nested cards/settings/shared duplicate-view editing remain incomplete. |
| Properties view | Existing note properties/query support; all-properties pane/type management incomplete. |
| Publish | Missing integration; requires supported service or an explicitly chosen equivalent. Publishing private notes requires authorization. |
| Quick switcher | Native open/create/link completion covered. Paired1.14.4 exact-title queries exposed a native shared-prefix false match; the one-Atom correction and actual chooser checks frombaaac935 are integrated in9b5eafc and pass all6 early native groups; optimized/full verification is pending. Broader ranking/modifier/empty-state parity remains incomplete. |
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

1. Preserve the installed9feb release, rollback, original data and accepted
   scrolling/navigation. The failed9b5 candidate is evidence, not a release.
2. Fix the reported inline-formatting defect globally; verify the actual old
   failure and new Reading/Live/Source/table/callout/preview behavior. Retain all
  45 old workflows, both themes, exact files and applicable performance gates.
3. Implement and verify global and local graph in parallel under the actual reference contract in
   PLAN.md. Continue the remaining rows, including shared views, settings,
   extensions/services and platforms; no earlier agent priority omits a feature.
4. Each increment needs exact Actions executable identity, actual user actions,
   inspected visuals and equivalent-work performance evidence. Masih decides
   acceptance; Rust alone proves no speed advantage.

Full parity is complete only when all applicable rows have paired behavioral,
visual and performance evidence and Masih accepts the result. Unsupported service
access or plugin/runtime conflicts stay visible until resolved.
