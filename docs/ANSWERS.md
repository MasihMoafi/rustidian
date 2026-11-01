# Your Rustidian answers — 28 October 2026

**Installed locally:** [ES.md](../ES.md) names the build and its rollback. The default vault is `~/Desktop/o`; a file outside it opens with its own folder. Vault opening runs in the background and cancels cleanly. The Agent tab is a terminal that runs Elpis, Codex or Claude in the vault. [Debian package candidate](../reports/launch-prep-20261028/rustidian-linux-amd64.deb).

**Privacy:** package/source checks passed; older public Git history still contains earlier session files.

- **Obsidian plugins:** **Unsupported today.** A compatibility layer is feasible, but browser/JavaScript hosting and [Obsidian API adapters](https://github.com/obsidianmd/obsidian-api/blob/master/obsidian.d.ts) remain unfinished. The browser-host prototype is separate.
- **Frontend/style:** Native **Rust + egui/eframe/OpenGL**. shadcn React and daisyUI CSS cannot run directly; shared native controls implement their patterns.
- **Graph/tables:** Graph dragging, colors, filters, camera persistence and cancellation are checked. Tables support resizing, bottom/right “+” controls and Undo. Use Settings → Display → Animate for chronological playback; Stop restores the full graph.
- **Tabs and deletion:** Tab dragging and focused-tab arrow navigation exist. Delete/D applies in its menu. Files move to recoverable system Trash; “Don’t ask again” persists after confirmed deletion.
- **Files:** UTF-8 code/config highlighting and Markdown images work. Browser-style HTML, native PDF, audio/video and web embeds remain unfinished. Generated/dependency folders are excluded.
- **Themes/Agent:** Four palettes have matching icons; System follows OS appearance. Click Agent at the top right or press Ctrl+Shift+A. The Agent tab shows the agent's own screen live; its menu lists Elpis, Codex and Claude when they are installed, and ↻ restarts the agent.
- **Math/mouse:** Inline/display LaTeX renders in Reading, Split, Live, tables and callouts. Source remains editable. Middle-drag pans notes/graph; middle-click links opens tabs.
- **GPL/Notepad++:** GNU General Public License is open source. [Notepad++ uses GPLv3](https://github.com/notepad-plus-plus/notepad-plus-plus/blob/master/LICENSE); Rustidian uses MIT. Advanced Notepad++ workflows remain incomplete.
- **Reuse:** Excalidraw is React/TypeScript/browser canvas, MIT. Rust can host it. [egui-snarl](https://github.com/zakarumych/egui-snarl) fits native nodes; [xyflow](https://github.com/xyflow/xyflow) fits Elpis workflows. Blender/Fusion are interaction references.
- **Old CI error:** “0 tests” was an empty target; `watcher_behavior` failed elsewhere. Subsequent release checks passed.
- **Publication/outreach:** This increment is local; [public0.1.2](https://github.com/MasihMoafi/rustidian/releases/tag/v0.1.2) is unchanged. SEO topics/description were corrected; stars are not guaranteed. [TWiR8915](https://github.com/rust-lang/this-week-in-rust/pull/8915) is closed: contributor calls require supported tasks, not marketing. Your draft remains unsent.

**Speed and stability** remain requirements; full parity and overall speed superiority are unproven. **Fully quit all Rustidian windows and reopen to activate the installed build.**
