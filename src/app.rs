#[path = "settings_ui.rs"]
pub(crate) mod settings_ui;

use crate::{
    file_tree::{FileAction, Tree},
    preview,
    theme::{self, Icon, icon_button},
};
use eframe::egui::{
    self, Align, Color32, FontId, Key, Layout, Margin, RichText, ScrollArea, Stroke, TextEdit,
};
use egui_commonmark::CommonMarkCache;
use rust_notes::commands::Scope;
use rust_notes::tab::{ClosedTab, Tab, View};
use rust_notes::vault::{Document, Vault, wiki_links};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq)]
enum Dialog {
    QuickOpen,
    NewNote,
    NewFolder,
    SaveCopy,
    OpenVault,
    Commands,
    RenameNote,
    TrashNote,
    AddProperty,
    BookmarkTitle,
    InsertTemplate,
    Settings,
}
#[derive(Clone)]
enum Pending {
    CloseTab(usize),
    CloseOtherTabs(u64),
    Quit,
    ChangeVault,
    OpenVault(PathBuf, Option<PathBuf>),
    LoadWorkspace(String),
    RestoreSnapshot(Box<crate::recovery_ui::Restore>),
}

pub struct NotesApp {
    pub(crate) vault: Option<Vault>,
    tree: Tree,
    pub(crate) tabs: Vec<Tab>,
    closed_tabs: Vec<ClosedTab>,
    pub(crate) active: usize,
    search: String,
    search_cache: rust_notes::query::Cache,
    searching: bool,
    search_focus: bool,
    pub(crate) show_left: bool,
    pub(crate) show_right: bool,
    pub(crate) dark: bool,
    cache: CommonMarkCache,
    dialog: Option<Dialog>,
    dialog_input: String,
    dialog_path: Option<PathBuf>,
    dialog_focus: bool,
    dialog_select_all: bool,
    dialog_selected: usize,
    settings_page: settings_ui::Page,
    files: rust_notes::search::FileMatcher,
    pending: Option<Pending>,
    error: Option<String>,
    status: String,
    allow_close: bool,
    started: Instant,
    first_frame: bool,
    window_title: String,
    ctx: egui::Context,
    loading: Option<Receiver<Result<Vault, String>>>,
    watcher: Option<notify::RecommendedWatcher>,
    watch_events: Option<Receiver<notify::Result<notify::Event>>>,
    refresh_due: Option<Instant>,
    loading_root: Option<PathBuf>,
    startup_document: Option<PathBuf>,
    find: crate::editor_ui::Find,
    editor: crate::editor_ui::Editor,
    title: crate::title_ui::Title,
    pending_anchor: Option<String>,
    new_tab_gesture: bool,
    properties: crate::properties_ui::Properties,
    pub(crate) sort_descending: bool,
    collapse_epoch: u64,
    reveal_folder: Option<PathBuf>,
    reset_tree_scroll: bool,
    pub(crate) bookmarks: rust_notes::bookmarks::Bookmarks,
    pub(crate) show_bookmarks: bool,
    pub(crate) right_tab: u8,
    pub(crate) ui_zoom: f32,
    pub(crate) document_font_size: u8,
    pub(crate) editor_preferences: settings_ui::EditorPreferences,
    pub(crate) autosave_enabled: bool,
    vault_manager: crate::vault_manager::Manager,
    open_picker: rust_notes::native_picker::Picker,
    pub(crate) note_aids: rust_notes::note_aids::Config,
    template_source: Option<(u64, String)>,
    template_choices: Result<rust_notes::search::FileMatcher, String>,
    workspace_manager: crate::workspace_ui::Manager,
    pub(crate) sidebar_widths: [f32; 2],
    pub(crate) restore_sidebars: [bool; 2],
    pub(crate) recovery_config: rust_notes::recovery::Config,
    recovery_history: Option<rust_notes::recovery::History>,
    recovery_recorder: Option<rust_notes::recovery_capture::Recorder>,
    recovery_revisions: std::collections::HashMap<PathBuf, u64>,
    recovery_changed_paths: std::collections::HashSet<PathBuf>,
    recovery_index_paths: std::collections::HashSet<PathBuf>,
    recovery_saved: std::collections::HashMap<PathBuf, String>,
    recovery_manager: crate::recovery_ui::Manager,
    file_move: crate::file_move_ui::Manager,
    page_preview: crate::page_preview::Manager,
    graph: crate::graph_ui::Manager,
    pub(crate) always_update_links: bool,
}

