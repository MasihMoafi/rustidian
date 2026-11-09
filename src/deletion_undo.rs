//! Deletions retain byte-for-byte snapshots until undone (or this vault closes).
//! Text-history markers order them with the editor's existing undo groups.
use super::NotesApp;
use eframe::egui::{self, Key, Modifiers};
use rustidian::{tab::Tab, vault::Document};
use std::{
    collections::HashMap,
    fs, io,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Default)]
pub(super) struct History {
    records: Vec<Deletion>,
    sequence: u64,
    editor: Option<(u64, egui::Id)>,
    edits: HashMap<u64, Instant>,
    notice: Option<(String, Instant)>,
}

struct Deletion {
    root: PathBuf,
    entries: Vec<Snapshot>,
    tabs: Vec<(usize, Tab)>,
    active: Option<u64>,
    marker: u64,
    baselines: HashMap<PathBuf, String>,
    at: Instant,
}

struct Snapshot {
    original: PathBuf,
    copy: tempfile::TempDir,
    restored: Option<PathBuf>,
}

impl History {
    pub(super) fn clear(&mut self) {
        self.records.clear();
        self.editor = None;
        self.edits.clear();
        self.notice = None;
        // Keep sequence numbers unique across vault switches and old text histories.
    }

    pub(super) fn observe(&mut self, ctx: &egui::Context, root: &Path, tab: &Tab, id: egui::Id) {
        self.editor = Some((tab.id(), id));
        self.mark_document(ctx, root, &tab.document);
    }

    fn mark_document(&mut self, ctx: &egui::Context, root: &Path, document: &Document) {
        if document.is_blank() || document.is_image() {
            return;
        }
        for record in &mut self.records {
            if record.root != root || record.baselines.contains_key(&document.path) {
                continue;
            }
            let mut state = crate::editor_ui::document_history(ctx, root, &document.path);
            let mut history = state.undoer();
            let cursor = state.cursor.char_range().unwrap_or_default();
            history.add_marker(&(cursor, document.text.clone()), record.marker);
            state.set_undoer(history);
            record
                .baselines
                .insert(document.path.clone(), document.text.clone());
        }
    }
}

impl NotesApp {
    pub(super) fn trash_for_undo(&mut self, sources: &[PathBuf]) -> (Vec<PathBuf>, Vec<String>) {
        let Some(vault) = &mut self.vault else {
            return (Vec::new(), vec!["Open a vault first.".into()]);
        };
        let paths = match vault.selected_entries(sources) {
            Ok(paths) => paths,
            Err(error) => return (Vec::new(), vec![error]),
        };
        let mut entries = Vec::new();
        let mut failures = Vec::new();
        for original in paths {
            let saved = (|| {
                let copy = tempfile::Builder::new()
                    .prefix("rustidian-deletion-")
                    .tempdir()
                    .map_err(|error| error.to_string())?;
                copy_entry(&vault.root.join(&original), &copy.path().join("entry")).map_err(
                    |error| format!("Cannot keep an undo copy; nothing deleted: {error}"),
                )?;
                vault.trash_entry(&original)?;
                Ok::<_, String>(Snapshot {
                    original: original.clone(),
                    copy,
                    restored: None,
                })
            })();
            match saved {
                Ok(snapshot) => entries.push(snapshot),
                Err(error) => failures.push(format!("{}: {error}", original.display())),
            }
        }
        let removed = entries.iter().map(|entry| entry.original.clone()).collect();
        if !entries.is_empty() {
            self.deletions.sequence += 1;
            self.deletions.notice = None;
            self.deletions.records.push(Deletion {
                root: vault.root.clone(),
                entries,
                tabs: Vec::new(),
                active: self.tabs.get(self.active).map(Tab::id),
                // Table layout owns the low, increasing marker numbers.
                marker: u64::MAX - self.deletions.sequence,
                baselines: HashMap::new(),
                at: Instant::now(),
            });
            if self.deletions.records.len() > 100 {
                self.deletions.records.remove(0);
            }
            for tab in &self.tabs {
                self.deletions
                    .mark_document(&self.ctx, &vault.root, &tab.document);
            }
        }
        (removed, failures)
    }

    pub(super) fn keep_deleted_tabs(&mut self, removed: &[PathBuf]) {
        let Some(record) = self.deletions.records.last_mut() else {
            return;
        };
        for (index, tab) in std::mem::take(&mut self.tabs).into_iter().enumerate() {
            if removed
                .iter()
                .any(|path| tab.document.path.starts_with(path))
            {
                record.tabs.push((index, tab));
            } else {
                self.tabs.push(tab);
            }
        }
    }

