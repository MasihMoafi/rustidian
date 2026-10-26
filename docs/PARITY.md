# Rustidian feature coverage

Rustidian implements selected Obsidian-style workflows in native Rust. Full parity and an overall performance advantage have not been established.

| Area | Current coverage |
| --- | --- |
| Editing | Markdown Source/Live/Reading, syntax-highlighted text, autosave, Undo/Redo, folding, find/replace |
| Notes | Vault folders, links/backlinks, tags, properties, bookmarks, daily notes, templates, recovery |
| Tables | Cell editing, row/column operations, edge append buttons, bounded resizing and layout Undo |
| Graph | Global/local views, filters, groups/colors, node dragging, camera/preferences persistence |
| Workspace | Tabs, tab dragging, focused-tab navigation, saved layouts, palettes and recent vaults |
| Appearance | Four palettes with matching icons, light/dark appearance and shared native controls |
| Existing Obsidian plugins | Unsupported; requires a JavaScript/browser runtime and API adapters |
| Browser-rendered HTML, PDF and agent tab | Not included |
| Canvas, Bases, mathematics and complete Settings/graph parity | Missing or incomplete |

Release 0.1.2 focuses on table/graph interaction, compact controls, Templates configuration recovery and popup consistency. Native checks cover selected Linux X11 workflows; broader Wayland and other-platform coverage remains open. Refer to each release's BUILDINFO and notes for the checks actually performed on its exact executable.