impl NotesApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        root: &Path,
        started: Instant,
        startup_document: Option<PathBuf>,
    ) -> Self {
        theme::apply(&cc.egui_ctx, true);
        theme::install_fonts(&cc.egui_ctx);
        theme::install_icons(&cc.egui_ctx);
        let mut app = Self {
            vault: None,
            tree: Tree::default(),
            tabs: Vec::new(),
            closed_tabs: Vec::new(),
            active: 0,
            search: String::new(),
            search_cache: Default::default(),
            searching: false,
            search_focus: false,
            show_left: true,
            show_right: false,
            dark: true,
            cache: CommonMarkCache::default(),
            dialog: None,
            dialog_input: String::new(),
            dialog_path: None,
            dialog_focus: false,
            dialog_select_all: false,
            dialog_selected: 0,
            settings_page: Default::default(),
            files: rust_notes::search::FileMatcher::new(Vec::new()),
            pending: None,
            error: None,
            status: "Ready".into(),
            allow_close: false,
            started,
            first_frame: true,
            window_title: String::new(),
            ctx: cc.egui_ctx.clone(),
            loading: None,
            watcher: None,
            watch_events: None,
            refresh_due: None,
            loading_root: None,
            startup_document,
            find: Default::default(),
            editor: Default::default(),
            title: Default::default(),
            pending_anchor: None,
            new_tab_gesture: false,
            properties: Default::default(),
            sort_descending: false,
            collapse_epoch: 0,
            reveal_folder: None,
            reset_tree_scroll: false,
            bookmarks: Default::default(),
            show_bookmarks: false,
            right_tab: 0,
            ui_zoom: 1.0,
            document_font_size: theme::DEFAULT_DOCUMENT_FONT_SIZE,
            editor_preferences: Default::default(),
            autosave_enabled: true,
            vault_manager: Default::default(),
            open_picker: Default::default(),
            note_aids: Default::default(),
            template_source: None,
            template_choices: Err(String::new()),
            workspace_manager: Default::default(),
            sidebar_widths: [300.0, 234.0],
            restore_sidebars: [false; 2],
            recovery_config: Default::default(),
            recovery_history: None,
            recovery_recorder: None,
            recovery_revisions: Default::default(),
            recovery_changed_paths: Default::default(),
            recovery_index_paths: Default::default(),
            recovery_saved: Default::default(),
            recovery_manager: Default::default(),
            file_move: Default::default(),
            page_preview: Default::default(),
            graph: Default::default(),
            always_update_links: false,
        };
        app.load_vault(root);
        app
    }

    fn view(&self) -> View {
        self.tabs
            .get(self.active)
            .map_or(View::Live, |tab| tab.view)
    }

    fn set_view(&mut self, view: View) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.set_view(view);
        }
        self.focus_editor();
    }

    fn focus_editor(&mut self) {
        self.editor.focus = self.tabs.get(self.active).is_some_and(|tab| {
            !matches!(tab.view, View::Read | View::Graph)
                && !tab.document.is_blank()
                && !tab.document.is_image()
        });
        self.ctx.request_repaint();
    }

    fn toggle_reading(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.toggle_reading();
        }
        self.focus_editor();
    }

    fn load_vault(&mut self, root: &Path) {
        self.persist_session();
        self.loading_root = Some(root.to_owned());
        self.queue_index(root.to_owned());
        self.status = "Opening vault…".into();
    }

    fn queue_index(&mut self, root: PathBuf) {
        self.recovery_index_paths
            .extend(self.recovery_changed_paths.drain());
        let (sender, receiver) = mpsc::channel();
        self.loading = Some(receiver);
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(Vault::open(&root));
            ctx.request_repaint();
        });
    }

    fn background(&mut self, ctx: &egui::Context) {
        use notify::Watcher;
        if let Some(events) = &self.watch_events {
            while let Ok(event) = events.try_recv() {
                match event {
                    Ok(event)
                        if (event.kind.is_modify()
                            || event.kind.is_create()
                            || event.kind.is_remove())
                            && self.vault.as_ref().is_some_and(|vault| {
                                event.paths.iter().any(|path| {
                                    path.strip_prefix(&vault.root).is_ok_and(|relative| {
                                        !rust_notes::vault::is_ignored_path(relative)
                                    })
                                })
                            })
                            && (matches!(
                                event.kind,
                                notify::EventKind::Create(notify::event::CreateKind::Folder)
                                    | notify::EventKind::Remove(notify::event::RemoveKind::Folder)
                                    | notify::EventKind::Modify(notify::event::ModifyKind::Name(_))
                            ) || event.paths.iter().any(|p| {
                                p.is_dir()
                                    || rust_notes::assets::is_image(p)
                                    || rust_notes::vault::is_text_document(p)
                                    || p.extension().is_none()
                            })) =>
                    {
                        if let Some(vault) = &self.vault {
                            for path in &event.paths {
                                if let Ok(relative) = path.strip_prefix(&vault.root)
                                    && rust_notes::vault::is_markdown(relative)
                                    && !rust_notes::vault::is_ignored_path(relative)
                                {
                                    self.recovery_changed_paths.insert(relative.to_owned());
                                }
                            }
                        }
                        for path in event
                            .paths
                            .iter()
                            .filter(|path| rust_notes::assets::is_image(path))
                        {
                            ctx.forget_image(&format!("file://{}", path.display()));
                        }
                        self.refresh_due = Some(Instant::now() + Duration::from_millis(150));
                    }
                    Err(error) => self.error = Some(format!("File watcher: {error}")),
                    _ => {}
                }
            }
        }
        if self.loading.is_none() && self.refresh_due.is_some_and(|due| Instant::now() >= due) {
            self.refresh_due = None;
            if let Some(vault) = &self.vault {
                self.queue_index(vault.root.clone());
            }
        }
        let result = self
            .loading
            .as_ref()
            .and_then(|loading| loading.try_recv().ok());
        if let Some(result) = result {
            self.loading = None;
            let result = result.and_then(|vault| {
                // Reject unreadable/invalid documents before replacing the old workspace.
                if self.loading_root.is_some()
                    && let Some(path) = &self.startup_document
                {
                    vault.open_document(path)?;
                }
                Ok(vault)
            });
            match result {
                Ok(vault) => {
                    let opening = self.loading_root.take().is_some();
                    if opening {
                        self.graph.reset();
                    } else {
                        self.graph.invalidate();
                    }
                    if opening {
                        self.file_move = Default::default();
                        self.page_preview.close();
                        self.closed_tabs.clear();
                        self.always_update_links = rust_notes::paths::read_obsidian::<
                            serde_json::Value,
                        >(
                            &vault.root, "app.json"
                        )
                        .unwrap_or_else(|error| {
                            self.error = Some(error);
                            Default::default()
                        })
                        .get("alwaysUpdateLinks")
                        .and_then(|value| value.as_bool())
                        .unwrap_or(false);
                        self.workspace_manager.open = false;
                        self.recovery_manager = Default::default();
                        self.recovery_recorder = None;
                        self.recovery_history = None;
                        self.recovery_revisions.clear();
                        self.recovery_changed_paths.clear();
                        self.recovery_index_paths.clear();
                        self.recovery_saved.clear();
                        self.recovery_config =
                            rust_notes::recovery::Config::from_obsidian(&vault.root)
                                .unwrap_or_else(|error| {
                                    self.error = Some(error);
                                    Default::default()
                                });
                        self.sidebar_widths = [300.0, 234.0];
                        self.restore_sidebars = [true; 2];
                        self.bookmarks = Default::default();
                        self.right_tab = 0;
                        self.ui_zoom = 1.0;
                        self.document_font_size = rust_notes::paths::read_obsidian::<
                            serde_json::Value,
                        >(
                            &vault.root, "appearance.json"
                        )
                        .unwrap_or_else(|error| {
                            self.error = Some(error);
                            Default::default()
                        })
                        .get("baseFontSize")
                        .and_then(serde_json::Value::as_u64)
                        .map_or(theme::DEFAULT_DOCUMENT_FONT_SIZE, |size| {
                            size.clamp(10, 30) as u8
                        });
                        self.editor_preferences = Default::default();
                        self.autosave_enabled = true;
                        ctx.set_zoom_factor(1.0);
                        self.show_bookmarks = false;
                        self.sort_descending = false;
                        self.collapse_epoch = 0;
                        self.reveal_folder = None;
                        self.reset_tree_scroll = true;
                        self.note_aids = rust_notes::note_aids::Config::from_obsidian(&vault.root)
                            .unwrap_or_else(|error| {
                                self.error = Some(error);
                                Default::default()
                            });
                        let first = vault
                            .notes
                            .iter()
                            .find(|n| n.path == Path::new("Home.md"))
                            .or_else(|| vault.notes.first())
                            .map(|n| n.path.clone());
                        self.tabs.clear();
                        self.active = 0;
                        self.search.clear();
                        let (sender, receiver) = mpsc::channel();
                        let watcher_ctx = ctx.clone();
                        match notify::RecommendedWatcher::new(
                            move |event| {
                                let _ = sender.send(event);
                                watcher_ctx.request_repaint();
                            },
                            notify::Config::default().with_follow_symlinks(false),
                        ) {
                            Ok(mut watcher) => match watcher
                                .watch(&vault.root, notify::RecursiveMode::Recursive)
                            {
                                Ok(()) => {
                                    self.watcher = Some(watcher);
                                    self.watch_events = Some(receiver);
                                }
                                Err(error) => {
                                    self.error = Some(format!("Cannot watch this vault: {error}"))
                                }
                            },
                            Err(error) => {
                                self.error = Some(format!("Cannot start file watcher: {error}"))
                            }
                        }
                        self.tree = Tree::from_vault(&vault);
                        self.files = rust_notes::search::FileMatcher::from_vault(&vault);
                        self.search_cache.invalidate();
                        eprintln!(
                            "vault={} notes={} index_us={} warnings={}",
                            vault.root.display(),
                            vault.notes.len(),
                            vault.index_time.as_micros(),
                            vault.warnings.len()
                        );
                        if let Err(error) = self.vault_manager.remember(&vault.root) {
                            self.error = Some(error);
                        }
                        self.vault = Some(vault);
                        self.vault_manager.close();
                        if !self.restore_session(ctx)
                            && let Some(path) = first
                        {
                            self.open_note(&path);
                        }
                        if let Some(path) = self.startup_document.take() {
                            self.open_note_at(&path, true);
                        }
                    } else {
                        // Indexing already read these saved bytes off the UI thread.
                        // Capture changed files even when they have no editor tab.
                        for note in &vault.notes {
                            if self.recovery_index_paths.remove(&note.path) {
                                self.recovery_saved
                                    .insert(note.path.clone(), note.text.clone());
                            }
                        }
                        self.recovery_index_paths.clear();
                        for tab in &mut self.tabs {
                            if !tab.document.dirty() && !tab.document.is_blank() {
                                match vault.open_document(&tab.document.path) {
                                    Ok(document) if document.text != tab.document.text => {
                                        tab.replace(document)
                                    }
                                    Err(error) => self.error = Some(error),
                                    _ => {}
                                }
                            }
                        }
                        self.tree = Tree::from_vault(&vault);
                        self.files = rust_notes::search::FileMatcher::from_vault(&vault);
                        self.search_cache.invalidate();
                        self.vault = Some(vault);
                    }
                    if let Some(vault) = &self.vault {
                        self.status = format!("{} notes", vault.notes.len());
                        if !vault.warnings.is_empty() {
                            self.error = Some(format!(
                                "Some files could not be indexed: {}",
                                vault.warnings.join("; ")
                            ));
                        }
                    }
                    self.cache = CommonMarkCache::default();
                }
                Err(error) => {
                    self.recovery_changed_paths
                        .extend(self.recovery_index_paths.drain());
                    self.loading_root = None;
                    self.startup_document = None;
                    if self.vault_manager.open {
                        self.vault_manager.show_error(error.clone());
                    }
                    self.error = Some(error);
                }
            }
        }
        let mut saves = Vec::new();
        for (index, tab) in self.tabs.iter().enumerate() {
            if self.autosave_enabled
                && self.loading_root.is_none()
                && !self.open_picker.active()
                && tab.document.dirty()
                && !tab.autosave_failed
                && let Some(edited) = tab.edited_at
            {
                let remaining = Duration::from_millis(600).saturating_sub(edited.elapsed());
                if remaining.is_zero() && self.pending.is_none() {
                    saves.push(index);
                } else {
                    ctx.request_repaint_after(remaining.max(Duration::from_millis(20)));
                }
            }
        }
        for index in saves {
            self.save(index);
        }
        if let Some(due) = self.refresh_due {
            ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
        }
        self.capture_recovery(ctx);
    }

    fn capture_recovery(&mut self, ctx: &egui::Context) {
        let Some(vault) = &self.vault else { return };
        if self.recovery_recorder.is_none() {
            match rust_notes::recovery::History::open(&vault.root) {
                Ok(history) => {
                    let wake = ctx.clone();
                    self.recovery_recorder = Some(rust_notes::recovery_capture::Recorder::start(
                        history.clone(),
                        self.recovery_config.clone(),
                        move || wake.request_repaint(),
                    ));
                    self.recovery_history = Some(history);
                }
                Err(error) => {
                    self.error = Some(error);
                    return;
                }
            }
        }
        let Some(recorder) = &mut self.recovery_recorder else {
            return;
        };
        if !recorder.configure(&self.recovery_config) {
            ctx.request_repaint_after(Duration::from_millis(20));
        }
        while let Ok(error) = recorder.errors.try_recv() {
            self.error = Some(error);
        }
        if recorder.events.try_iter().count() > 0 {
            self.recovery_manager.changed(ctx);
        }
        self.recovery_manager.poll(ctx);
        if !self.recovery_manager.open {
            if let Some(error) = self.recovery_manager.take_error() {
                self.error = Some(error);
            }
            if let Some(notice) = self.recovery_manager.take_notice() {
                self.status = notice;
            }
        }
        for tab in &self.tabs {
            let document = &tab.document;
            if !rust_notes::vault::is_markdown(&document.path)
                || self.recovery_revisions.get(&document.path) == Some(&document.saved_revision())
            {
                continue;
            }
            if let Some(text) = document.saved_text() {
                if recorder.capture(&document.path, text, &self.recovery_config) {
                    self.recovery_saved.remove(&document.path);
                    self.recovery_revisions
                        .insert(document.path.clone(), document.saved_revision());
                } else {
                    ctx.request_repaint_after(Duration::from_millis(20));
                }
            }
        }
        let pending: Vec<_> = self.recovery_saved.keys().take(16).cloned().collect();
        for path in pending {
            let text = &self.recovery_saved[&path];
            if recorder.capture(&path, text, &self.recovery_config) {
                self.recovery_saved.remove(&path);
            } else {
                break;
            }
        }
        if !self.recovery_saved.is_empty() {
            ctx.request_repaint_after(Duration::from_millis(20));
        }
    }

    fn open_recovery(&mut self, current_file: bool) {
        if let (Some(history), Some(vault)) = (&self.recovery_history, &self.vault) {
            let path = current_file
                .then(|| {
                    self.tabs
                        .get(self.active)
                        .filter(|tab| {
                            tab.view != View::Graph
                                && rust_notes::vault::is_markdown(&tab.document.path)
                        })
                        .map(|tab| tab.document.path.clone())
                })
                .flatten();
            self.dialog = None;
            self.editor.focus = false;
            self.recovery_manager
                .open(history.clone(), vault, path, &self.ctx);
        }
    }

    fn restore_snapshot(&mut self, request: crate::recovery_ui::Restore) {
        let result = match (&self.recovery_history, &mut self.vault) {
            (Some(history), Some(vault)) => history.restore(
                vault,
                &request.expected,
                request.version,
                chrono::Local::now().timestamp_millis(),
                &self.recovery_config,
            ),
            _ => Err("Open a vault first.".into()),
        };
        match result {
            Ok(document) => {
                let mut found = false;
                for tab in &mut self.tabs {
                    if tab.document.path == document.path {
                        tab.replace(document.clone());
                        found = true;
                    }
                }
                if !found {
                    self.tabs.push(Tab::new(document.clone()));
                    self.active = self.tabs.len() - 1;
                }
                if let Some(vault) = &self.vault {
                    self.tree = Tree::from_vault(vault);
                    self.files = rust_notes::search::FileMatcher::from_vault(vault);
                }
                self.cache = Default::default();
                self.search_cache.invalidate();
                self.recovery_manager.restored(document, &self.ctx);
                self.status = "Snapshot restored".into();
                self.error = None;
                self.ctx.request_repaint();
            }
            Err(error) => {
                self.recovery_manager.error(error.clone());
                self.error = Some(error);
            }
        }
    }

    fn open_note(&mut self, path: &Path) {
        self.open_note_at(path, false);
    }

    fn open_note_at(&mut self, path: &Path, new_tab: bool) {
        let new_tab = new_tab
            || self
                .tabs
                .get(self.active)
                .is_some_and(|tab| tab.pinned && tab.document.path != path);
        if !new_tab
            && self
                .tabs
                .get(self.active)
                .is_some_and(|t| t.document.path == path)
        {
            self.tabs[self.active].show_document();
            self.focus_editor();
            return;
        }
        if !new_tab
            && self
                .tabs
                .get(self.active)
                .is_some_and(|t| t.document.dirty())
            && !self.save(self.active)
        {
            return;
        }
        if new_tab && let Some(index) = self.tabs.iter().position(|t| t.document.path == path) {
            self.active = index;
            self.tabs[index].show_document();
            self.focus_editor();
            return;
        }
        let Some(vault) = &self.vault else {
            return;
        };
        let document = self
            .tabs
            .iter()
            .find(|tab| tab.document.path == path)
            .map(|tab| Ok(tab.document.clone()))
            .unwrap_or_else(|| vault.open_document(path));
        match document {
            Ok(document) => {
                if new_tab || self.tabs.is_empty() {
                    let mut tab = Tab::new(document);
                    self.editor_preferences.apply(&mut tab);
                    self.tabs.push(tab);
                    self.active = self.tabs.len() - 1;
                } else if let Err(error) = self.tabs[self.active].open(document) {
                    self.error = Some(error.into());
                }
                self.cache = CommonMarkCache::default();
                self.find = Default::default();
                self.focus_editor();
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn navigate(&mut self, back: bool) {
        if back && self.view() == View::Graph && !self.tabs[self.active].document.is_blank() {
            self.tabs[self.active].show_document();
            self.focus_editor();
            return;
        }
        if self
            .tabs
            .get(self.active)
            .is_some_and(|t| t.document.dirty())
            && !self.save(self.active)
        {
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let mut history = tab.history.clone();
        let path = if back {
            history.back()
        } else {
            history.forward()
        };
        if let Some(path) = path
            && let Some(vault) = &self.vault
        {
            match vault.open_document(&path) {
                Ok(document) => {
                    tab.replace(document);
                    tab.show_document();
                    tab.history = history;
                    self.cache = CommonMarkCache::default();
                    self.focus_editor();
                }
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn follow_link(&mut self, target: &str) {
        let Some(from) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.document.path.clone())
        else {
            return;
        };
        self.follow_link_from(&from, target);
    }

    fn follow_link_from(&mut self, from: &Path, target: &str) {
        let uri = target.split_once(':').is_some_and(|(scheme, _)| {
            let mut characters = scheme.chars();
            characters
                .next()
                .is_some_and(|character| character.is_ascii_alphabetic())
                && characters.all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
                })
        });
        if uri {
            self.ctx.open_url(egui::OpenUrl::new_tab(target));
            return;
        }
        let new_tab = self.new_tab_gesture
            || self.tabs.get(self.active).is_some_and(|tab| tab.pinned)
            || self.ctx.input(|input| {
                input.modifiers.command || input.pointer.button_clicked(egui::PointerButton::Middle)
            });
        if !new_tab
            && self
                .tabs
                .get(self.active)
                .is_some_and(|tab| tab.document.dirty())
            && !self.save(self.active)
        {
            return;
        }
        let result = self
            .vault
            .as_mut()
            .map(|vault| vault.open_link(from, target));
        match result {
            Some(Ok(document)) => {
                if let Some(vault) = &self.vault {
                    self.tree = Tree::from_vault(vault);
                    self.files = rust_notes::search::FileMatcher::from_vault(vault);
                    self.search_cache.invalidate();
                }
                self.open_note_at(&document.path, new_tab);
                if let Some((_, fragment)) = target.split_once('#') {
                    let fragment = fragment.split('|').next().unwrap_or(fragment);
                    let decoded = rust_notes::paths::decode(fragment);
                    self.jump_to_anchor(decoded.as_deref().unwrap_or(fragment));
                }
            }
            Some(Err(error)) => self.error = Some(error),
            None => {}
        }
    }

    fn jump_to_anchor(&mut self, fragment: &str) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if let Some(character) = rust_notes::structure::find_anchor(&tab.document.text, fragment) {
            if self.view() == View::Read {
                self.pending_anchor = Some(fragment.into());
            } else {
                self.editor.jump = Some(character);
            }
        } else {
            self.error = Some(format!("Cannot find heading or block: {fragment}"));
        }
    }

    fn save(&mut self, index: usize) -> bool {
        if self
            .tabs
            .get(index)
            .is_some_and(|tab| tab.view == View::Graph && !tab.document.dirty())
        {
            return true;
        }
        if self
            .tabs
            .get(index)
            .is_some_and(|tab| tab.document.is_image())
        {
            return true;
        }
        if self
            .tabs
            .get(index)
            .is_some_and(|tab| tab.document.is_blank())
        {
            self.ask(Dialog::NewNote);
            return false;
        }
        if let (Some(vault), Some(tab)) = (&mut self.vault, self.tabs.get_mut(index)) {
            match vault.save(&mut tab.document) {
                Ok(()) => {
                    self.status = format!("Saved {}", tab.document.path.display());
                    tab.edited_at = None;
                    tab.autosave_failed = false;
                    self.error = None;
                    self.tree = Tree::from_vault(vault);
                    self.files = rust_notes::search::FileMatcher::from_vault(vault);
                    self.search_cache.invalidate();
                    self.graph.invalidate();
                    return true;
                }
                Err(error) => {
                    tab.autosave_failed = true;
                    self.error = Some(error);
                }
            }
        }
        false
    }

    fn dirty(&self) -> bool {
        self.tabs.iter().any(|tab| tab.document.dirty())
    }

    fn new_tab(&mut self) {
        let mut tab = Tab::new(Document::blank());
        self.editor_preferences.apply(&mut tab);
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.editor.focus = false;
    }

    fn step_tab(&mut self, back: bool) {
        if !self.tabs.is_empty() {
            let delta = if back { self.tabs.len() - 1 } else { 1 };
            self.active = (self.active + delta) % self.tabs.len();
            self.focus_editor();
        }
    }

    fn toggle_pin(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.pinned = !tab.pinned;
        }
    }

    fn ask(&mut self, dialog: Dialog) {
        self.template_source = (dialog == Dialog::InsertTemplate)
            .then(|| {
                self.tabs
                    .get(self.active)
                    .map(|tab| (tab.id(), tab.document.text.clone()))
            })
            .flatten();
        if dialog == Dialog::InsertTemplate {
            self.template_choices = self
                .vault
                .as_ref()
                .map(|vault| {
                    self.note_aids
                        .template_paths(vault)
                        .map(rust_notes::search::FileMatcher::new)
                })
                .unwrap_or_else(|| Err("Open a vault first.".into()));
        }
        if matches!(dialog, Dialog::SaveCopy | Dialog::AddProperty) && self.view() == View::Graph {
            return;
        }
        if matches!(dialog, Dialog::SaveCopy | Dialog::AddProperty)
            && self
                .tabs
                .get(self.active)
                .is_some_and(|tab| tab.document.is_image())
        {
            self.error = Some("Choose a Markdown note for this action.".into());
            return;
        }
        self.editor.focus = false;
        self.ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
        self.dialog_path = None;
        self.dialog_input = match dialog {
            Dialog::RenameNote | Dialog::TrashNote => self
                .tabs
                .get(self.active)
                .map(|tab| tab.document.path.display().to_string())
                .unwrap_or_default(),
            Dialog::NewNote => "Untitled.md".into(),
            Dialog::NewFolder => self
                .tabs
                .get(self.active)
                .and_then(|tab| tab.document.path.parent())
                .unwrap_or(Path::new(""))
                .join("Untitled")
                .display()
                .to_string(),
            Dialog::SaveCopy => self
                .tabs
                .get(self.active)
                .map(|tab| format!("{} copy.md", tab.document.title()))
                .unwrap_or_default(),
            Dialog::OpenVault => self
                .vault
                .as_ref()
                .map(|vault| vault.root.display().to_string())
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.dialog_selected = 0;
        self.dialog = Some(dialog);
        self.dialog_focus = true;
        self.dialog_select_all = dialog == Dialog::NewNote;
    }

    fn ask_file(&mut self, dialog: Dialog, path: PathBuf) {
        self.ask(dialog);
        self.dialog_input = path.display().to_string();
        self.dialog_path = Some(path);
    }

    fn save_all(&mut self) -> bool {
        for index in 0..self.tabs.len() {
            if self.tabs[index].document.dirty() && !self.save(index) {
                return false;
            }
        }
        true
    }

    fn request_close_tab(&mut self, index: usize) {
        if self.tabs[index].pinned {
            self.toggle_pin(index);
            return;
        }
        if self.tabs[index].document.dirty() && !self.save(index) {
            self.pending = Some(Pending::CloseTab(index));
        } else {
            self.close_tab(index);
        }
    }

    fn close_tab(&mut self, index: usize) {
        if let Some(closed) = self.tabs.remove(index).into_closed() {
            self.closed_tabs.push(closed);
        }
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        self.focus_editor();
    }

    fn request_close_other_tabs(&mut self) {
        let Some(keep) = self.tabs.get(self.active).map(Tab::id) else {
            return;
        };
        let indices = rust_notes::tab::other_tab_indices(&self.tabs, keep);
        if indices
            .into_iter()
            .all(|index| !self.tabs[index].document.dirty() || self.save(index))
        {
            self.close_other_tabs(keep);
        } else {
            self.pending = Some(Pending::CloseOtherTabs(keep));
        }
    }

    fn close_other_tabs(&mut self, keep: u64) {
        // Reference closes in visual order, so Undo restores the rightmost first.
        let indices = rust_notes::tab::other_tab_indices(&self.tabs, keep);
        for (removed, index) in indices.into_iter().enumerate() {
            self.close_tab(index - removed);
        }
    }

    fn undo_close_tab(&mut self) {
        let Some(closed) = self.closed_tabs.pop() else {
            return;
        };
        self.page_preview.close();
        let existing = self
            .tabs
            .iter()
            .find(|tab| tab.document.path == closed.path());
        let from_existing = existing.is_some();
        let document = existing
            .map(|tab| Ok(tab.document.clone()))
            .unwrap_or_else(|| {
                if closed.path().as_os_str().is_empty() {
                    return Ok(Document::blank());
                }
                self.vault
                    .as_ref()
                    .ok_or_else(|| "Open a vault before reopening a tab.".to_owned())
                    .and_then(|vault| vault.open_document(closed.path()))
            });
        match document {
            Ok(document) => {
                if !from_existing
                    && !closed.matches_saved_text(&document)
                    && let Some(vault) = &self.vault
                {
                    crate::editor_ui::clear_document_history(&self.ctx, &vault.root, closed.path());
                }
                let tab = closed.restore(document);
                if self.tabs.len() == 1
                    && self.tabs[0].document.is_blank()
                    && self.tabs[0].view != View::Graph
                {
                    self.tabs.clear();
                }
                self.tabs.push(tab);
                self.active = self.tabs.len() - 1;
                self.find = Default::default();
                self.error = None;
                self.focus_editor();
            }
            Err(error) => {
                if self
                    .tabs
                    .get(self.active)
                    .is_none_or(|tab| !tab.document.is_blank())
                {
                    self.new_tab();
                }
                self.error = Some(error);
            }
        }
    }

    fn set_document_font_size(&mut self, size: u8) {
        let before = self.document_font_size;
        self.document_font_size = size.clamp(10, 30);
        if self.document_font_size != before {
            if let Err(error) = self.write_session() {
                self.document_font_size = before;
                self.error = Some(format!("Settings weren’t saved: {error}"));
            } else {
                self.settings_saved();
            }
            self.ctx.request_repaint();
        }
        self.focus_editor();
    }

    fn settings_saved(&mut self) {
        self.status = "Settings saved".into();
        if self
            .error
            .as_deref()
            .is_some_and(|error| error.starts_with("Settings weren’t saved:"))
        {
            self.error = None;
        }
    }

    fn run_pending(&mut self, pending: Pending, ctx: &egui::Context) {
        match pending {
            Pending::CloseTab(index) => self.close_tab(index),
            Pending::CloseOtherTabs(keep) => self.close_other_tabs(keep),
            Pending::Quit => {
                self.persist_session();
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Pending::ChangeVault => self.ask(Dialog::OpenVault),
            Pending::OpenVault(root, document) => {
                self.startup_document = document;
                self.load_vault(&root);
            }
            Pending::LoadWorkspace(name) => self.load_workspace(&name),
            Pending::RestoreSnapshot(request) => self.restore_snapshot(*request),
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().close_requested()) {
            self.open_picker.cancel();
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.dirty() {
            self.persist_session();
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.dirty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        }
        if self.pending.is_some() {
            return;
        }
        if self.loading_root.is_some()
            || self.open_picker.active()
            || self.vault_manager.open
            || self.workspace_manager.open
            || self.recovery_manager.active()
            || self.file_move.active()
        {
            return;
        }
        if self.dialog.is_none() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Comma)) {
                self.ask(Dialog::Settings);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Semicolon)) {
                self.ask(Dialog::AddProperty);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::F2))
                && self.view() != View::Graph
                && let Some(path) = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.path.clone())
            {
                self.ask_file(Dialog::RenameNote, path);
            }

            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, Key::ArrowLeft)) {
                self.navigate(true);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::ALT, Key::ArrowRight)) {
                self.navigate(false);
            }
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, Key::F)
            }) {
                self.show_left = true;
                self.searching = true;
                self.show_bookmarks = false;
                self.search_focus = true;
                self.editor.focus = false;
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::F))
                && self
                    .tabs
                    .get(self.active)
                    .is_some_and(|tab| !tab.document.is_image() && tab.view != View::Graph)
            {
                self.find.open = true;
                self.find.focus = true;
                self.editor.focus = false;
            }
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, Key::Tab)
            }) && !self.tabs.is_empty()
            {
                self.step_tab(true);
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Tab))
                && !self.tabs.is_empty()
            {
                self.step_tab(false);
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx
            .input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::T))
        {
            self.undo_close_tab();
        } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::T)) {
            self.new_tab();
        }
        if self.dialog.is_none()
            && ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::O)
            })
        {
            self.start_open_picker(rust_notes::native_picker::Kind::File);
            return;
        }
        if self.dialog.is_none()
            && ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::ALT, Key::O)
            })
        {
            self.start_open_picker(rust_notes::native_picker::Kind::Folder);
            return;
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::O)) {
            self.ask(Dialog::QuickOpen);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::N)) {
            self.ask(Dialog::NewNote);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::P)) {
            self.ask(Dialog::Commands);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::E)) {
            self.toggle_reading();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::W))
            && !self.tabs.is_empty()
        {
            self.request_close_tab(self.active);
        }
    }

    fn ribbon(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("ribbon")
            .exact_size(44.0)
            .resizable(false)
            .frame(
                egui::Frame::NONE
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin::symmetric(7, 5)),
            )
            .show(ui, |ui| {
                if icon_button(ui, Icon::SidebarLeft, "Toggle left sidebar").clicked() {
                    self.show_left = !self.show_left;
                }

                let new_note = theme::icon_toggle(
                    ui,
                    Icon::NewNote,
                    "New note · Ctrl+N",
                    self.dialog == Some(Dialog::NewNote),
                );
                crate::ui_probe::record(ui.ctx(), "ribbon-new-note", new_note.rect);
                if new_note.clicked() {
                    self.ask(Dialog::NewNote);
                }
                let graph = ui.add_enabled_ui(self.vault.is_some(), |ui| {
                    theme::icon_toggle(
                        ui,
                        Icon::Graph,
                        "Open graph view",
                        self.tabs
                            .get(self.active)
                            .is_some_and(|tab| tab.view == View::Graph),
                    )
                    .on_disabled_hover_text("Open a vault first.")
                });
                if graph.inner.clicked() {
                    self.open_global_graph();
                }
                if icon_button(ui, Icon::DailyNote, "Open today's daily note").clicked() {
                    self.open_daily();
                }
                if theme::icon_toggle(
                    ui,
                    Icon::Recovery,
                    "Open local history",
                    self.recovery_manager.active(),
                )
                .clicked()
                {
                    self.open_recovery(true);
                }
                if theme::icon_toggle(
                    ui,
                    Icon::Templates,
                    "Insert template",
                    self.dialog == Some(Dialog::InsertTemplate),
                )
                .clicked()
                {
                    if self.editable_note() {
                        self.ask(Dialog::InsertTemplate);
                    } else {
                        self.status = "Open a note to insert a template.".into();
                    }
                }
                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    Layout::bottom_up(Align::Center),
                    |ui| {
                        if theme::icon_toggle(
                            ui,
                            Icon::Commands,
                            "Command palette · Ctrl+P",
                            self.dialog == Some(Dialog::Commands),
                        )
                        .clicked()
                        {
                            self.ask(Dialog::Commands);
                        }
                    },
                );
            });
    }

    fn change_vault(&mut self) {
        if self.dirty() {
            self.pending = Some(Pending::ChangeVault);
        } else {
            self.ask(Dialog::OpenVault);
        }
    }

    fn request_vault(&mut self, root: PathBuf) {
        self.request_open_input(root, None);
    }

    fn request_open_input(&mut self, root: PathBuf, document: Option<PathBuf>) {
        if self.vault.as_ref().is_some_and(|vault| vault.root == root) {
            self.vault_manager.close();
            if let Some(path) = document {
                // Keep the current editor, Undo and pin intact, including unsaved buffers.
                self.open_note_at(&path, true);
            } else {
                self.focus_editor();
            }
        } else if self.dirty() {
            self.vault_manager.close();
            self.pending = Some(Pending::OpenVault(root, document));
        } else {
            self.startup_document = document;
            self.load_vault(&root);
        }
    }

    fn start_open_picker(&mut self, kind: rust_notes::native_picker::Kind) {
        let title = match kind {
            rust_notes::native_picker::Kind::File => "Open file",
            rust_notes::native_picker::Kind::Folder => "Open folder",
        };
        match self.open_picker.start(
            kind,
            self.vault.as_ref().map(|vault| vault.root.as_path()),
            title,
        ) {
            Ok(()) => {
                self.dialog = None;
                self.page_preview.close();
                self.editor.focus = false;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn open_system_selection(&mut self, kind: rust_notes::native_picker::Kind, path: PathBuf) {
        use rust_notes::{native_picker::Kind, vaults};
        let target = match kind {
            Kind::Folder => vaults::folder(&path).map(|root| (root, None)),
            Kind::File => (|| {
                let absolute = path
                    .canonicalize()
                    .map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
                let current = self.vault.as_ref().map(|vault| vault.root.as_path());
                let target = if let Some(current) =
                    current.filter(|root| absolute.starts_with(root))
                {
                    // Within the active vault, keep its links and workspace context.
                    vaults::resolve_input(&absolute, Some(current), &[])?
                } else {
                    let registered = vaults::Vaults::open().ok();
                    let default = std::env::var_os("RUST_NOTES_DEFAULT_VAULT").map(PathBuf::from);
                    let roots = registered.as_ref().map_or(&[][..], |list| list.roots());
                    vaults::resolve_input(&absolute, default.as_deref(), roots)?
                };
                if target.1.is_none() {
                    return Err(
                        "Choose an existing Markdown (.md) or plain-text (.txt) file.".into(),
                    );
                }
                Ok(target)
            })(),
        };
        match target {
            Ok((root, document)) => self.request_open_input(root, document),
            Err(error) => {
                self.error = Some(error);
                self.focus_editor();
            }
        }
    }

    fn manage_vaults(&mut self) {
        self.vault_manager
            .show_chooser(self.vault.as_ref().map(|vault| vault.root.as_path()));
    }

    fn save_bookmarks(&mut self, before: rust_notes::bookmarks::Bookmarks) -> bool {
        self.ctx.request_repaint();
        if let Err(error) = self.write_session() {
            self.bookmarks = before;
            self.status = "Bookmarks not saved".into();
            self.error = Some(format!("Bookmark changes weren't saved. {error}"));
            return false;
        }
        self.status = "Bookmarks saved".into();
        if !self.dirty() {
            self.error = None;
        }
        true
    }

    fn add_bookmark(&mut self, path: &Path) {
        let Some(vault) = &self.vault else {
            return;
        };
        let before = self.bookmarks.clone();
        match self.bookmarks.add(vault, path) {
            Ok(true) => {
                self.save_bookmarks(before);
            }
            Ok(false) => self.status = "This file is already bookmarked".into(),
            Err(error) => self.error = Some(error),
        }
        self.ctx.request_repaint();
    }

    fn bookmark_list(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);
        ui.label(RichText::new("Bookmarks").size(12.0).strong().weak());
        ui.add_space(8.0);
        let can_add = self
            .tabs
            .get(self.active)
            .is_some_and(|tab| !tab.document.is_blank());
        let add = ui.add_enabled(
            can_add,
            egui::Button::new(RichText::new("Bookmark current file").size(13.0))
                .frame_when_inactive(false),
        );
        crate::ui_probe::record(ui.ctx(), "bookmark-add-current", add.rect);
        if add.clicked()
            && let Some(path) = self
                .tabs
                .get(self.active)
                .map(|tab| tab.document.path.clone())
        {
            self.add_bookmark(&path);
        }
        ui.add_space(8.0);
        if self.bookmarks.items.is_empty() {
            ui.label(RichText::new("No bookmarks yet").small().weak());
        }
        let mut open = None;
        let mut action = None;
        ScrollArea::vertical()
            .id_salt("bookmarks")
            .max_height((ui.available_height() - 64.0).max(0.0))
            .show(ui, |ui| {
                for (index, item) in self.bookmarks.items.iter().enumerate() {
                    let active = self
                        .tabs
                        .get(self.active)
                        .is_some_and(|tab| tab.document.path == item.path);
                    let exists = self.vault.as_ref().is_some_and(|vault| {
                        vault.notes.iter().any(|note| note.path == item.path)
                            || vault.attachments.contains(&item.path)
                    });
                    let label = if exists {
                        item.label()
                    } else {
                        format!("{} (missing)", item.label())
                    };
                    let response = crate::file_tree::file_row(ui, &item.path, &label, active)
                        .on_hover_text(item.path.display().to_string());
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("bookmark:{}", item.path.display()),
                        response.rect,
                    );
                    response.context_menu(|ui| {
                        for (name, label, enabled) in [
                            ("title", "Edit title…", true),
                            ("up", "Move up", index > 0),
                            ("down", "Move down", index + 1 < self.bookmarks.items.len()),
                            ("remove", "Remove bookmark", true),
                        ] {
                            let response = ui.add_enabled(enabled, egui::Button::new(label));
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("bookmark-{name}"),
                                response.rect,
                            );
                            if response.clicked() {
                                action = Some((item.path.clone(), name));
                                ui.close();
                            }
                        }
                    });
                    if response.clicked() {
                        open = Some(item.path.clone());
                    }
                }
            });
        if let Some(path) = open {
            self.open_note_at(&path, ui.ctx().input(|input| input.modifiers.command));
            self.ctx.request_repaint();
        }
        if let Some((path, action)) = action {
            if action == "title" {
                let title = self
                    .bookmarks
                    .items
                    .iter()
                    .find(|item| item.path == path)
                    .and_then(|item| item.title.clone())
                    .unwrap_or_default();
                self.ask(Dialog::BookmarkTitle);
                self.dialog_path = Some(path);
                self.dialog_input = title;
            } else {
                let before = self.bookmarks.clone();
                match action {
                    "up" => self.bookmarks.move_item(&path, -1),
                    "down" => self.bookmarks.move_item(&path, 1),
                    "remove" => self.bookmarks.remove(&path),
                    _ => {}
                }
                self.save_bookmarks(before);
            }
        }
    }

    pub(crate) fn workspace_layout(&self) -> rust_notes::workspaces::Layout {
        rust_notes::workspaces::Layout {
            tabs: self
                .tabs
                .iter()
                .map(|tab| rust_notes::workspaces::SavedTab {
                    path: tab.document.path.clone(),
                    view: tab.view,
                    editing_view: tab.editing_view(),
                    pinned: tab.pinned,
                })
                .collect(),
            active: self.active,
            left: self.show_left,
            right: self.show_right,
            left_width: self.sidebar_widths[0],
            right_width: self.sidebar_widths[1],
            right_tab: self.right_tab,
        }
    }

    fn show_workspaces(&mut self, load_only: bool) {
        let Some(vault) = &self.vault else {
            self.error = Some("Open a vault first.".into());
            return;
        };
        self.workspace_manager.show_manager(&vault.root, load_only);
        self.dialog = None;
        self.editor.focus = false;
        self.ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
    }

    fn save_named_workspace(&mut self) -> Result<bool, String> {
        let vault = self.vault.as_ref().ok_or("Open a vault first.")?;
        let mut list = rust_notes::workspaces::Workspaces::open(&vault.root)?;
        let Some(name) = list.active().map(str::to_owned) else {
            return Ok(false);
        };
        list.save(
            &name,
            self.workspace_layout(),
            chrono::Local::now().timestamp_millis(),
        )?;
        self.status = format!("Workspace saved: {name}");
        Ok(true)
    }

    fn request_workspace(&mut self, name: String) {
        if self.dirty() {
            self.workspace_manager.open = false;
            self.pending = Some(Pending::LoadWorkspace(name));
        } else {
            self.load_workspace(&name);
        }
    }

    fn load_workspace(&mut self, name: &str) {
        let load = || -> Result<_, String> {
            let vault = self.vault.as_ref().ok_or("Open a vault first.")?;
            let mut list = rust_notes::workspaces::Workspaces::open(&vault.root)?;
            let layout = list
                .entries()
                .iter()
                .find(|entry| entry.name == name)
                .ok_or("This workspace no longer exists.")?
                .layout
                .clone();
            let restored = layout.restore(vault)?;
            list.activate(name)?;
            Ok((layout, restored))
        };
        match load() {
            Ok((layout, restored)) => {
                self.tabs = restored.tabs;
                self.active = restored.active;
                self.show_left = layout.left;
                self.show_right = layout.right;
                self.right_tab = layout.right_tab.min(2);
                self.sidebar_widths = [
                    layout.left_width.clamp(180.0, 450.0),
                    layout.right_width.clamp(170.0, 420.0),
                ];
                self.restore_sidebars = [true; 2];
                self.workspace_manager.open = false;
                self.error = None;
                self.find = Default::default();
                self.title = Default::default();
                self.properties = Default::default();
                self.pending_anchor = None;
                self.status = format!("Workspace loaded: {name}");
                self.focus_editor();
            }
            Err(error) => {
                if self.workspace_manager.open {
                    self.workspace_manager.show_error(error);
                } else {
                    self.error = Some(error);
                }
            }
        }
    }

    fn prepare_sidebar(&mut self, ui: &egui::Ui, salt: &str, index: usize) {
        if self.restore_sidebars[index] {
            let state = egui::containers::panel::PanelState {
                outer_rect: egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(self.sidebar_widths[index], 1.0),
                ),
            };
            ui.ctx()
                .data_mut(|data| data.insert_persisted(ui.scope_id().with(salt), state));
            self.restore_sidebars[index] = false;
        }
    }

    fn capture_sidebar(&mut self, ui: &egui::Ui, salt: &str, index: usize) {
        if let Some(state) =
            egui::containers::panel::PanelState::load(ui.ctx(), ui.scope_id().with(salt))
        {
            self.sidebar_widths[index] = state.size().x;
            crate::ui_probe::record(ui.ctx(), format!("sidebar:{salt}"), state.outer_rect);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        if !self.show_left {
            return;
        }
        let mut selected = None;
        self.prepare_sidebar(ui, "explorer", 0);
        egui::Panel::left("explorer")
            .default_size(300.0)
            .min_size(180.0)
            .max_size(450.0)
            .frame(
                egui::Frame::NONE
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin::symmetric(12, 0)),
            )
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 40.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_height(40.0);
                        let files = theme::icon_toggle(
                            ui,
                            Icon::Files,
                            "File explorer",
                            !self.searching && !self.show_bookmarks,
                        );
                        if files.clicked() {
                            self.searching = false;
                            self.show_bookmarks = false;
                        }
                        let search = theme::icon_toggle(
                            ui,
                            Icon::Search,
                            "Search notes",
                            self.searching && !self.show_bookmarks,
                        );
                        if search.clicked() {
                            self.searching = true;
                            self.show_bookmarks = false;
                            self.search_focus = true;
                        }
                        let bookmarks = theme::icon_toggle(
                            ui,
                            Icon::Bookmark,
                            "Bookmarks",
                            self.show_bookmarks,
                        );
                        crate::ui_probe::record(ui.ctx(), "bookmarks-tab", bookmarks.rect);
                        if bookmarks.clicked() {
                            self.show_bookmarks = true;
                        }
                        let selected = if self.show_bookmarks {
                            &bookmarks
                        } else if self.searching {
                            &search
                        } else {
                            &files
                        };
                        ui.painter().line_segment(
                            [selected.rect.left_bottom(), selected.rect.right_bottom()],
                            Stroke::new(1.5, theme::accent(ui)),
                        );
                    },
                );
                if !self.show_bookmarks {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 40.0),
                        Layout::left_to_right(Align::Center),
                        |ui| {
                            ui.set_min_height(40.0);
                            ui.spacing_mut().item_spacing.x = 2.0;
                            ui.add_space(74.0);
                            if theme::icon_toggle(
                                ui,
                                Icon::NewNote,
                                "New note · Ctrl+N",
                                self.dialog == Some(Dialog::NewNote),
                            )
                            .clicked()
                            {
                                self.ask(Dialog::NewNote);
                            }
                            let new_folder = theme::icon_toggle(
                                ui,
                                Icon::NewFolder,
                                "New folder",
                                self.dialog == Some(Dialog::NewFolder),
                            );
                            crate::ui_probe::record(ui.ctx(), "new-folder", new_folder.rect);
                            if new_folder.clicked() {
                                self.ask(Dialog::NewFolder);
                            }
                            let sort = icon_button(ui, Icon::Sort, "Change sort order");
                            crate::ui_probe::record(ui.ctx(), "file-sort", sort.rect);
                            egui::Popup::menu(&sort).show(|ui| {
                                for (descending, label, probe) in [
                                    (false, "File name (A to Z)", "sort-ascending"),
                                    (true, "File name (Z to A)", "sort-descending"),
                                ] {
                                    let response = ui.selectable_label(
                                        self.sort_descending == descending,
                                        label,
                                    );
                                    crate::ui_probe::record(ui.ctx(), probe, response.rect);
                                    if response.clicked() {
                                        self.sort_descending = descending;
                                        self.reset_tree_scroll = true;
                                        ui.close();
                                    }
                                }
                            });
                            let collapse =
                                icon_button(ui, Icon::CollapseAll, "Collapse all folders");
                            crate::ui_probe::record(ui.ctx(), "file-collapse", collapse.rect);
                            if collapse.clicked() {
                                self.collapse_epoch = self.collapse_epoch.wrapping_add(1);
                                self.reveal_folder = None;
                            }
                        },
                    );
                    ui.add_space(8.0);
                }

                if self.show_bookmarks {
                    self.bookmark_list(ui);
                } else if self.searching {
                    let input = ui.add(
                        TextEdit::singleline(&mut self.search)
                            .hint_text("Search your notes…")
                            .desired_width(f32::INFINITY)
                            .font(FontId::proportional(14.0)),
                    );
                    if self.search_focus {
                        input.request_focus();
                        self.search_focus = false;
                    }
                    ui.add_space(10.0);
                    if let Some(vault) = &self.vault {
                        self.search_cache.update(&self.search, vault);
                        let results = &self.search_cache.paths;
                        if let Some(error) = &self.search_cache.error {
                            ui.colored_label(
                                Color32::from_rgb(220, 135, 110),
                                RichText::new(error).size(12.0),
                            );
                        }
                        ui.label(
                            RichText::new(format!("{} results", results.len()))
                                .small()
                                .weak(),
                        );
                        ScrollArea::vertical()
                            .id_salt("search-results")
                            .max_height((ui.available_height() - 64.0).max(0.0))
                            .show_rows(ui, 52.0, results.len(), |ui, rows| {
                                for note in &results[rows] {
                                    ui.allocate_ui_with_layout(
                                        egui::vec2(ui.available_width(), 52.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            let result = ui.add(
                                                egui::Button::new(
                                                    RichText::new(
                                                        note.file_stem()
                                                            .unwrap_or_default()
                                                            .to_string_lossy(),
                                                    )
                                                    .size(14.0),
                                                )
                                                .frame_when_inactive(false),
                                            );
                                            crate::ui_probe::record(
                                                ui.ctx(),
                                                format!("search:{}", note.display()),
                                                result.rect,
                                            );
                                            if result.clicked() {
                                                selected = Some(FileAction::Open(note.clone()));
                                            }
                                            ui.add(
                                                egui::Label::new(
                                                    RichText::new(note.display().to_string())
                                                        .size(11.0)
                                                        .weak(),
                                                )
                                                .truncate(),
                                            );
                                        },
                                    );
                                }
                            });
                    }
                } else {
                    ui.label(
                        RichText::new(
                            self.vault
                                .as_ref()
                                .map(Vault::name)
                                .unwrap_or("No vault".into()),
                        )
                        .size(12.0)
                        .strong()
                        .weak(),
                    );
                    ui.add_space(8.0);
                    let mut tree_scroll = ScrollArea::vertical()
                        .id_salt("file-tree")
                        .max_height((ui.available_height() - 64.0).max(0.0));
                    if self.reset_tree_scroll {
                        tree_scroll = tree_scroll.vertical_scroll_offset(0.0);
                        self.reset_tree_scroll = false;
                    }
                    tree_scroll.show(ui, |ui| {
                        let (action, revealed) = self.tree.show(
                            ui,
                            "",
                            self.tabs
                                .get(self.active)
                                .map(|tab| tab.document.path.as_path()),
                            self.sort_descending,
                            self.collapse_epoch,
                            self.reveal_folder.as_deref(),
                        );
                        selected = action;
                        if revealed {
                            self.reveal_folder = None;
                        }
                    });
                }
                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    Layout::bottom_up(Align::LEFT),
                    |ui| {
                        let name = self
                            .vault
                            .as_ref()
                            .map(Vault::name)
                            .unwrap_or_else(|| "Open vault…".into());
                        ui.horizontal(|ui| {
                            let width = (ui.available_width() - 38.0).max(40.0);
                            let vault = ui.add_sized(
                                [width, 30.0],
                                egui::Button::new((
                                    theme::icon_image(
                                        Icon::Disclosure,
                                        ui.visuals().weak_text_color(),
                                    )
                                    .rotate(std::f32::consts::FRAC_PI_2, egui::Vec2::splat(0.5)),
                                    RichText::new(name).size(13.0),
                                ))
                                .frame(false)
                                .truncate(),
                            );
                            crate::ui_probe::record(ui.ctx(), "vault-profile", vault.rect);
                            if vault.clicked() {
                                self.manage_vaults();
                            }
                            let settings =
                                icon_button(ui, Icon::Settings, "Open settings · Ctrl+,");
                            crate::ui_probe::record(ui.ctx(), "open-settings", settings.rect);
                            if settings.clicked() {
                                self.ask(Dialog::Settings);
                            }
                        });
                        ui.separator();
                    },
                );
            });
        self.capture_sidebar(ui, "explorer", 0);
        if let Some(action) = selected {
            match action {
                FileAction::Open(path) => self.open_note_at(
                    &path,
                    self.new_tab_gesture || ui.ctx().input(|i| i.modifiers.command),
                ),
                FileAction::OpenInNewTab(path) => self.open_note_at(&path, true),
                FileAction::Copy(path) => {
                    self.duplicate_file(&path);
                }
                FileAction::Move(path) => self.ask_move_file(path),
                FileAction::CopyPath(path) => {
                    if let Some(vault) = &self.vault {
                        ui.ctx()
                            .copy_text(vault.root.join(path).display().to_string());
                    }
                }
                FileAction::Rename(path) => self.ask_file(Dialog::RenameNote, path),
                FileAction::Trash(path) => self.ask_file(Dialog::TrashNote, path),
                FileAction::Bookmark(path) => self.add_bookmark(&path),
            }
        }
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        if !self.show_right || ui.available_width() < 700.0 || self.view() == View::Graph {
            return;
        }
        let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document) else {
            return;
        };
        let outline = rust_notes::structure::outline(&document.text);
        let outgoing = wiki_links(&document.text);
        let backlinks: Vec<_> = self
            .vault
            .as_ref()
            .map(|vault| {
                vault
                    .backlinks(&document.path)
                    .iter()
                    .map(|note| (note.path.clone(), note.title()))
                    .collect()
            })
            .unwrap_or_default();
        let mut selected = None;
        let mut follow = None;
        let mut jump = None;
        self.prepare_sidebar(ui, "details", 1);
        egui::Panel::right("details")
            .default_size(234.0)
            .min_size(170.0)
            .max_size(420.0)
            .frame(
                egui::Frame::NONE
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin::symmetric(17, 18)),
            )
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    for (tab, title, key) in [
                        (0, "Outline", "outline"),
                        (1, "Backlinks", "backlinks"),
                        (2, "Outgoing", "outgoing"),
                    ] {
                        let response = ui.selectable_value(
                            &mut self.right_tab,
                            tab,
                            RichText::new(title).size(12.0),
                        );
                        crate::ui_probe::record(ui.ctx(), format!("panel:{key}"), response.rect);
                    }
                });
                ui.add_space(10.0);
                ScrollArea::vertical().show(ui, |ui| {
                    if self.right_tab == 0 {
                        ui.horizontal(|ui| {
                            ui.add(theme::icon_image(
                                Icon::Outline,
                                ui.visuals().weak_text_color(),
                            ));
                            ui.label(RichText::new("Outline").size(14.0).strong());
                        });
                        ui.add_space(9.0);
                        if outline.is_empty() {
                            ui.label(RichText::new("No headings yet").size(13.0).weak());
                        }
                        for heading in outline {
                            let depth = heading.level;
                            ui.horizontal_wrapped(|ui| {
                                ui.add_space((depth.saturating_sub(1) * 10) as f32);
                                if ui
                                    .selectable_label(
                                        false,
                                        RichText::new(&heading.title).size(13.0),
                                    )
                                    .clicked()
                                {
                                    jump = Some(heading.title);
                                }
                            });
                        }
                    }
                    if self.right_tab == 1 {
                        ui.label(RichText::new("Backlinks").size(14.0).strong());
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(format!("{} linked mentions", backlinks.len()))
                                .size(12.0)
                                .weak(),
                        );
                        ui.add_space(9.0);
                        for (path, title) in backlinks {
                            let row = ui.add(
                                egui::Button::new(
                                    RichText::new(title).size(13.0).color(theme::accent(ui)),
                                )
                                .frame_when_inactive(false),
                            );
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("backlink:{}", path.display()),
                                row.rect,
                            );
                            if row.clicked() {
                                selected = Some(path);
                            }
                        }
                    }
                    if self.right_tab == 2 {
                        ui.label(RichText::new("Outgoing links").size(14.0).strong());
                        ui.add_space(9.0);
                        if outgoing.is_empty() {
                            ui.label(
                                RichText::new("Connect a note with [[links]]")
                                    .size(12.0)
                                    .weak(),
                            );
                        }
                        for target in outgoing {
                            let label = target
                                .split_once('|')
                                .map(|(_, label)| label)
                                .unwrap_or(&target);
                            let row = ui.add(
                                egui::Button::new(
                                    RichText::new(label).size(13.0).color(theme::accent(ui)),
                                )
                                .frame_when_inactive(false),
                            );
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("outgoing:{target}"),
                                row.rect,
                            );
                            if row.clicked() {
                                follow = Some(target);
                            }
                        }
                    }
                });
            });
        self.capture_sidebar(ui, "details", 1);
        if let Some(fragment) = jump {
            self.jump_to_anchor(&fragment);
        }
        if let Some(path) = selected {
            self.open_note(&path);
        }
        if let Some(target) = follow {
            self.follow_link(&target);
        }
    }

    fn open_global_graph(&mut self) {
        if self.vault.is_none() || self.view() == View::Graph {
            return;
        }
        if self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.document.dirty())
            && !self.save(self.active)
        {
            return;
        }
        if self.tabs.is_empty() || self.tabs[self.active].pinned {
            self.new_tab();
        }
        self.tabs[self.active].set_view(View::Graph);
        self.find = Default::default();
        self.page_preview.close();
        self.focus_editor();
    }

    fn local_graph_available(&self) -> bool {
        self.tabs.get(self.active).is_some_and(|tab| {
            tab.view != View::Graph
                && !tab.document.is_blank()
                && !tab.document.is_image()
                && !tab.document.is_plain_text()
        })
    }

    fn interaction_blocked(&self) -> bool {
        self.dialog.is_some()
            || self.open_picker.active()
            || self.pending.is_some()
            || self.loading_root.is_some()
            || self.vault_manager.open
            || self.workspace_manager.open
            || self.recovery_manager.active()
            || self.file_move.active()
    }

    fn graph_view(&mut self, ui: &mut egui::Ui, local: bool) {
        let blocked = self.interaction_blocked();
        let root = local.then(|| self.tabs[self.active].document.path.clone());
        let action = self
            .vault
            .as_ref()
            .and_then(|vault| self.graph.show(ui, vault, root, local, blocked));
        match action {
            Some(crate::graph_ui::Action::CloseLocal) => self.graph.local_open = false,
            Some(crate::graph_ui::Action::Open { path, kind }) => match kind {
                rust_notes::graph::Kind::Note | rust_notes::graph::Kind::Attachment => {
                    self.open_note(&path)
                }
                rust_notes::graph::Kind::Unresolved => {
                    self.follow_link_from(Path::new(""), &path.to_string_lossy())
                }
                rust_notes::graph::Kind::Tag => {
                    self.search = format!("tag:{}", path.display());
                    self.searching = true;
                    self.show_left = true;
                    self.show_bookmarks = false;
                    self.search_focus = true;
                    self.editor.focus = false;
                }
            },
            None => {}
        }
    }

    fn content(&mut self, ui: &mut egui::Ui) {
        let background = if self.dark {
            theme::DARK_BG
        } else {
            Color32::from_rgb(247, 246, 242)
        };
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(background))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if self.open_picker.active() {
                    ui.disable();
                }
                if self.loading_root.is_some() {
                    ui.centered_and_justified(|ui| {
                        ui.label("Opening vault…");
                    });
                    return;
                }
                self.tab_strip(ui);
                if self.view() == View::Graph {
                    egui::Frame::NONE
                        .inner_margin(12)
                        .show(ui, |ui| self.graph_view(ui, false));
                    return;
                }
                if self
                    .tabs
                    .get(self.active)
                    .is_none_or(|tab| tab.document.is_blank())
                {
                    ui.vertical_centered(|ui| {
                        ui.add_space((ui.available_height() * 0.35).max(60.0));
                        ui.label(
                            RichText::new(if self.loading.is_some() {
                                "Opening vault…"
                            } else {
                                "No file is open"
                            })
                            .size(24.0),
                        );
                        ui.add_space(22.0);
                        if ui.link("Create new note (Ctrl+N)").clicked() {
                            self.ask(Dialog::NewNote);
                        }
                        if ui.link("Go to file (Ctrl+O)").clicked() {
                            self.ask(Dialog::QuickOpen);
                        }
                        if !self.tabs.is_empty() && ui.link("Close").clicked() {
                            self.close_tab(self.active);
                        }
                    });
                    return;
                }
                if self.graph.local_open && self.local_graph_available() {
                    let maximum = (ui.available_width() - 180.0).max(160.0);
                    egui::Panel::right("local-graph-pane")
                        .default_size(440.0)
                        .size_range(160.0..=maximum)
                        .resizable(true)
                        .show(ui, |ui| {
                            crate::ui_probe::record(ui.ctx(), "local-graph-pane", ui.max_rect());
                            egui::Frame::NONE
                                .inner_margin(10)
                                .show(ui, |ui| self.graph_view(ui, true));
                        });
                }
                egui::Frame::NONE
                    .inner_margin(Margin::symmetric(12, 0))
                    .show(ui, |ui| {
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 38.375),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                if ui
                                    .add_enabled_ui(
                                        self.tabs[self.active].history.can_back(),
                                        |ui| {
                                            icon_button(ui, Icon::Back, "Navigate back · Alt+Left")
                                        },
                                    )
                                    .inner
                                    .clicked()
                                {
                                    self.navigate(true);
                                }
                                if ui
                                    .add_enabled_ui(
                                        self.tabs[self.active].history.can_forward(),
                                        |ui| {
                                            icon_button(
                                                ui,
                                                Icon::Forward,
                                                "Navigate forward · Alt+Right",
                                            )
                                        },
                                    )
                                    .inner
                                    .clicked()
                                {
                                    self.navigate(false);
                                }
                                let path = rust_notes::paths::display_path(
                                    &self.tabs[self.active].document.path,
                                )
                                .replace('/', " / ");
                                let available = ui.available_width();
                                let actions_width = if self.tabs[self.active].document.is_image() {
                                    144.0
                                } else {
                                    76.0
                                };
                                ui.allocate_ui_with_layout(
                                    egui::vec2((available - actions_width).max(20.0), 30.0),
                                    Layout::left_to_right(Align::Center)
                                        .with_main_align(Align::Center),
                                    |ui| {
                                        let (rect, _) = ui.allocate_exact_size(
                                            egui::vec2((available - 76.0).max(20.0), 30.0),
                                            egui::Sense::hover(),
                                        );
                                        let galley = egui::WidgetText::from(
                                            RichText::new(path).size(13.0).weak(),
                                        )
                                        .into_galley(
                                            ui,
                                            Some(egui::TextWrapMode::Truncate),
                                            rect.width(),
                                            FontId::proportional(13.0),
                                        );
                                        let position = rect.center() - galley.size() / 2.0;
                                        ui.painter().galley(
                                            position,
                                            galley,
                                            ui.visuals().weak_text_color(),
                                        );
                                    },
                                );
                                if self.tabs[self.active].document.is_image() {
                                    use rust_notes::tab::ImageScale;
                                    for (label, scale) in
                                        [("Fit", ImageScale::Fit), ("100%", ImageScale::Actual)]
                                    {
                                        let response = ui.selectable_label(
                                            self.tabs[self.active].image_scale == scale,
                                            label,
                                        );
                                        crate::ui_probe::record(
                                            ui.ctx(),
                                            format!("image-scale:{label}"),
                                            response.rect,
                                        );
                                        if response.clicked() {
                                            self.tabs[self.active].image_scale = scale;
                                        }
                                    }
                                    let more = icon_button(ui, Icon::More, "More options");
                                    egui::Popup::menu(&more).show(|ui| {
                                        for (label, dialog) in [
                                            ("Rename or move…", Dialog::RenameNote),
                                            ("Move to system Trash…", Dialog::TrashNote),
                                        ] {
                                            if ui.button(label).clicked() {
                                                self.ask_file(
                                                    dialog,
                                                    self.tabs[self.active].document.path.clone(),
                                                );
                                                ui.close();
                                            }
                                        }
                                    });
                                    return;
                                }
                                let reading = self.view() == View::Read;
                                if icon_button(
                                    ui,
                                    if reading { Icon::Edit } else { Icon::Reading },
                                    if reading {
                                        "Editing view · Ctrl+E"
                                    } else {
                                        "Reading view · Ctrl+E"
                                    },
                                )
                                .clicked()
                                {
                                    self.toggle_reading();
                                }
                                let more = icon_button(ui, Icon::More, "More options");
                                egui::Popup::menu(&more).show(|ui| {
                                    if ui.button("Save · Ctrl+S").clicked() {
                                        self.save(self.active);
                                        ui.close();
                                    }
                                    if ui.button("Save a copy…").clicked() {
                                        self.ask(Dialog::SaveCopy);
                                        ui.close();
                                    }
                                    ui.separator();
                                    for (label, view) in [
                                        ("Reading view", View::Read),
                                        ("Live preview", View::Live),
                                        ("Source editing", View::Source),
                                        ("Split source and preview", View::Split),
                                    ] {
                                        if ui.selectable_label(self.view() == view, label).clicked()
                                        {
                                            self.set_view(view);
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    if ui.button(rust_notes::commands::OPEN_GRAPH).clicked() {
                                        self.open_global_graph();
                                        ui.close();
                                    }
                                    if self.local_graph_available()
                                        && ui
                                            .button(rust_notes::commands::OPEN_LOCAL_GRAPH)
                                            .clicked()
                                    {
                                        self.graph.local_open = true;
                                        ui.close();
                                    }
                                });
                            },
                        );
                    });
                self.document_view(ui);
            });
    }

    fn document_view(&mut self, ui: &mut egui::Ui) {
        let view = self.view();
        if view == View::Graph {
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.document.is_image() {
            self.pending_anchor = None;
            if let Some(vault) = &self.vault {
                crate::image_view::show(
                    ui,
                    &vault.root.join(&tab.document.path),
                    ui.scope_id().with(("image", tab.id(), &tab.document.path)),
                    tab.image_scale,
                );
            }
            return;
        }
        let id = ui
            .scope_id()
            .with(("note-editor", tab.id(), tab.editor_path()));
        crate::editor_ui::find_bar(ui, &mut self.find, tab, id);
        let mut follow = None;
        let anchor = self.pending_anchor.take();
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        ui.spacing_mut().scroll.bar_width = 6.0;
        ui.spacing_mut().scroll.bar_inner_margin = 4.0;
        ui.spacing_mut().scroll.bar_outer_margin = 2.0;
        let mut add_property = false;
        let mut renamed_title = None;
        let title = tab.document.title();
        let scroll = ScrollArea::vertical()
            .id_salt(("document", tab.id(), tab.editor_path()))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                theme::document_style(ui, self.document_font_size);
                let width = ui.available_width();
                let content_width = if self.editor_preferences.readable_line_length {
                    width.min(920.0)
                } else {
                    width
                }
                .min((width - 48.0).max(100.0));
                crate::ui_probe::metric(ui.ctx(), "document-content-width", content_width);
                let margin = ((width - content_width) / 2.0).max(0.0);
                if view == View::Split {
                    ui.columns(2, |columns| {
                        follow = self.editor.show(
                            &mut columns[0],
                            tab,
                            id,
                            &mut self.find,
                            &mut self.files,
                            self.vault.as_ref(),
                        );
                        let preview_link = preview::show(
                            &mut columns[1],
                            &mut self.cache,
                            tab,
                            anchor.as_deref(),
                            self.vault.as_ref(),
                        );
                        follow = follow.take().or(preview_link);
                    });
                } else {
                    ui.horizontal_top(|ui| {
                        ui.add_space(margin);
                        ui.allocate_ui_with_layout(
                            egui::vec2(content_width, 0.0),
                            Layout::top_down(Align::Min),
                            |ui| {
                                ui.set_width(content_width);
                                ui.add_space(32.0);
                                if self.editor_preferences.inline_title
                                    && let Some(name) =
                                        self.title.show(ui, &tab.document.path, &title)
                                {
                                    renamed_title = Some((tab.document.path.clone(), name));
                                }
                                if self.editor_preferences.inline_title {
                                    ui.add_space(16.218);
                                }
                                if matches!(view, View::Live | View::Read) {
                                    add_property = self.properties.show(ui, tab);
                                }
                                if view == View::Read {
                                    follow = preview::show(
                                        ui,
                                        &mut self.cache,
                                        tab,
                                        anchor.as_deref(),
                                        self.vault.as_ref(),
                                    );
                                } else {
                                    follow = self.editor.show(
                                        ui,
                                        tab,
                                        id,
                                        &mut self.find,
                                        &mut self.files,
                                        self.vault.as_ref(),
                                    );
                                }
                                ui.add_space(ui.available_height().max(80.0));
                            },
                        );
                    });
                }
            });
        crate::ui_probe::metric(
            ui.ctx(),
            "document-scroll-y",
            scroll.state.clamped_offset().y,
        );
        if let Some((source, name)) = renamed_title {
            if name.is_empty() || name.contains(['/', '\\']) || matches!(name.as_str(), "." | "..")
            {
                self.error = Some("Enter a file name without folder separators.".into());
                self.title.reject();
            } else {
                let extension = source
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or("md");
                let target = source.with_file_name(format!("{name}.{extension}"));
                let previous = self.dialog_path.replace(source);
                let previous_input =
                    std::mem::replace(&mut self.dialog_input, target.display().to_string());
                if !self.submit_dialog(Dialog::RenameNote) {
                    self.title.reject();
                }
                self.dialog_path = previous;
                self.dialog_input = previous_input;
            }
        }
        if add_property {
            self.ask(Dialog::AddProperty);
        }
        if let Some(target) = follow {
            self.follow_link(&target);
        }
    }

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        crate::ui_probe::metric(ui.ctx(), "tab-count", self.tabs.len() as f32);
        let mut close = None;
        let mut pin = None;
        egui::Frame::NONE
            .fill(ui.visuals().panel_fill)
            .inner_margin(Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                ui.horizontal(|ui| {
                    let count = self.tabs.len().max(1) as f32;
                    let width = (ui.available_width() - 88.0)
                        .max(80.0)
                        .min(200.0 * count + 4.0 * (count - 1.0));
                    let tab_width = ((width - 4.0 * (count - 1.0)) / count).clamp(32.0, 200.0);
                    let selected = self.tabs.get(self.active);
                    let reveal_key = (
                        selected.map_or(0, Tab::id),
                        selected.map(|tab| tab.document.path.clone()),
                        self.tabs.len(),
                        width.to_bits(),
                    );
                    let state_id = ui.scope_id().with("tab-reveal");
                    let reveal_active = ui.data_mut(|data| {
                        let changed = data
                            .get_temp::<(u64, Option<PathBuf>, usize, u32)>(state_id)
                            .as_ref()
                            != Some(&reveal_key);
                        data.insert_temp(state_id, reveal_key);
                        changed
                    });
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 40.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_size(egui::vec2(width, 40.0));
                            ScrollArea::horizontal()
                                .id_salt("tab-strip")
                                .max_height(40.0)
                                .scroll_bar_visibility(
                                    egui::scroll_area::ScrollBarVisibility::AlwaysHidden,
                                )
                                .show(ui, |ui| {
                                    ui.add_space(7.0);
                                    ui.horizontal(|ui| {
                                        for (index, tab) in self.tabs.iter().enumerate() {
                                            let (rect, response) = ui.allocate_exact_size(
                                                egui::vec2(tab_width, 33.0),
                                                egui::Sense::click(),
                                            );
                                            let active = index == self.active;
                                            let show_close = tab.pinned
                                                || active
                                                || ui.rect_contains_pointer(rect);
                                            if active && reveal_active {
                                                ui.scroll_to_rect_animation(
                                                    rect,
                                                    Some(Align::Center),
                                                    egui::style::ScrollAnimation::none(),
                                                );
                                            }
                                            if rect.intersects(ui.clip_rect()) {
                                                crate::ui_probe::record(
                                                    ui.ctx(),
                                                    format!("tab:{}", tab.document.path.display()),
                                                    rect.intersect(ui.clip_rect()),
                                                );
                                                crate::ui_probe::record(
                                                    ui.ctx(),
                                                    format!(
                                                        "tab-full:{}",
                                                        tab.document.path.display()
                                                    ),
                                                    rect,
                                                );
                                            }
                                            if active || response.hovered() {
                                                ui.painter().rect_filled(
                                                    rect,
                                                    6,
                                                    if active {
                                                        ui.visuals().window_fill
                                                    } else {
                                                        ui.visuals().widgets.hovered.weak_bg_fill
                                                    },
                                                );
                                            }
                                            let label = format!(
                                                "{}{}",
                                                tab.title(),
                                                if tab.document.dirty() { " •" } else { "" }
                                            );
                                            let galley = egui::WidgetText::from(
                                                RichText::new(label).size(13.0),
                                            )
                                            .into_galley(
                                                ui,
                                                Some(egui::TextWrapMode::Truncate),
                                                (tab_width - if show_close { 46.0 } else { 20.0 })
                                                    .max(1.0),
                                                FontId::proportional(13.0),
                                            );
                                            ui.painter().galley(
                                                egui::pos2(
                                                    rect.left() + 10.0,
                                                    rect.center().y - galley.size().y / 2.0,
                                                ),
                                                galley,
                                                ui.visuals().text_color(),
                                            );
                                            let cross = egui::Rect::from_center_size(
                                                rect.right_center() - egui::vec2(16.0, 0.0),
                                                egui::vec2(24.0, 26.0),
                                            );
                                            let closing = ui
                                                .interact(
                                                    cross,
                                                    ui.scope_id().with(("close-tab", tab.id())),
                                                    egui::Sense::click(),
                                                )
                                                .on_hover_text(if tab.pinned {
                                                    "Unpin"
                                                } else {
                                                    "Close tab · Ctrl+W"
                                                });
                                            if show_close {
                                                theme::control_feedback(ui, &closing);
                                                theme::icon_image(
                                                    if tab.pinned {
                                                        Icon::Pin
                                                    } else {
                                                        Icon::Close
                                                    },
                                                    if closing.hovered() {
                                                        ui.visuals().text_color()
                                                    } else if tab.pinned {
                                                        theme::accent(ui)
                                                    } else {
                                                        ui.visuals().weak_text_color()
                                                    },
                                                )
                                                .fit_to_exact_size(egui::vec2(12.0, 12.0))
                                                .paint_at(
                                                    ui,
                                                    egui::Rect::from_center_size(
                                                        cross.center(),
                                                        egui::vec2(12.0, 12.0),
                                                    ),
                                                );
                                                crate::ui_probe::record(
                                                    ui.ctx(),
                                                    format!(
                                                        "{}:{}",
                                                        if tab.pinned {
                                                            "unpin-tab"
                                                        } else {
                                                            "close-tab"
                                                        },
                                                        tab.document.path.display()
                                                    ),
                                                    cross,
                                                );
                                            }
                                            if closing.clicked() {
                                                if tab.pinned {
                                                    pin = Some(index);
                                                } else {
                                                    close = Some(index);
                                                }
                                            } else if response.clicked() {
                                                self.active = index;
                                                self.editor.focus =
                                                    !matches!(tab.view, View::Read | View::Graph);
                                            }
                                            response.context_menu(|ui| {
                                                theme::menu_style(ui);
                                                let action =
                                                    theme::menu_item(ui, Icon::Close, "Close");
                                                crate::ui_probe::record(
                                                    ui.ctx(),
                                                    "tab-menu-close",
                                                    action.rect,
                                                );
                                                if action.clicked() {
                                                    close = Some(index);
                                                    ui.close();
                                                }
                                                ui.separator();
                                                let action = theme::menu_item(
                                                    ui,
                                                    Icon::Pin,
                                                    if tab.pinned { "Unpin" } else { "Pin" },
                                                );
                                                crate::ui_probe::record(
                                                    ui.ctx(),
                                                    "tab-menu-pin",
                                                    action.rect,
                                                );
                                                if action.clicked() {
                                                    pin = Some(index);
                                                    ui.close();
                                                }
                                            });
                                            if active {
                                                ui.painter().line_segment(
                                                    [rect.left_bottom(), rect.right_bottom()],
                                                    Stroke::new(2.0, theme::interactive(ui)),
                                                );
                                            }
                                        }
                                    });
                                });
                        },
                    );
                    let new_tab = icon_button(ui, Icon::NewTab, "New tab · Ctrl+T");
                    crate::ui_probe::record(ui.ctx(), "new-tab", new_tab.rect);
                    if new_tab.clicked() {
                        self.new_tab();
                    }
                    ui.add_space((ui.available_width() - 42.0).max(0.0));
                    if icon_button(ui, Icon::SidebarRight, "Toggle right sidebar").clicked() {
                        self.show_right = !self.show_right;
                    }
                });
            });
        if let Some(index) = close {
            self.request_close_tab(index);
        }
        if let Some(index) = pin {
            self.toggle_pin(index);
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.open_picker.active() {
            let kind = self.open_picker.kind();
            if let Some(result) = self.open_picker.poll() {
                match result {
                    Ok(Some(path)) => self.open_system_selection(kind, path),
                    Ok(None) => self.focus_editor(),
                    Err(error) => {
                        self.error = Some(error);
                        self.focus_editor();
                    }
                }
                ctx.request_repaint();
            } else {
                let mut cancel = false;
                let response =
                    egui::Modal::new(egui::Id::unique("system-open-picker")).show(ctx, |ui| {
                        theme::dialog_style(ui);
                        ui.set_width(400.0);
                        ui.heading(match kind {
                            rust_notes::native_picker::Kind::File => "Open file",
                            rust_notes::native_picker::Kind::Folder => "Open folder",
                        });
                        ui.label("Choose an existing item in the file browser.");
                        ui.add_space(16.0);
                        let button = ui.button("Cancel");
                        crate::ui_probe::record(ctx, "system-open-cancel", button.rect);
                        crate::ui_probe::record(ctx, "system-open-picker", ui.max_rect());
                        cancel = button.clicked();
                    });
                if cancel || response.should_close() {
                    self.open_picker.cancel();
                    self.focus_editor();
                } else {
                    ctx.request_repaint_after(Duration::from_millis(100));
                }
            }
            return;
        }
        if let Some(pending) = self.pending.clone() {
            let response = egui::Modal::new(egui::Id::unique("unsaved-changes")).show(ctx, |ui| {
                theme::dialog_style(ui);
                ui.set_width(440.0);
                ui.heading("Keep your changes?");
                ui.label("You have unsaved edits. Save them before continuing, or discard them explicitly.");
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    let save = ui.button("Save & continue");
                    crate::ui_probe::record(ctx, "pending-save", save.rect);
                    if save.clicked() {
                        let indices: Vec<usize> = match &pending {
                            Pending::CloseTab(index) => vec![*index],
                            Pending::CloseOtherTabs(keep) => rust_notes::tab::other_tab_indices(&self.tabs, *keep)
                                .into_iter().filter(|&index| self.tabs[index].document.dirty()).collect(),
                            _ => self.tabs.iter().enumerate().filter(|(_, tab)| tab.document.dirty()).map(|(i, _)| i).collect(),
                        };
                        if indices.into_iter().all(|index| self.save(index)) {
                            let mut next = pending.clone();
                            if let Pending::RestoreSnapshot(request) = &mut next
                                && let Some(tab) = self.tabs.iter().find(|tab|tab.document.path == request.expected.path) {
                                request.expected = tab.document.clone();
                            }
                            self.pending = None; self.run_pending(next, ctx);
                        }
                    }
                    let discard = ui.button("Discard");
                    crate::ui_probe::record(ctx, "pending-discard", discard.rect);
                    if discard.clicked() { self.pending = None; self.run_pending(pending.clone(), ctx); }
                    let cancel = ui.button("Cancel");
                    crate::ui_probe::record(ctx, "pending-cancel", cancel.rect);
                    if cancel.clicked() { self.pending = None; }
                });
                if let Some(error) = &self.error { ui.colored_label(Color32::from_rgb(235, 157, 132), error); }
            });
            if response.should_close() {
                self.pending = None;
            }
            if self.pending.is_none() && self.dialog.is_none() && !self.allow_close {
                self.focus_editor();
            }
            return;
        }
        if self.vault_manager.open {
            let root = self.vault_manager.show(
                ctx,
                self.vault.as_ref().map(|vault| vault.root.as_path()),
                self.loading_root.is_some(),
            );
            if let Some(root) = root {
                self.request_vault(root);
            } else if !self.vault_manager.open {
                if self.loading_root.take().is_some() {
                    self.loading = None;
                    self.status = "Ready".into();
                }
                self.focus_editor();
            }
            return;
        }
        if self.file_move.active() {
            if let Some(action) = self.file_move.show(ctx) {
                match action {
                    crate::file_move_ui::Action::Move {
                        source,
                        folder,
                        create,
                    } => {
                        self.move_file(&source, &folder, create);
                    }
                    crate::file_move_ui::Action::Update { updates, always } => {
                        if always {
                            self.always_update_links = true;
                        }
                        self.apply_move_links(updates);
                        if let Err(error) = self.write_session() {
                            self.error = Some(error);
                        }
                    }
                }
            }
            if !self.file_move.active() {
                self.focus_editor();
            }
            return;
        }
        if self.workspace_manager.open {
            let layout = self.workspace_layout();
            if let Some(name) = self.workspace_manager.show(ctx, &layout) {
                self.request_workspace(name);
            } else if !self.workspace_manager.open {
                self.focus_editor();
            }
            return;
        }
        if self.recovery_manager.open {
            if let (Some(vault), Some(recorder)) = (&self.vault, &self.recovery_recorder) {
                if let Some(request) = self.recovery_manager.show(ctx, vault, recorder) {
                    if self.tabs.iter().any(|tab| {
                        tab.document.path == request.expected.path && tab.document.dirty()
                    }) {
                        self.pending = Some(Pending::RestoreSnapshot(Box::new(request)));
                    } else {
                        self.restore_snapshot(request);
                    }
                } else if !self.recovery_manager.active() {
                    self.focus_editor();
                }
            }
            return;
        }
        let Some(dialog) = self.dialog else {
            return;
        };
        let title = match dialog {
            Dialog::QuickOpen => "Open a file",
            Dialog::NewNote => "New note",
            Dialog::NewFolder => "New folder",
            Dialog::SaveCopy => "Save a copy",
            Dialog::OpenVault => "Open vault",
            Dialog::Commands => "Command palette",
            Dialog::RenameNote => "Rename or move",
            Dialog::TrashNote => "Move file to system Trash",
            Dialog::AddProperty => "Add property",
            Dialog::BookmarkTitle => "Edit bookmark title",
            Dialog::Settings => "Settings",
            Dialog::InsertTemplate => "Insert template",
        };
        let mut close = false;
        let mut submit = false;
        let submit_enter = ctx.input(|i| {
            i.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::Key {
                        key: Key::Enter,
                        pressed: true,
                        ..
                    }
                )
            })
        });
        let open_in_new_tab = ctx.input(|i| i.events.iter().any(|event| matches!(event, egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if modifiers.command)));
        let navigate_down = matches!(
            dialog,
            Dialog::QuickOpen | Dialog::Commands | Dialog::InsertTemplate
        ) && ctx
            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
        let navigate_up = matches!(
            dialog,
            Dialog::QuickOpen | Dialog::Commands | Dialog::InsertTemplate
        ) && ctx
            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
        let mut select = None;
        let mut run_command = None;
        let mut recovery_action = None;
        let mut modal = egui::Modal::new(egui::Id::unique("main-dialog"));
        if dialog == Dialog::Settings {
            modal = modal.frame(
                egui::Frame::new()
                    .fill(if self.dark {
                        Color32::from_rgb(30, 30, 30)
                    } else {
                        Color32::WHITE
                    })
                    .stroke(Stroke::new(
                        1.0,
                        Color32::from_gray(if self.dark { 60 } else { 210 }),
                    ))
                    .corner_radius(10),
            );
        }
        let response = modal.show(ctx, |ui| {
            if dialog == Dialog::Settings {
                self.settings_ui(ui, ctx, &mut close, &mut recovery_action);
                return;
            }
            ui.set_width(if matches!(dialog, Dialog::Commands | Dialog::Settings) {
                650.0
            } else {
                500.0
            });
            theme::dialog_style(ui);
            if dialog != Dialog::Commands {
                ui.heading(title);
                ui.add_space(10.0);
            }
            let hint = match dialog {
                Dialog::QuickOpen => "Find a file…",
                Dialog::NewNote | Dialog::SaveCopy => "Folder/Note.md",
                Dialog::NewFolder => "Folder/New folder",
                Dialog::OpenVault => "/path/to/your/vault",
                Dialog::Commands => "Select a command…",
                Dialog::Settings => "Search settings and shortcuts…",
                Dialog::InsertTemplate => "Search templates…",
                Dialog::BookmarkTitle => "Leave blank to use the file name",
                Dialog::RenameNote
                    if self.vault.as_ref().is_some_and(|vault| {
                        self.dialog_path
                            .as_ref()
                            .is_some_and(|path| vault.folders.contains(path))
                    }) =>
                {
                    "Folder/New name"
                }
                Dialog::RenameNote
                    if self
                        .tabs
                        .get(self.active)
                        .is_some_and(|tab| tab.document.is_image()) =>
                {
                    "Folder/New filename"
                }
                Dialog::RenameNote | Dialog::TrashNote => "Folder/Note.md",
                Dialog::AddProperty => "Property name",
            };
            let input_id = ui.make_persistent_id(("dialog-input", title));
            let focus_settled = ui.memory(|memory| {
                memory.has_focus(input_id) && memory.had_focus_last_frame(input_id)
            });
            if self.dialog_select_all && !ui.is_sizing_pass() {
                let mut state =
                    egui::text_edit::TextEditState::load(ctx, input_id).unwrap_or_default();
                let end = self.dialog_input.chars().count();
                let start = Path::new(&self.dialog_input)
                    .file_name()
                    .map_or(0, |name| end - name.to_string_lossy().chars().count());
                state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(start),
                        egui::text::CCursor::new(end),
                    )));
                state.store(ctx, input_id);
                self.dialog_select_all = false;
            }
            if self.dialog_focus && !ui.memory(|memory| memory.has_focus(input_id)) {
                ui.memory_mut(|memory| memory.request_focus(input_id));
            }
            let input = ui.add_enabled(
                dialog != Dialog::TrashNote,
                TextEdit::singleline(&mut self.dialog_input)
                    .id(input_id)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint),
            );
            if self.dialog_focus {
                // A new Area/menu may still be completing its first layout pass.
                // Claim before input, then retain focus until it survives a pass.
                if !ui.memory(|memory| memory.has_focus(input_id)) {
                    input.request_focus();
                }
                self.dialog_focus =
                    dialog != Dialog::TrashNote && (!focus_settled || ui.is_sizing_pass());
                if self.dialog_focus {
                    ctx.request_repaint();
                }
            }
            crate::ui_probe::record(ui.ctx(), "dialog-input", input.rect);
            if input.has_focus() {
                crate::ui_probe::record(ui.ctx(), "dialog-input-focused", input.rect);
            }
            let enter = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
            ui.add_space(12.0);
            if let Some(error) = &self.error {
                let label = ui.label(
                    RichText::new(error)
                        .size(13.0)
                        .color(Color32::from_rgb(235, 157, 132)),
                );
                crate::ui_probe::record(ctx, "dialog-error", label.rect);
            }
            match dialog {
                Dialog::QuickOpen => {
                    let matches = self.files.rank(&self.dialog_input, 100);
                    if input.changed() {
                        self.dialog_selected = 0;
                    }
                    if navigate_down {
                        self.dialog_selected =
                            (self.dialog_selected + 1).min(matches.len().saturating_sub(1));
                    }
                    if navigate_up {
                        self.dialog_selected = self.dialog_selected.saturating_sub(1);
                    }
                    self.dialog_selected =
                        self.dialog_selected.min(matches.len().saturating_sub(1));
                    let enter = submit_enter;
                    if enter {
                        select = matches.get(self.dialog_selected).cloned();
                    }
                    ScrollArea::vertical().max_height(330.0).show(ui, |ui| {
                        for (index, path) in matches.iter().enumerate() {
                            let row = ui.add(
                                egui::Button::new(
                                    RichText::new(rust_notes::paths::display_path(path)).size(15.0),
                                )
                                .selected(index == self.dialog_selected)
                                .frame_when_inactive(false)
                                .min_size(egui::vec2(ui.available_width(), 34.0)),
                            );
                            if crate::ui_probe::enabled() {
                                crate::ui_probe::record(
                                    ctx,
                                    format!("quick-open:{}", path.display()),
                                    row.rect,
                                );
                            }
                            if row.clicked() {
                                select = Some(path.clone());
                            }
                            if index == self.dialog_selected
                                && (enter
                                    || ui.input(|i| {
                                        i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)
                                    }))
                            {
                                row.scroll_to_me(Some(Align::Center));
                            }
                        }
                    });
                    if matches.is_empty() {
                        ui.label("No matching files");
                    }
                    ui.label(
                        RichText::new("↑ ↓ to navigate · Enter to open · Ctrl+Enter for a new tab")
                            .size(11.0)
                            .weak(),
                    );
                }
                Dialog::InsertTemplate => {
                    match &mut self.template_choices {
                        Err(error) => {
                            let label = ui.label(error.as_str());
                            crate::ui_probe::record(ctx, "template-error", label.rect);
                        }
                        Ok(matcher) => {
                            let folder = Path::new(self.note_aids.templates.folder.trim());
                            let matches = matcher.rank(&self.dialog_input, 100);
                            if input.changed() {
                                self.dialog_selected = 0;
                            }
                            if navigate_down {
                                self.dialog_selected =
                                    (self.dialog_selected + 1).min(matches.len().saturating_sub(1));
                            }
                            if navigate_up {
                                self.dialog_selected = self.dialog_selected.saturating_sub(1);
                            }
                            self.dialog_selected =
                                self.dialog_selected.min(matches.len().saturating_sub(1));
                            ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                                for (index, path) in matches.iter().enumerate() {
                                    let label = rust_notes::paths::display_path(
                                        path.strip_prefix(folder).unwrap_or(path),
                                    );
                                    let row = ui.add(
                                        egui::Button::new(label)
                                            .selected(index == self.dialog_selected)
                                            .frame_when_inactive(false)
                                            .min_size(egui::vec2(ui.available_width(), 34.0)),
                                    );
                                    crate::ui_probe::record(
                                        ctx,
                                        format!("template:{}", path.display()),
                                        row.rect,
                                    );
                                    if row.clicked()
                                        || submit_enter && index == self.dialog_selected
                                    {
                                        select = Some(path.clone());
                                    }
                                    if index == self.dialog_selected
                                        && (navigate_down || navigate_up)
                                    {
                                        row.scroll_to_me(Some(Align::Center));
                                    }
                                }
                            });
                            if matches.is_empty() {
                                ui.weak("No matching templates");
                            }
                        }
                    }
                    ui.weak("↑ ↓ navigate · Enter to insert template · Esc to dismiss");
                }
                Dialog::Commands => {
                    let tab = self.tabs.get(self.active);
                    let scope = if tab.is_none() {
                        Scope::Global
                    } else if tab
                        .is_some_and(|tab| tab.document.is_blank() || tab.view == View::Graph)
                    {
                        Scope::Tab
                    } else if tab.is_some_and(|tab| tab.document.is_image()) {
                        Scope::File
                    } else if self.can_edit_active() {
                        Scope::Editor
                    } else {
                        Scope::Document
                    };
                    let mut matches = rust_notes::commands::ranked(&self.dialog_input, scope);
                    if !self.local_graph_available() {
                        matches.retain(|command| {
                            command.name != rust_notes::commands::OPEN_LOCAL_GRAPH
                        });
                    }
                    if self.view() == View::Graph {
                        matches.retain(|command| {
                            !matches!(
                                command.name,
                                "Rename or move file"
                                    | "Move file to system Trash"
                                    | "Bookmark current file"
                            )
                        });
                    }
                    if tab.is_none_or(|tab| {
                        rust_notes::tab::other_tab_indices(&self.tabs, tab.id()).is_empty()
                    }) {
                        matches.retain(|command| {
                            command.name != rust_notes::commands::CLOSE_OTHER_TABS
                        });
                    }
                    if self.closed_tabs.is_empty() {
                        matches
                            .retain(|command| command.name != rust_notes::commands::UNDO_CLOSE_TAB);
                    }
                    if input.changed() {
                        self.dialog_selected = 0;
                    }
                    if navigate_down {
                        self.dialog_selected =
                            (self.dialog_selected + 1).min(matches.len().saturating_sub(1));
                    }
                    if navigate_up {
                        self.dialog_selected = self.dialog_selected.saturating_sub(1);
                    }
                    self.dialog_selected =
                        self.dialog_selected.min(matches.len().saturating_sub(1));
                    ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                        for (index, command) in matches.iter().enumerate() {
                            let row = ui.add(
                                egui::Button::new(command.name)
                                    .shortcut_text(command.shortcut)
                                    .selected(index == self.dialog_selected)
                                    .frame_when_inactive(false)
                                    .min_size(egui::vec2(ui.available_width(), 34.0)),
                            );
                            crate::ui_probe::record(
                                ctx,
                                format!("command:{}", command.name),
                                row.rect,
                            );
                            if row.clicked() {
                                run_command = Some(command.name);
                            }
                            if index == self.dialog_selected && (navigate_down || navigate_up) {
                                row.scroll_to_me(Some(Align::Center));
                            }
                        }
                    });
                    if matches.is_empty() {
                        ui.label("No matching commands");
                    }
                    ui.label(
                        RichText::new("↑ ↓ navigate · Enter to run · Esc to close")
                            .size(11.0)
                            .weak(),
                    );
                    if submit_enter {
                        run_command = matches
                            .get(self.dialog_selected)
                            .map(|command| command.name);
                    }
                }
                _ => {
                    if dialog == Dialog::TrashNote {
                        ui.label("The file will be recoverable from your desktop’s Trash.");
                    }
                    if dialog == Dialog::RenameNote {
                        ui.label("Links will be updated. Enter a new path inside this vault.");
                    }
                    if dialog == Dialog::OpenVault {
                        ui.label(
                            RichText::new("Opening a vault does not modify its files.")
                                .small()
                                .weak(),
                        );
                    }
                    submit = enter;
                    ui.horizontal(|ui| {
                        submit |= ui
                            .button(match dialog {
                                Dialog::OpenVault => "Open folder",
                                Dialog::NewFolder => "Create folder",
                                Dialog::RenameNote => "Rename",
                                Dialog::TrashNote => "Move to Trash",
                                Dialog::AddProperty => "Add property",
                                Dialog::SaveCopy => "Save copy",
                                Dialog::BookmarkTitle => "Save title",
                                _ => "Create note",
                            })
                            .clicked();
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                }
            }
        });
        if response.should_close() {
            close = true;
        }
        if let Some(clear) = recovery_action {
            if clear {
                self.recovery_manager.request_clear();
            } else {
                self.open_recovery(false);
                return;
            }
        }
        if self.recovery_manager.active()
            && !self.recovery_manager.open
            && let (Some(vault), Some(recorder)) = (&self.vault, &self.recovery_recorder)
        {
            let _ = self.recovery_manager.show(ctx, vault, recorder);
            if let Some(error) = self.recovery_manager.take_error() {
                self.error = Some(error);
            }
            if let Some(notice) = self.recovery_manager.take_notice() {
                self.status = notice;
            }
        }
        if let Some(command) = run_command {
            self.dialog = None;
            self.command(command, ctx);
            return;
        }
        if let Some(path) = select {
            if dialog == Dialog::InsertTemplate {
                if !self.insert_template(&path) {
                    return;
                }
            } else {
                self.open_note_at(&path, open_in_new_tab || ctx.input(|i| i.modifiers.command));
            }
            close = true;
        }
        if submit {
            close = self.submit_dialog(dialog);
        }
        if close {
            self.dialog = None;
            self.template_source = None;
            if matches!(dialog, Dialog::RenameNote | Dialog::TrashNote) {
                if !submit && self.tabs.iter().all(|tab| !tab.document.dirty()) {
                    self.error = None;
                }
                self.focus_editor();
            }
            if dialog == Dialog::InsertTemplate {
                self.focus_editor();
            }
        }
    }

    fn relocate_tabs(&mut self, source: &Path, destination: &Path) {
        self.page_preview.close();
        if let Some(vault) = &self.vault {
            crate::editor_ui::relocate_histories(&self.ctx, &vault.root, source, destination);
        }
        self.bookmarks.rename(source, destination);
        for closed in &mut self.closed_tabs {
            closed.rename_path(source, destination);
        }
        for tab in &mut self.tabs {
            tab.history.rename_path(source, destination);
            tab.document.path =
                rust_notes::paths::moved_path(&tab.document.path, source, destination, true);
            if let Some(vault) = &self.vault
                && !tab.document.is_blank()
                && let Ok(document) = vault.open_document(&tab.document.path)
            {
                tab.replace(document);
            }
        }
    }

    fn refresh_file_views(&mut self) {
        if let Some(vault) = &self.vault {
            self.tree = Tree::from_vault(vault);
            self.files = rust_notes::search::FileMatcher::from_vault(vault);
        }
        self.search_cache.invalidate();
        self.graph.invalidate();
        self.cache = CommonMarkCache::default();
        self.ctx.request_repaint();
    }

    fn show_page_preview(&mut self, ctx: &egui::Context) {
        let blocked = self.interaction_blocked() || self.view() == View::Graph;
        match self.page_preview.show(
            ctx,
            self.vault.as_ref(),
            &self.tabs,
            self.active,
            blocked,
            self.document_font_size,
        ) {
            Some(crate::page_preview::Action::Open { from, target }) => {
                self.follow_link_from(&from, &target)
            }
            Some(crate::page_preview::Action::Edit { before, mut after }) => {
                let indices: Vec<_> = self
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(_, tab)| tab.document.path == before.path)
                    .map(|(index, _)| index)
                    .collect();
                if indices
                    .iter()
                    .any(|&index| self.tabs[index].document.text != before.text)
                {
                    self.error = Some("The target changed while its preview was open. Reopen the preview before editing it.".into());
                    self.page_preview.close();
                    return;
                }
                if !indices.is_empty()
                    && let Some(vault) = &self.vault
                {
                    crate::editor_ui::record_document_edit(ctx, &vault.root, &before, &after);
                }
                for &index in &indices {
                    self.tabs[index].document.text.clone_from(&after.text);
                    self.tabs[index].edited();
                }
                if let Some(&index) = indices.first() {
                    if self.autosave_enabled && self.save(index) {
                        let saved = self.tabs[index].document.clone();
                        for &other in &indices[1..] {
                            self.tabs[other].replace(saved.clone());
                        }
                    }
                } else if let Some(vault) = &mut self.vault {
                    match vault.save(&mut after) {
                        Ok(()) => self.refresh_file_views(),
                        Err(error) => {
                            let mut tab = Tab::new(after);
                            tab.edited();
                            tab.autosave_failed = true;
                            self.tabs.push(tab);
                            self.error = Some(error);
                            self.page_preview.close();
                        }
                    }
                }
            }
            None => {}
        }
    }

    fn ask_move_file(&mut self, source: PathBuf) {
        if let Some(vault) = &self.vault {
            self.file_move.open(vault, source);
            self.editor.focus = false;
        }
    }

    fn apply_move_links(&mut self, updates: rust_notes::operations::LinkUpdates) {
        let Some(vault) = &mut self.vault else {
            return;
        };
        let outcome = vault.finish_link_updates(updates);
        // The file is already moved; only reload clean documents whose links changed.
        for tab in &mut self.tabs {
            if !tab.document.is_blank()
                && !tab.document.dirty()
                && let Ok(document) = vault.open_document(&tab.document.path)
            {
                tab.replace(document);
            }
        }
        self.error = (!outcome.warnings.is_empty()).then(|| {
            format!(
                "File moved. Some links need review: {}",
                outcome.warnings.join("; ")
            )
        });
        self.status = format!(
            "Moved to {} · updated {} files",
            outcome.path.display(),
            outcome.updated_notes
        );
        self.refresh_file_views();
    }

    fn move_file(&mut self, source: &Path, folder: &Path, create: bool) {
        if !self.save_all() {
            self.file_move.error(
                self.error
                    .clone()
                    .unwrap_or_else(|| "Save your changes before moving the file.".into()),
            );
            return;
        }
        let result = self
            .vault
            .as_mut()
            .ok_or_else(|| "Open a vault first.".to_owned())
            .and_then(|vault| vault.begin_move_file(source, folder, create));
        match result {
            Ok(Some(updates)) => {
                self.relocate_tabs(source, updates.path());
                self.reveal_folder = Some(folder.to_owned());
                self.status = format!("Moved to {}", updates.path().display());
                self.error = None;
                let automatic = self.always_update_links || updates.counts().0 == 0;
                self.file_move.moved(None);
                if automatic {
                    self.apply_move_links(updates);
                } else {
                    self.file_move.moved(Some(updates));
                }
                self.refresh_file_views();
            }
            Ok(None) => self.file_move.moved(None),
            Err(error) => self.file_move.error(error),
        }
        if let Err(error) = self.write_session() {
            self.error = Some(error);
        }
    }

    fn submit_dialog(&mut self, dialog: Dialog) -> bool {
        let input = self.dialog_input.trim().to_owned();
        if dialog == Dialog::BookmarkTitle {
            let Some(path) = self.dialog_path.clone() else {
                return false;
            };
            let before = self.bookmarks.clone();
            self.bookmarks.set_title(&path, &input);
            return self.save_bookmarks(before);
        }
        if dialog == Dialog::NewFolder {
            let result = self
                .vault
                .as_mut()
                .ok_or("Open a vault first.".to_owned())
                .and_then(|vault| vault.create_folder(Path::new(&input)));
            return match result {
                Ok(()) => {
                    self.tree = Tree::from_vault(self.vault.as_ref().unwrap());
                    self.reveal_folder = Some(PathBuf::from(&input));
                    self.searching = false;
                    self.show_bookmarks = false;
                    self.status = format!("Created folder {input}");
                    self.error = None;
                    true
                }
                Err(error) => {
                    self.error = Some(error);
                    false
                }
            };
        }
        if dialog == Dialog::AddProperty {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return false;
            };
            let result =
                rust_notes::metadata::frontmatter(&tab.document.text).and_then(|existing| {
                    if existing.is_some_and(|properties| {
                        properties
                            .values
                            .contains_key(serde_yaml_ng::Value::String(input.clone()))
                    }) {
                        return Err("This property already exists.".into());
                    }
                    let value = if matches!(input.as_str(), "aliases" | "tags" | "cssclasses") {
                        serde_yaml_ng::Value::Sequence(Vec::new())
                    } else {
                        serde_yaml_ng::Value::String(String::new())
                    };
                    rust_notes::metadata::set_property(&mut tab.document.text, &input, value)
                });
            return match result {
                Ok(()) => {
                    tab.edited();
                    tab.set_view(View::Live);
                    self.error = None;
                    true
                }
                Err(error) => {
                    self.error = Some(error);
                    false
                }
            };
        }
        if matches!(dialog, Dialog::RenameNote | Dialog::TrashNote) {
            let Some(source) = self.dialog_path.clone() else {
                self.error = Some("Choose a file first.".into());
                return false;
            };
            if !self.save_all() {
                return false;
            }
            let result = if dialog == Dialog::RenameNote {
                let mut target = PathBuf::from(&input);
                if target.extension().is_none()
                    && !self
                        .vault
                        .as_ref()
                        .is_some_and(|vault| vault.folders.contains(&source))
                {
                    target.set_extension(
                        source
                            .extension()
                            .unwrap_or_else(|| std::ffi::OsStr::new("md")),
                    );
                }
                self.vault
                    .as_mut()
                    .ok_or_else(|| "Open a vault first.".to_owned())
                    .and_then(|vault| {
                        let plan = vault.plan_rename(&source, &target)?;
                        vault.rename_note(plan).map(Some)
                    })
            } else {
                self.vault
                    .as_mut()
                    .ok_or_else(|| "Open a vault first.".to_owned())
                    .and_then(|vault| vault.trash_note(&source).map(|()| None))
            };
            match result {
                Ok(Some(outcome)) => {
                    self.relocate_tabs(&source, &outcome.path);
                    self.status = format!(
                        "Renamed to {} · updated {} notes",
                        outcome.path.display(),
                        outcome.updated_notes
                    );
                    self.reveal_folder = self
                        .vault
                        .as_ref()
                        .is_some_and(|vault| vault.folders.contains(&outcome.path))
                        .then(|| outcome.path.clone());
                    self.error = if outcome.warnings.is_empty() {
                        None
                    } else {
                        Some(format!(
                            "File renamed. Some links need review: {}",
                            outcome.warnings.join("; ")
                        ))
                    };
                }
                Ok(None) => {
                    let active_id = self.tabs.get(self.active).map(Tab::id);
                    let deleted_active = self
                        .tabs
                        .get(self.active)
                        .is_some_and(|tab| tab.document.path == source);
                    self.tabs.retain(|tab| tab.document.path != source);
                    if deleted_active {
                        self.new_tab();
                    } else {
                        self.active = self
                            .tabs
                            .iter()
                            .position(|tab| Some(tab.id()) == active_id)
                            .unwrap_or(0);
                    }
                    self.status = format!("Moved {} to system Trash", source.display());
                    self.error = None;
                }
                Err(error) => {
                    self.error = Some(error);
                    return false;
                }
            }
            if let Some(vault) = &self.vault {
                self.tree = Tree::from_vault(vault);
                self.files = rust_notes::search::FileMatcher::from_vault(vault);
                self.search_cache.invalidate();
            }
            self.cache = CommonMarkCache::default();
            if let Err(error) = self.write_session() {
                self.error = Some(error);
            }
            self.ctx.request_repaint();
            return true;
        }
        if input.is_empty() {
            self.error = Some("Enter a name or folder path first.".into());
            return false;
        }
        if dialog == Dialog::OpenVault {
            match rust_notes::vaults::folder(Path::new(&input)) {
                Ok(root) => {
                    self.load_vault(&root);
                    return true;
                }
                Err(error) => {
                    self.error = Some(error);
                    return false;
                }
            }
        }
        let mut path = PathBuf::from(input);
        if path.extension().is_none() {
            path.set_extension("md");
        }
        if self.tabs.iter().any(|tab| tab.document.path == path) {
            self.error = Some("This note is already open. Choose another name.".into());
            return false;
        }
        let mut document = Document::new(path);
        document.text = if dialog == Dialog::SaveCopy {
            self.tabs
                .get(self.active)
                .map(|tab| tab.document.text.clone())
                .unwrap_or_default()
        } else {
            format!("# {}\n\n", document.title())
        };
        let Some(vault) = &mut self.vault else {
            self.error = Some("Open a vault before creating a note.".into());
            return false;
        };
        // A successful create validates containment and prevents name collisions.
        match vault.save(&mut document) {
            Ok(()) => {
                self.tree = Tree::from_vault(vault);
                self.files = rust_notes::search::FileMatcher::from_vault(vault);
                self.search_cache.invalidate();
                self.error = None;
                self.status = format!("Created {}", document.path.display());
                self.tabs.push(Tab::new(document));
                self.active = self.tabs.len() - 1;
                self.set_view(View::Live);
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    fn command(&mut self, command: &str, ctx: &egui::Context) {
        if let Some(color) = rust_notes::highlight::NativeColor::ALL
            .into_iter()
            .find(|color| command == color.command())
        {
            if let Some(tab) = self.tabs.get(self.active).filter(|tab| {
                !tab.document.is_blank()
                    && !tab.document.is_image()
                    && !matches!(tab.view, View::Read | View::Graph)
            }) {
                self.editor.native_highlight(tab, color);
                ctx.request_repaint();
            } else {
                self.error = Some("Choose an editable Markdown note to highlight text.".into());
            }
            return;
        }
        let highlight = rust_notes::highlight::Color::ALL
            .into_iter()
            .find(|color| color.command() == command)
            .or_else(|| {
                (command == "Highlight selection").then_some(rust_notes::highlight::Color::Yellow)
            });
        if let Some(color) = highlight {
            if let Some(tab) = self.tabs.get(self.active).filter(|tab| {
                !tab.document.is_image() && !matches!(tab.view, View::Read | View::Graph)
            }) {
                self.editor.highlight(tab, color);
                ctx.request_repaint();
            } else {
                self.error = Some("Choose an editable Markdown note to highlight text.".into());
            }
            return;
        }
        match command {
            rust_notes::commands::OPEN_GRAPH => self.open_global_graph(),
            rust_notes::commands::OPEN_LOCAL_GRAPH if self.local_graph_available() => {
                self.graph.local_open = true;
                self.ctx.request_repaint();
            }
            rust_notes::commands::MOVE_FILE
            | rust_notes::commands::DUPLICATE_FILE
            | "Rename or move file"
            | "Move file to system Trash"
            | "Bookmark current file"
                if self.view() == View::Graph => {}
            rust_notes::commands::MOVE_FILE => {
                if let Some(path) = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.path.clone())
                {
                    self.ask_move_file(path);
                }
            }
            rust_notes::commands::DUPLICATE_FILE => {
                if let Some(path) = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.path.clone())
                {
                    self.duplicate_file(&path);
                }
            }
            "Toggle pin" => self.toggle_pin(self.active),
            rust_notes::commands::UNDO_CLOSE_TAB => self.undo_close_tab(),
            rust_notes::commands::CLOSE_OTHER_TABS => self.request_close_other_tabs(),
            rust_notes::commands::INCREASE_FONT_SIZE => {
                self.set_document_font_size(self.document_font_size.saturating_add(1))
            }
            rust_notes::commands::DECREASE_FONT_SIZE => {
                self.set_document_font_size(self.document_font_size.saturating_sub(1))
            }
            "New tab" => self.new_tab(),
            "Close current tab" if !self.tabs.is_empty() => self.request_close_tab(self.active),
            "Go to next tab" => self.step_tab(false),
            "Go to previous tab" => self.step_tab(true),
            "File recovery: Open local history" => self.open_recovery(true),
            "Workspaces: Manage workspace layouts" => self.show_workspaces(false),
            "Workspaces: Load workspace layout" => self.show_workspaces(true),
            "Workspaces: Save layout" | "Workspaces: Save and load another layout" => {
                match self.save_named_workspace() {
                    Ok(true) => {
                        if command == "Workspaces: Save and load another layout" {
                            self.show_workspaces(true);
                        }
                    }
                    Ok(false) => self.show_workspaces(false),
                    Err(error) => self.error = Some(error),
                }
            }
            "Daily notes: Open today's daily note" => self.open_daily(),
            "Unique note creator: Create new unique note" => {
                self.open_created_note(rust_notes::note_aids::Config::create_unique);
            }
            rust_notes::commands::ADD_UNIQUE_LINK if self.editable_note() => {
                if let Some(vault) = &self.vault {
                    match self
                        .note_aids
                        .unique_link(vault, chrono::Local::now().fixed_offset())
                    {
                        Ok(text) => self.editor.insert_wiki_link(&self.tabs[self.active], text),
                        Err(error) => self.error = Some(error),
                    }
                }
            }
            "Random note: Open random note" => {
                if let Some(path) = self
                    .vault
                    .as_ref()
                    .and_then(rust_notes::note_aids::random_note)
                {
                    self.open_note(&path);
                } else {
                    self.status = "No Markdown notes in this vault.".into();
                }
            }
            "Templates: Insert template" if self.editable_note() => {
                self.ask(Dialog::InsertTemplate);
            }
            "Templates: Insert current date" | "Templates: Insert current time"
                if self.editable_note() =>
            {
                let variable = if command.ends_with("date") {
                    "{{date}}"
                } else {
                    "{{time}}"
                };
                let tab = &self.tabs[self.active];
                let text = self.note_aids.expand(
                    variable,
                    &tab.document.title(),
                    chrono::Local::now().fixed_offset(),
                );
                self.editor.insert_text(tab, text);
            }
            "Open settings" => self.ask(Dialog::Settings),
            "New note" => self.ask(Dialog::NewNote),
            "New folder" => self.ask(Dialog::NewFolder),
            "Add property" => self.ask(Dialog::AddProperty),
            "Rename or move file" => {
                if let Some(path) = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.path.clone())
                {
                    self.ask_file(Dialog::RenameNote, path);
                }
            }
            "Move file to system Trash" => {
                if let Some(path) = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.path.clone())
                {
                    self.ask_file(Dialog::TrashNote, path);
                }
            }
            rust_notes::commands::OPEN_FILE => {
                self.start_open_picker(rust_notes::native_picker::Kind::File)
            }
            rust_notes::commands::OPEN_FOLDER => {
                self.start_open_picker(rust_notes::native_picker::Kind::Folder)
            }
            "Open a file" => self.ask(Dialog::QuickOpen),
            "Open vault" => self.change_vault(),
            "Open another vault" => self.manage_vaults(),
            "Create new vault" => {
                self.vault_manager
                    .show_create(self.vault.as_ref().map(|vault| vault.root.as_path()));
            }
            "Bookmark current file" => {
                if let Some(path) = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.path.clone())
                {
                    self.add_bookmark(&path);
                }
            }
            "Show bookmarks" => {
                self.show_left = true;
                self.show_bookmarks = true;
            }
            "Save note" => {
                self.save(self.active);
            }
            "Save a copy" => self.ask(Dialog::SaveCopy),
            "Live preview" => self.set_view(View::Live),
            "Reading view" => self.set_view(View::Read),
            "Source editing" => self.set_view(View::Source),
            "Split source and preview" => self.set_view(View::Split),
            "Toggle left sidebar" => self.show_left = !self.show_left,
            "Toggle right sidebar" => self.show_right = !self.show_right,
            "Switch appearance" => {
                self.dark = !self.dark;
                theme::apply(ctx, self.dark);
            }
            "Refresh vault" => {
                if let Some(vault) = &self.vault {
                    self.queue_index(vault.root.clone());
                }
            }
            _ => {}
        }
    }

    fn duplicate_file(&mut self, path: &Path) {
        if let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.document.path == path && tab.document.dirty())
            && !self.save(index)
        {
            return;
        }
        let Some(vault) = &mut self.vault else { return };
        match vault.duplicate_file(path) {
            Ok(copy) => {
                self.tree = Tree::from_vault(vault);
                self.files = rust_notes::search::FileMatcher::from_vault(vault);
                self.search_cache.invalidate();
                self.error = None;
                self.open_note_at(&copy, true);
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn editable_note(&mut self) -> bool {
        if self.can_edit_active() {
            true
        } else {
            self.error = Some("Open a note in an editing view first.".into());
            false
        }
    }

    fn can_edit_active(&self) -> bool {
        self.tabs.get(self.active).is_some_and(|tab| {
            !tab.document.is_blank()
                && !tab.document.is_image()
                && !matches!(tab.view, View::Read | View::Graph)
        })
    }

    fn open_daily(&mut self) {
        self.open_created_note(rust_notes::note_aids::Config::create_daily);
    }

    fn open_created_note(
        &mut self,
        create: impl FnOnce(
            &rust_notes::note_aids::Config,
            &mut Vault,
            chrono::DateTime<chrono::FixedOffset>,
        ) -> rust_notes::vault::Result<PathBuf>,
    ) {
        if self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.document.dirty())
            && !self.save(self.active)
        {
            return;
        }
        let Some(vault) = &mut self.vault else {
            return;
        };
        match create(&self.note_aids, vault, chrono::Local::now().fixed_offset()) {
            Ok(path) => {
                self.tree = Tree::from_vault(vault);
                self.files = rust_notes::search::FileMatcher::from_vault(vault);
                self.search_cache.invalidate();
                self.error = None;
                self.open_note(&path);
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn insert_template(&mut self, path: &Path) -> bool {
        if !self.editable_note() {
            return false;
        }
        let tab = &self.tabs[self.active];
        if !self
            .template_source
            .as_ref()
            .is_some_and(|(id, source)| *id == tab.id() && source == &tab.document.text)
        {
            self.error = Some("This note changed while the template chooser was open. Close and reopen the chooser to choose an insertion point.".into());
            return false;
        }
        let Some(vault) = &self.vault else {
            return false;
        };
        match self.note_aids.render_template(
            vault,
            path,
            &tab.document.title(),
            chrono::Local::now().fixed_offset(),
        ) {
            Ok(text) => {
                self.editor.insert_text(tab, text);
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }
}

impl eframe::App for NotesApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Event modifiers retain the gesture even if Control was released in
        // this frame. The editor consumes Ctrl+Enter before following its link.
        self.new_tab_gesture = ctx.input(|input| {
            input.events.iter().any(|event| match event {
                egui::Event::Key {
                    key: Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } => modifiers.command,
                egui::Event::PointerButton {
                    button,
                    pressed: false,
                    modifiers,
                    ..
                } => modifiers.command || *button == egui::PointerButton::Middle,
                _ => false,
            })
        });
        self.background(&ctx);
        self.shortcuts(&ctx);
        let title = format!(
            "{} — {} — {}",
            self.tabs
                .get(self.active)
                .map(Tab::title)
                .unwrap_or("New tab".into()),
            self.vault.as_ref().map(Vault::name).unwrap_or_default(),
            rust_notes::APP_NAME
        );
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        let file_actions_enabled = self.dialog.is_none()
            && self.pending.is_none()
            && self.loading_root.is_none()
            && !self.open_picker.active()
            && !self.vault_manager.open
            && !self.workspace_manager.open
            && !self.recovery_manager.active()
            && !self.file_move.active();
        if let Some(kind) = crate::window::title_bar(ui, &self.window_title, file_actions_enabled) {
            self.start_open_picker(kind);
        }
        egui::Panel::bottom("status")
            .exact_size(28.0)
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new(&self.status).size(11.0).weak());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        if self
                            .tabs
                            .get(self.active)
                            .is_some_and(|tab| tab.document.is_image() && tab.view != View::Graph)
                        {
                            ui.label(RichText::new("Image").size(11.0).weak());
                            return;
                        }
                        let mode = match self.view() {
                            View::Read => "Reading",
                            View::Live => "Live preview",
                            View::Source => "Source mode",
                            View::Split => "Split view",
                            View::Graph => "Graph",
                        };
                        if self.editor_preferences.show_mode {
                            let response = ui.add(egui::Label::new(
                                RichText::new(mode).size(11.0).color(theme::accent(ui)),
                            ));
                            if crate::ui_probe::enabled() {
                                crate::ui_probe::record(
                                    &ctx,
                                    format!("view-mode:{mode}"),
                                    response.rect,
                                );
                            }
                        }
                        if let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document)
                            && self.view() != View::Graph
                        {
                            ui.label(
                                RichText::new(format!(
                                    "{} words   {} characters",
                                    document.text.split_whitespace().count(),
                                    document.text.chars().count()
                                ))
                                .size(11.0)
                                .weak(),
                            );
                            if document.dirty() {
                                ui.label(
                                    RichText::new("Unsaved").size(11.0).color(theme::accent(ui)),
                                );
                            }
                        }
                    });
                });
            });
        if self.dialog.is_none()
            && let Some(error) = self.error.clone()
        {
            egui::Panel::top("error").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    let message = ui.colored_label(
                        Color32::from_rgb(235, 157, 132),
                        RichText::new(error).size(13.0),
                    );
                    crate::ui_probe::record(ui.ctx(), "error-notice", message.rect);
                    let dismiss = ui.button("Dismiss");
                    crate::ui_probe::record(ui.ctx(), "error-dismiss", dismiss.rect);
                    if dismiss.clicked() {
                        self.error = None;
                    }
                    if self
                        .tabs
                        .get(self.active)
                        .is_some_and(|tab| tab.document.dirty())
                    {
                        let copy = ui.button("Save a copy…");
                        crate::ui_probe::record(ui.ctx(), "error-save-copy", copy.rect);
                        if copy.clicked() {
                            self.ask(Dialog::SaveCopy);
                        }
                    }
                });
            });
        }
        self.ribbon(ui);
        self.sidebar(ui);
        self.details(ui);
        self.content(ui);
        self.show_page_preview(&ctx);
        self.dialogs(&ctx);
        if let Some(error) = self.editor.error.take() {
            self.error = Some(error);
        }
        // Apply this frame's text events before saving. Ctrl+S can arrive in
        // the same frame as the final characters of a fast typing burst.
        if self.pending.is_none()
            && !self.open_picker.active()
            && self.loading_root.is_none()
            && !self.vault_manager.open
            && !self.workspace_manager.open
            && !self.recovery_manager.active()
            && !self.file_move.active()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, Key::S))
        {
            self.save(self.active);
        }
        crate::ui_probe::flush(&ctx);
        if self.first_frame {
            eprintln!(
                "first_ui_frame_prepared_us={}",
                self.started.elapsed().as_micros()
            );
            self.first_frame = false;
        }
    }
}