    pub(super) fn deletion_shortcut(&mut self, ctx: &egui::Context) {
        for tab in &self.tabs {
            if let Some(at) = tab.edited_at {
                self.deletions.edits.insert(tab.id(), at);
            }
        }
        // A watcher rescan must not immediately erase a restore/conflict notice.
        if let Some((notice, at)) = &self.deletions.notice
            && at.elapsed() < Duration::from_secs(6)
            && self.status.ends_with(" notes")
        {
            self.status.clone_from(notice);
        }
        let Some(record) = self.deletions.records.last() else {
            return;
        };
        let Some(vault) = &self.vault else { return };
        if record.root != vault.root {
            return;
        }
        let tab = self.tabs.get(self.active);
        let editor_focused = tab.is_some_and(|tab| {
            self.deletions
                .editor
                .is_some_and(|(owner, id)| owner == tab.id() && self.editor.has_focus(ctx, id))
                || crate::table_layout::has_focus(ctx, &tab.document.path)
        });
        if ctx.egui_wants_keyboard_input() && !editor_focused {
            return; // Search, find, titles and other text fields keep native undo.
        }
        // Earlier text in this frame must reach the editor before this shortcut.
        let undo_first = ctx.input(|input| {
            for event in &input.events {
                match event {
                    egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Cut
                    | egui::Event::Ime(egui::ImeEvent::Commit(_)) => return false,
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        if *key == Key::Z && modifiers.matches_logically(Modifiers::COMMAND) {
                            return true;
                        }
                        if matches!(key, Key::Backspace | Key::Delete | Key::Enter | Key::Tab) {
                            return false;
                        }
                    }
                    _ => {}
                }
            }
            false
        });
        if !undo_first {
            return;
        }
        // Clicking elsewhere is navigation, not an edit. A newer open draft
        // still precedes the deletion, even after its autosave has completed.
        let newer = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| {
                let at = self.deletions.edits.get(&tab.id())?;
                if *at <= record.at || tab.document.is_blank() || tab.document.is_image() {
                    return None;
                }
                let state =
                    crate::editor_ui::document_history(ctx, &vault.root, &tab.document.path);
                let current = (
                    state.cursor.char_range().unwrap_or_default(),
                    tab.document.text.clone(),
                );
                let history = state.undoer();
                (history.has_undo(&current)
                    && history.step_marker(&current, false) != Some(record.marker))
                .then_some((index, *at))
            })
            .max_by_key(|(_, at)| *at);
        if let Some((index, _)) = newer {
            if self.active != index {
                self.active = index;
                self.focus_editor();
            }
            return;
        }
        if let Some(tab) = tab.filter(|tab| !tab.document.is_blank() && !tab.document.is_image()) {
            let state = crate::editor_ui::document_history(ctx, &vault.root, &tab.document.path);
            let current = (
                state.cursor.char_range().unwrap_or_default(),
                tab.document.text.clone(),
            );
            if state.undoer().step_marker(&current, false) != Some(record.marker) {
                return; // Newer text or table edits still own Ctrl+Z.
            }
        }
        if ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::Z)) {
            self.restore_deletion();
        }
    }

    fn restore_deletion(&mut self) {
        let Some(mut record) = self.deletions.records.pop() else {
            return;
        };
        let mut notices = Vec::new();
        let mut failure = None;
        for entry in &mut record.entries {
            if entry.restored.is_some() {
                continue;
            }
            match restore_entry(&record.root, entry) {
                Ok(path) => {
                    notices.push(if path == entry.original {
                        format!("Restored {}", path.display())
                    } else {
                        format!("Restored {} · original path taken", path.display())
                    });
                    entry.restored = Some(path);
                }
                Err(error) => {
                    failure = Some(format!(
                        "Cannot restore {}: {error} · Ctrl+Z to retry",
                        entry.original.display()
                    ));
                    break;
                }
            }
        }
        let active_before = self.tabs.get(self.active).map(Tab::id);
        let mut waiting = Vec::new();
        for (index, mut tab) in std::mem::take(&mut record.tabs) {
            let restored = record.entries.iter().find_map(|entry| {
                let destination = entry.restored.as_ref()?;
                let suffix = tab.document.path.strip_prefix(&entry.original).ok()?;
                Some(if suffix.as_os_str().is_empty() {
                    destination.clone()
                } else {
                    destination.join(suffix)
                })
            });
            if let Some(path) = restored {
                if path != tab.document.path {
                    // A replacement at the old name owns its own text history.
                    crate::editor_ui::clear_document_history(&self.ctx, &record.root, &path);
                    let mut document = tab.document.clone();
                    document.path = path;
                    tab.replace(document);
                }
                tab.restore_scroll = Some(tab.scroll_y);
                if self.tabs.len() == 1
                    && self.tabs[0].document.is_blank()
                    && !self.tabs[0].document.dirty()
                {
                    self.tabs.clear();
                }
                self.tabs.insert(index.min(self.tabs.len()), tab);
            } else {
                waiting.push((index, tab));
            }
        }
        record.tabs = waiting;
        self.active = self
            .tabs
            .iter()
            .position(|tab| Some(tab.id()) == record.active)
            .or_else(|| {
                self.tabs
                    .iter()
                    .position(|tab| Some(tab.id()) == active_before)
            })
            .unwrap_or(0);
        if let Some(error) = failure {
            notices.push(error);
            self.deletions.records.push(record);
        } else {
            // Consume only this deletion's marker, retaining ordinary text/redo.
            for (path, baseline) in &record.baselines {
                let mut state = crate::editor_ui::document_history(&self.ctx, &record.root, path);
                let text = self
                    .tabs
                    .iter()
                    .find(|tab| tab.document.path == *path)
                    .map_or(baseline, |tab| &tab.document.text);
                let current = (state.cursor.char_range().unwrap_or_default(), text.clone());
                let mut history = state.undoer();
                if history.step_marker(&current, false) == Some(record.marker) {
                    history.undo(&current);
                    state.set_undoer(history);
                }
            }
        }
        self.queue_index(
            self.vault
                .as_ref()
                .expect("vault checked before undo")
                .root
                .clone(),
        );
        self.refresh_file_views();
        self.status = notices.join(" · ");
        self.deletions.notice = Some((self.status.clone(), Instant::now()));
        self.find = Default::default();
        self.focus_editor();
        if let Err(error) = self.write_session() {
            self.error = Some(error);
        }
    }
}

