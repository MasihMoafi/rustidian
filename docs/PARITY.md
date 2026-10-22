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
| Explorer, folders, rename/move/Trash | Native behavioral coverage includes file-menu Open in new tab, Make a copy, Bookmark, Copy path, Rename and Delete. Actual1.13.7 file actions prove immediate numbered copy, full-basename suffixes and binary copies. A separate five-action Move audit passes filtered cancellation, existing/root/nested destinations, conditional link-update choices and automatic numbered destinations on collision, with exact bytes and resolved aliases/Markdown targets. Native immediate copy/menu/palette and atomic collision/concurrency/byte behavior pass the installed9feb optimized/full gates; all5 focused copy checks pass. The paired actual1.14.4 Move candidate is integrated locally but uncompiled; it adds numbered collision handling and separate link-update choices. Preserve excluded development folders and recoverable Trash. Current1.13.7 file-menu audit records13 ordered actions with13px/24.9px rows; seven actions still need paired implementation/verification, including split/window opening, separate Move, merge, version history and OS opening. Folder menus, complete sorting/filtering and drag/drop remain incomplete. |
| Tabs and history | Native per-tab mode/history/undo, ordinary/modified links and clean-exit session restoration covered; abrupt-exit layout durability remains incomplete. Installed9feb passes all7 optimized pin checks and full regressions, including clean restart/per-vault flags, pin protection/persistence, its glyph/menu and five palette tab commands. Actual1.13.7 tab-actions audit passes pin/navigation and first-Ctrl+W-unpins/second-closes plus split/stack/group-close/undo-close/close-others with unchanged notes. Split groups, stacked tabs, popouts, undo-close and the complete25-row tab menu remain incomplete natively. |
| Live/Source/Reading Markdown | Existing native tasks, tables/cell editing, callouts, headings, links/images, syntax and Mermaid have focused coverage. Complete syntax/selection/clipboard/drag/indent/IME/bidi parity remains incomplete. |
| Inline title, properties and aliases | Editable title/property interactions covered. Complete type editing, metadata UI and all alias/anchor cases need comparison. |
| Link navigation and completion | Held click, Ctrl+Enter/new tab, drag-selection negative and root links covered; Masih accepted Approved spec in4dbb842. A current1.14.4 six-case preview audit confirms the mode/modifier rules and452px card. Native hover cards and complete URI actions remain missing. Supplemental1.14.4 evidence covers task editing, focused-card dismissal, current-tab corner/internal links and a missing-note create card. |
| Hover/focus/window controls | Generated controls, hover and real minimize/maximize/restore/close have native coverage. Audit every menu/focus/shortcut/compact state against current reference. |
| Settings, themes and hotkeys | Working theme/zoom/autosave plus available shortcuts. Full categorized settings, customization, CSS/theme compatibility and remappable shortcuts are incomplete. |
| Saving and external changes | Atomic/conflict-safe saves and per-vault sessions covered. Installed snapshot recovery passes all13 optimized focused checks and full regressions; concurrent-window editing policy and workspace interchange remain incomplete. |
| Attachments and non-Markdown | Native images and text documents covered. PDF/audio/video/web embeds, audio capture, attachment management and arbitrary file associations remain incomplete. |
| Formatting | The five user-requested highlight colors are implemented. Actual1.14.4 also exposes Blue/Purple and six native toggle commands; their six marker-based syntaxes, Reading and exact undo/redo now pass an isolated1.14.4 audit in reports/obsidian-1.14.4-highlights/, but native parsing/toggles are now integrated locally from6354fb7/4d8369d but remain uncompiled; repeated/partial/caret behavior and both themes now have actual1.14.4 evidence. Cross-span/multiline/IME parity remains unverified. Editor-only font size commands, math, all footnote behaviors and complete Obsidian-flavored Markdown remain incomplete. |

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
| Daily notes | Exacta104d89 passes the focused note-aids workflow with date-named create/open, calendar ribbon action, configured folder/format/template and read-only settings import; preview only, not installed. Paired1.13.7 create/reopen reference passes. Startup/previous-next/date-property links and full settings parity still incomplete. |
| Editing mode status | Native status bar shows the current mode; complete reference click/menu/accessibility behavior unverified. |
| File explorer | Covered subset; complete menus, sorting and drag/drop audit pending. |
| File recovery | Native storage/capture/restore is compiled with13 passing recovery behavior tests. All12 early native groups plus startup now pass, including exact Copy/diff/Restore, unopened notes, unreadable storage, dirty/conflict protection, deleted-note recreation, restart, vault isolation, Clear and compact/both-theme controls. Complete optimized release checks and the minor status-message visual correction remain pending. Paired1.13.7 audit verifies exact Copy/Restore, older-version selection, Show changes, settings defaults5min/7days, both cancelled/confirmed Clear, deleted-note recreation and two external versions of a never-opened note with unchanged originals. Actual1.13.7 live-update evidence keeps the displayed list fixed until file choice/reopening. Retain every native workflow and inspect the final optimized screenshots before installation. |
| Footnotes view | Missing panel; reference syntax/jump/backlink behavior unverified. |
| Format converter | Historical disabled plugin in1.13.7; absent as a plugin in1.14.4, which still exposes Frontmatter migration. Conversion behavior is unverified. |
| Graph view | Missing; global/local graph, filters, search, groups and navigation. |
| Note composer | Missing; extract/merge with source/link preservation and undo/cancellation. |
| Outgoing links | Existing linked-target panel; unlinked mentions and complete actions incomplete. |
| Outline | Heading navigation/folding covered; full context/drag actions unverified. |
| Page preview | Native card still missing. Actual1.13.7 six-case Reading/Live/Source × plain/Ctrl audit passes target content, unchanged active document, pointer-away dismissal and unchanged bytes. Reading needs no Ctrl; both editing modes require it by default. Pinned-card/navigation/settings behavior remains unverified. |
| Properties view | Existing note properties/query support; all-properties pane/type management incomplete. |
| Publish | Missing integration; requires supported service or an explicitly chosen equivalent. Publishing private notes requires authorization. |
| Quick switcher | Native open/create/link completion covered; complete ranking/modifier/empty-state audit pending. |
| Random note | Native selection/command compiles in512 and its Markdown/text/empty filesystem tests pass; all8 creator native checks pass on Cea debug, including blank-tab/empty-vault selection; optimized/full gates remain pending. Paired1.13.7 reference opens valid notes from a blank tab and creates no files in an empty vault; the empty reference also displays a failed-open/busy notice. Original notes remain unchanged. reports/obsidian-note-creators-reference.json records the reference outcome; native coverage is pending. |
| Search | Native queries/virtualized results/property search covered; full operators, saved query embeds and controls unverified. |
| Slash commands | Missing; typed slash chooser with normal-text/code negatives. |
| Slides | Missing; Markdown slides, presentation/navigation and export behavior. |
| Sync | Missing integration; service compatibility, offline/concurrent edits, encryption and recovery require evidence. |
| Tags view | Inline/property query support; dedicated nested tag pane and actions incomplete. |
| Templates | Exacta104d89 passes focused note-aids checks for folder/chooser/ribbon action, caret insertion, title/date/time tokens, format settings and undo; preview only, not installed. Paired1.13.7 insertion, unknown tokens and single undo/redo captured;51 date-format fixtures retained. Frontmatter merging, localized formats and full settings parity still incomplete. |
| Unique note creator | Native creation/settings/import and editor-only internal links compile in512 with passing filesystem/availability tests. all8 creator native checks pass on Cea debug, including settings in both themes/compact, collision/template errors, link/Escape/caret/undo/redo and vault isolation; optimized/full gates remain pending. Paired1.13.7 audits cover same-prefix minute advancement, folder/template/format, missing folders/templates, literal collision errors and bare-name internal links with no file creation, single undo/redo and caret inside brackets. Raw reports obsidian-{note-creators,unique-settings,unique-links,unique-links-configured}-reference.json retain exact outcomes. The complete native creator/link GUI and optimized release checks remain pending. |
| Web viewer | Missing; browser content conflicts with the current native-only runtime constraint and needs a concrete design decision. |
| Word count | Existing document counts; selection, CJK/Unicode and settings parity unverified. |
| Workspaces | Source9feb adds pin metadata to saved tabs; its old exact-schema native assertion fails because it omits pinned:false. The actual data is retained in reports/workspace-pin-schema-9febfbb/. All10 corrected schema/pinned-layout checks and all8 corrected keyboard checks pass on the unchanged optimized app; the complete corrected reuse gate remains pending, with no installation. Paired1.13.7 reference passes save/load/overwrite/delete/cancel, order/modes/sidebar widths and actual restored-tab clicks with unchanged notes. Native named storage/manager, four commands, dirty-load guards and sidebar width restoration are now written with behavioral checks; exacta104d89 passes all8 focused workspace groups plus startup, including row visibility and width restoration; preview only, not installed because a separate attachments-driver failure blocked that release. Current recovery candidate retains these checks. |

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