/// No decoding, filtering or traversal through links: hidden and binary data survive.
fn copy_entry(source: &Path, destination: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(fs::read_link(source)?, destination)?;
        }
        #[cfg(windows)]
        {
            let link = fs::read_link(source)?;
            if source.is_dir() {
                std::os::windows::fs::symlink_dir(link, destination)?;
            } else {
                std::os::windows::fs::symlink_file(link, destination)?;
            }
        }
    } else if metadata.is_dir() {
        fs::create_dir(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_entry(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        let mut input = fs::File::open(source)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        io::copy(&mut input, &mut output)?;
        output.set_permissions(metadata.permissions())?;
        output.sync_all()?;
    } else {
        return Err(io::Error::other("A special file cannot be copied safely"));
    }
    Ok(())
}

fn restore_entry(root: &Path, entry: &Snapshot) -> io::Result<PathBuf> {
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(io::Error::other("The vault folder is now a file or link"));
    }
    let parent = entry.original.parent().unwrap_or_else(|| Path::new(""));
    let mut directory = root.to_owned();
    for part in parent.components() {
        let Component::Normal(part) = part else {
            return Err(io::Error::other(
                "The original parent is outside this vault",
            ));
        };
        directory.push(part);
        match fs::create_dir(&directory) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
        let metadata = fs::symlink_metadata(&directory)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::Error::other(
                "The original parent is now a file or link",
            ));
        }
    }
    // Prepare a complete copy on the destination filesystem before publishing it.
    let stage = tempfile::Builder::new()
        .prefix(".rustidian-restore-")
        .tempdir_in(&directory)?;
    let staged = stage.path().join("entry");
    copy_entry(&entry.copy.path().join("entry"), &staged)?;
    let folder = fs::symlink_metadata(&staged)?.is_dir();
    for number in 0.. {
        let path = if number == 0 {
            entry.original.clone()
        } else {
            let name = entry
                .original
                .file_name()
                .ok_or_else(|| io::Error::other("Missing file name"))?;
            let stem = if folder {
                name
            } else {
                entry.original.file_stem().unwrap_or(name)
            };
            let mut name = stem.to_os_string();
            name.push(format!(" (restored {number})"));
            if !folder && let Some(extension) = entry.original.extension() {
                name.push(".");
                name.push(extension);
            }
            parent.join(name)
        };
        match publish(&staged, &root.join(&path)) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

#[cfg(target_os = "linux")]
fn publish(source: &Path, destination: &Path) -> io::Result<()> {
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        source,
        rustix::fs::CWD,
        destination,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(io::Error::from)
}

#[cfg(not(target_os = "linux"))]
fn publish(source: &Path, destination: &Path) -> io::Result<()> {
    // Exclusive creation also protects platforms without rename-noreplace.
    copy_entry(source, destination)
}
