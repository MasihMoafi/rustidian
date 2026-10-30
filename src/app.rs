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
use rustidian::commands::Scope;
use rustidian::tab::{ClosedTab, Tab, View};
use rustidian::vault::{Document, Vault, wiki_links};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, PartialEq)]
enum Dialog {
    QuickOpen,
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
    CloseTabsToRight(u64),
    Quit,
    /// Ctrl+Q's "Quit Rustidian?" question, asked when nothing is unsaved.
    ConfirmQuit,
    ChangeVault,
    OpenVault(PathBuf, Option<PathBuf>),
    LoadWorkspace(String),
    RestoreSnapshot(Box<crate::recovery_ui::Restore>),
}

struct DraggedTab(u64);

struct PreparedVault {
    vault: Vault,
    tree: Tree,
    files: rustidian::search::FileMatcher,
    /// A new watcher, or `None` when the current one was updated in place.
    monitoring: Option<Monitoring>,
    /// Folders gained watches after the scan.
    watched: bool,
    watch_warning: Option<String>,
}

const ATTENTION_DELAY: Duration = Duration::from_millis(1500);

type SharedWatch = Arc<Mutex<rustidian::watcher::Watch>>;

struct Monitoring {
    watcher: Option<SharedWatch>,
    events: Receiver<notify::Result<notify::Event>>,
    overflow: Arc<AtomicBool>,
}

struct Loading {
    receiver: Receiver<Result<PreparedVault, String>>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for Loading {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

pub struct NotesApp {
    pub(crate) vault: Option<Vault>,
    tree: Tree,
    pub(crate) tabs: Vec<Tab>,
    closed_tabs: Vec<ClosedTab>,
    pub(crate) active: usize,
    search: String,
    search_cache: rustidian::query::Cache,
    searching: bool,
    search_focus: bool,
    pub(crate) show_left: bool,
    pub(crate) show_right: bool,
    pub(crate) dark: bool,
    agent: crate::terminal_ui::Panel,
    page_pan: crate::page_pan::PagePan,
    pub(crate) appearance: theme::Appearance,
    system_theme: theme::SystemTheme,
    pub(crate) palette: theme::palette::Palette,
    pub(crate) confirm_trash: bool,
    dialog_skip_trash: bool,
    cache: CommonMarkCache,
    dialog: Option<Dialog>,
    dialog_input: String,
    dialog_path: Option<PathBuf>,
    dialog_focus: bool,
    dialog_selected: usize,
    settings_page: settings_ui::Page,
    files: rustidian::search::FileMatcher,
    pending: Option<Pending>,
    error: Option<String>,
    status: String,
    allow_close: bool,
    started: Instant,
    first_frame: bool,
    window_title: String,
    pub(crate) ctx: egui::Context,
    loading: Option<Loading>,
    /// Files and folders from later launches; see `instance`.
    #[cfg(unix)]
    pub(crate) launches: Option<Receiver<crate::instance::Request>>,
    /// Launches that wait to hear this window has the focus; see
    /// `instance::Instance::serve`.
    #[cfg(unix)]
    showing: Vec<mpsc::Sender<()>>,
    /// When to check that a window just opened has focus. GNOME may leave it
    /// behind the window being typed in; it then asks for attention, so
    /// "Rustidian is ready" appears instead of nothing.
    attention_check: Option<Instant>,
    watcher: Option<SharedWatch>,
    watch_events: Option<Receiver<notify::Result<notify::Event>>>,
    watch_overflow: Arc<AtomicBool>,
    refresh_due: Option<Instant>,
    /// Files the watcher reported since the last refresh. Rustidian's own
    /// saves are among them; the refresh re-indexes only if one holds
    /// something the vault does not already have.
    refresh_paths: std::collections::HashSet<PathBuf>,
    /// Events were lost or folders gained watches: the refresh must re-index.
    refresh_required: bool,
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
    /// The active note the explorer last scrolled to; a new one is revealed once.
    revealed_note: Option<PathBuf>,
    reset_tree_scroll: bool,
    pub(crate) bookmarks: rustidian::bookmarks::Bookmarks,
    pub(crate) show_bookmarks: bool,
    pub(crate) right_tab: u8,
    pub(crate) ui_zoom: f32,
    pub(crate) document_font_size: u8,
    pub(crate) fonts: theme::fonts::Fonts,
    /// Chosen font names that are not installed, shown under the font settings.
    pub(crate) missing_fonts: Vec<String>,
    pub(crate) editor_preferences: settings_ui::EditorPreferences,
    pub(crate) autosave_enabled: bool,
    autosave: rustidian::autosave::Writer,
    vault_manager: crate::vault_manager::Manager,
    open_picker: rustidian::native_picker::Picker,
    reveal_result: Option<Receiver<Result<(), String>>>,
    pub(crate) note_aids: rustidian::note_aids::Config,
    template_source: Option<(u64, String)>,
    template_choices: Result<rustidian::search::FileMatcher, String>,
    workspace_manager: crate::workspace_ui::Manager,
    pub(crate) sidebar_widths: [f32; 2],
    pub(crate) restore_sidebars: [bool; 2],
    pub(crate) recovery_config: rustidian::recovery::Config,
    recovery_history: Option<rustidian::recovery::History>,
    recovery_recorder: Option<rustidian::recovery_capture::Recorder>,
    recovery_revisions: std::collections::HashMap<PathBuf, u64>,
    recovery_changed_paths: std::collections::HashSet<PathBuf>,
    recovery_index_paths: std::collections::HashSet<PathBuf>,
    recovery_saved: std::collections::HashMap<PathBuf, String>,
    recovery_manager: crate::recovery_ui::Manager,
    file_move: crate::file_move_ui::Manager,
    page_preview: crate::page_preview::Manager,
    pub(crate) graph: crate::graph_ui::Manager,
    pub(crate) always_update_links: bool,
}

impl NotesApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        root: &Path,
        started: Instant,
        startup_document: Option<PathBuf>,
    ) -> Self {
        theme::apply_appearance(&cc.egui_ctx, theme::Appearance::System);
        crate::window::disable_builtin_quit(&cc.egui_ctx);
        theme::install_fonts(&cc.egui_ctx);
        theme::install_icons(&cc.egui_ctx);
        crate::syntax::prepare();
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
            dark: cc.egui_ctx.theme() == egui::Theme::Dark,
            agent: Default::default(),
            page_pan: Default::default(),
            appearance: Default::default(),
            system_theme: Default::default(),
            palette: Default::default(),
            cache: CommonMarkCache::default(),
            dialog: None,
            dialog_input: String::new(),
            confirm_trash: true,
            dialog_skip_trash: false,
            dialog_path: None,
            dialog_focus: false,
            dialog_selected: 0,
            settings_page: Default::default(),
            files: rustidian::search::FileMatcher::new(Vec::new()),
            pending: None,
            error: None,
            status: "Ready".into(),
            allow_close: false,
            started,
            first_frame: true,
            window_title: String::new(),
            ctx: cc.egui_ctx.clone(),
            loading: None,
            #[cfg(unix)]
            launches: None,
            #[cfg(unix)]
            showing: Vec::new(),
            attention_check: Some(Instant::now() + ATTENTION_DELAY),
            watcher: None,
            watch_events: None,
            watch_overflow: Arc::default(),
            refresh_due: None,
            refresh_paths: Default::default(),
            refresh_required: false,
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
            revealed_note: None,
            reset_tree_scroll: false,
            bookmarks: Default::default(),
            show_bookmarks: false,
            right_tab: 0,
            ui_zoom: 1.0,
            document_font_size: theme::DEFAULT_DOCUMENT_FONT_SIZE,
            fonts: Default::default(),
            missing_fonts: Vec::new(),
            editor_preferences: Default::default(),
            autosave_enabled: true,
            autosave: Default::default(),
            vault_manager: Default::default(),
            open_picker: Default::default(),
            reveal_result: None,
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
        self.settle_saves();
        self.agent = Default::default();
        self.page_pan = Default::default();
        self.persist_session();
        self.loading_root = Some(root.to_owned());
        self.queue_index(root.to_owned());
        self.status = "Opening vault…".into();
    }

    fn queue_index(&mut self, root: PathBuf) {
        self.recovery_index_paths
            .extend(self.recovery_changed_paths.drain());
        let (sender, receiver) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        self.loading = Some(Loading {
            receiver,
            cancelled: cancelled.clone(),
        });
        let ctx = self.ctx.clone();
        let document = self
            .loading_root
            .is_some()
            .then(|| self.startup_document.clone())
            .flatten();
        let current_watch = self
            .loading_root
            .is_none()
            .then(|| self.watcher.clone())
            .flatten()
            .filter(|_| self.vault.as_ref().is_some_and(|vault| vault.root == root));
        let spawned = std::thread::Builder::new()
            .name("rustidian-index".into())
            .spawn(move || {
                let prepare = || -> Result<_, String> {
                    let vault =
                        Vault::open_cancellable(&root, || cancelled.load(Ordering::Relaxed))?;
                    if let Some(path) = &document {
                        vault.open_document(path)?;
                    }
                    let tree = Tree::from_vault(&vault);
                    let files = rustidian::search::FileMatcher::from_vault(&vault);
                    let update = |watch: &mut rustidian::watcher::Watch| {
                        watch.update(&vault.folders, || cancelled.load(Ordering::Relaxed))
                    };
                    if let Some(watch) = current_watch {
                        let update =
                            update(&mut watch.lock().unwrap_or_else(PoisonError::into_inner));
                        return Ok(PreparedVault {
                            watched: matches!(update, Ok(true)),
                            watch_warning: update
                                .err()
                                .map(|error| rustidian::watcher::describe(&error)),
                            vault,
                            tree,
                            files,
                            monitoring: None,
                        });
                    }
                    let (events_sender, events) = mpsc::sync_channel(256);
                    let overflow = Arc::new(AtomicBool::new(false));
                    let handler_overflow = overflow.clone();
                    let handler_ctx = ctx.clone();
                    let watch_root = vault.root.clone();
                    let watcher = rustidian::watcher::Watch::new(
                        &vault.root,
                        move |event: notify::Result<notify::Event>| {
                            if let Ok(event) = &event {
                                if !(event.kind.is_modify()
                                    || event.kind.is_create()
                                    || event.kind.is_remove())
                                    || !event.paths.iter().any(|path| {
                                        path.strip_prefix(&watch_root).is_ok_and(|relative| {
                                            !rustidian::vault::is_ignored_path(relative)
                                        })
                                    })
                                {
                                    return;
                                }
                            }
                            if matches!(
                                events_sender.try_send(event),
                                Err(mpsc::TrySendError::Full(_))
                            ) {
                                handler_overflow.store(true, Ordering::Relaxed);
                            }
                            handler_ctx.request_repaint();
                        },
                    );
                    let (watcher, update) = match watcher {
                        Ok(mut watch) => {
                            let update = update(&mut watch);
                            (Some(Arc::new(Mutex::new(watch))), update)
                        }
                        Err(error) => (None, Err(error)),
                    };
                    Ok(PreparedVault {
                        watched: matches!(update, Ok(true)),
                        watch_warning: update
                            .err()
                            .map(|error| rustidian::watcher::describe(&error)),
                        vault,
                        tree,
                        files,
                        monitoring: Some(Monitoring {
                            watcher,
                            events,
                            overflow,
                        }),
                    })
                };
                match prepare() {
                    Ok(prepared) => {
                        if cancelled.load(Ordering::Relaxed) {
                            return;
                        }
                        let _ = sender.send(Ok(prepared));
                        ctx.request_repaint();
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        ctx.request_repaint();
                    }
                }
            });
        if let Err(error) = spawned {
            self.loading = None;
            self.loading_root = None;
            self.error = Some(format!("Cannot start vault indexing: {error}"));
        }
    }

    fn background(&mut self, ctx: &egui::Context) {
        crate::ui_probe::metric(
            ctx,
            "vault-loading",
            f32::from(u8::from(self.loading_root.is_some())),
        );
        crate::ui_probe::metric(
            ctx,
            "vault-indexing",
            f32::from(u8::from(self.loading.is_some())),
        );
        let finished = self.autosave.finished();
        self.commit_saves(finished);
        if self.watch_overflow.swap(false, Ordering::Relaxed) {
            self.refresh_required = true;
            self.refresh_due = Some(Instant::now() + Duration::from_millis(150));
        }
        let mut renamed = Vec::new();
        if let Some(events) = &self.watch_events {
            for _ in 0..256 {
                let Ok(event) = events.try_recv() else {
                    break;
                };
                match event {
                    Ok(event)
                        if (event.kind.is_modify()
                            || event.kind.is_create()
                            || event.kind.is_remove())
                            && self.vault.as_ref().is_some_and(|vault| {
                                event.paths.iter().any(|path| {
                                    path.strip_prefix(&vault.root).is_ok_and(|relative| {
                                        !rustidian::vault::is_ignored_path(relative)
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
                                    || rustidian::assets::is_image(p)
                                    || rustidian::vault::is_text_document(p)
                                    || p.extension().is_none()
                            })) =>
                    {
                        if let Some(vault) = &self.vault {
                            if matches!(
                                event.kind,
                                notify::EventKind::Modify(notify::event::ModifyKind::Name(
                                    notify::event::RenameMode::Both
                                ))
                            ) && let [from, to] = event.paths.as_slice()
                                && let (Ok(from), Ok(to)) =
                                    (from.strip_prefix(&vault.root), to.strip_prefix(&vault.root))
                                && from != to
                                && !rustidian::vault::is_ignored_path(from)
                                && !rustidian::vault::is_ignored_path(to)
                                && from
                                    .components()
                                    .chain(to.components())
                                    .all(|part| matches!(part, std::path::Component::Normal(_)))
                            {
                                renamed.push((from.to_owned(), to.to_owned()));
                            }
                            for path in &event.paths {
                                if let Ok(relative) = path.strip_prefix(&vault.root)
                                    && rustidian::vault::is_markdown(relative)
                                    && !rustidian::vault::is_ignored_path(relative)
                                {
                                    self.recovery_changed_paths.insert(relative.to_owned());
                                }
                            }
                        }
                        for path in event
                            .paths
                            .iter()
                            .filter(|path| rustidian::assets::is_image(path))
                        {
                            ctx.forget_image(&format!("file://{}", path.display()));
                        }
                        self.refresh_paths.extend(event.paths);
                        self.refresh_due = Some(Instant::now() + Duration::from_millis(150));
                    }
                    Err(error) => self.error = Some(rustidian::watcher::describe(&error)),
                    _ => {}
                }
            }
        }
        if !renamed.is_empty() {
            for (source, destination) in renamed {
                self.relocate_tabs(&source, &destination);
            }
            self.persist_session();
        }
        if self.loading.is_none() && self.refresh_due.is_some_and(|due| Instant::now() >= due) {
            self.refresh_due = None;
            let paths = std::mem::take(&mut self.refresh_paths);
            let required = std::mem::take(&mut self.refresh_required);
            if let Some(root) = self.vault.as_ref().map(|vault| vault.root.clone())
                && (required || !paths.iter().all(|path| self.holds_known_text(path)))
            {
                self.queue_index(root);
            }
        }
        let result = self
            .loading
            .as_ref()
            .and_then(|loading| match loading.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "Vault indexing stopped. Try opening the folder again.".into(),
                )),
            });
        if let Some(result) = result {
            self.loading = None;
            match result {
                Ok(prepared) => {
                    let PreparedVault {
                        vault,
                        tree,
                        files,
                        monitoring,
                        watched,
                        watch_warning,
                    } = prepared;
                    if let Some(Monitoring {
                        watcher,
                        events,
                        overflow,
                    }) = monitoring
                    {
                        self.watcher = watcher;
                        self.watch_events = Some(events);
                        self.watch_overflow = overflow;
                    }
                    if let Some(warning) = watch_warning {
                        self.error = Some(warning);
                    }
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
                        self.always_update_links = rustidian::paths::read_obsidian::<
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
                        self.recovery_config = rustidian::recovery::Config::from_obsidian(
                            &vault.root,
                        )
                        .unwrap_or_else(|error| {
                            self.error = Some(error);
                            Default::default()
                        });
                        self.sidebar_widths = [300.0, 234.0];
                        self.restore_sidebars = [true; 2];
                        self.bookmarks = Default::default();
                        self.right_tab = 0;
                        self.ui_zoom = 1.0;
                        let appearance = rustidian::paths::read_obsidian::<serde_json::Value>(
                            &vault.root,
                            "appearance.json",
                        )
                        .unwrap_or_else(|error| {
                            self.error = Some(error);
                            Default::default()
                        });
                        self.fonts = theme::fonts::Fonts::from_obsidian(&appearance);
                        self.document_font_size = appearance
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
                        let (note_aids, import_errors) =
                            rustidian::note_aids::Config::from_obsidian_partial(&vault.root);
                        self.note_aids = note_aids;
                        if !import_errors.is_empty() {
                            self.error = Some(import_errors.join("\n"));
                        }
                        let first = vault
                            .notes
                            .iter()
                            .find(|n| n.path == Path::new("Home.md"))
                            .or_else(|| vault.notes.first())
                            .map(|n| n.path.clone());
                        self.tabs.clear();
                        self.active = 0;
                        self.search.clear();
                        self.refresh_due = None;
                        self.refresh_paths.clear();
                        self.refresh_required = false;
                        self.tree = tree;
                        self.files = files;
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
                        let restored = self.restore_session(ctx);
                        self.missing_fonts = theme::set_fonts(ctx, &self.fonts);
                        if !restored && let Some(path) = first {
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
                        self.tree = tree;
                        self.files = files;
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
                    if watched {
                        // Catch changes between the scan and completed registration once;
                        // the follow-up scan reuses these watches.
                        self.refresh_required = true;
                        self.refresh_due = Some(Instant::now() + Duration::from_millis(150));
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
                && !self.autosave.busy(tab.id())
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
            self.autosave(index);
        }
        // A running index wakes the window when it ends; repainting until then
        // would redraw an unchanged window every frame.
        if let Some(due) = self.refresh_due
            && self.loading.is_none()
        {
            ctx.request_repaint_after(due.saturating_duration_since(Instant::now()));
        }
        self.capture_recovery(ctx);
    }

    /// Whether a file the watcher reported holds what Rustidian already has
    /// for it, as after its own save; anything else needs a re-index.
    fn holds_known_text(&self, path: &Path) -> bool {
        let Some(vault) = &self.vault else {
            return false;
        };
        let Ok(relative) = path.strip_prefix(&vault.root) else {
            return false;
        };
        if rustidian::vault::is_ignored_path(relative) {
            return true;
        }
        let known = vault
            .notes
            .iter()
            .find(|note| note.path == relative)
            .map(|note| note.text.as_str())
            .or_else(|| {
                self.tabs
                    .iter()
                    .find(|tab| tab.document.path == relative)
                    .and_then(|tab| tab.document.saved_text())
            });
        known.is_some_and(|text| std::fs::read(path).is_ok_and(|bytes| bytes == text.as_bytes()))
    }

    fn capture_recovery(&mut self, ctx: &egui::Context) {
        let Some(vault) = &self.vault else { return };
        if self.recovery_recorder.is_none() {
            match rustidian::recovery::History::open(&vault.root) {
                Ok(history) => {
                    let wake = ctx.clone();
                    self.recovery_recorder = Some(rustidian::recovery_capture::Recorder::start(
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
            if !rustidian::vault::is_markdown(&document.path)
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
                                && rustidian::vault::is_markdown(&tab.document.path)
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
        self.settle_saves();
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
                    self.files = rustidian::search::FileMatcher::from_vault(vault);
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
        self.page_preview.close();
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
        if let Some(uri) = self
            .vault
            .as_ref()
            .and_then(|vault| vault.external_file_uri(from, target))
        {
            self.ctx.open_url(egui::OpenUrl::new_tab(uri));
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
                    self.files = rustidian::search::FileMatcher::from_vault(vault);
                    self.search_cache.invalidate();
                }
                self.open_note_at(&document.path, new_tab);
                if let Some((_, fragment)) = target.split_once('#') {
                    let fragment = fragment.split('|').next().unwrap_or(fragment);
                    let decoded = rustidian::paths::decode(fragment);
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
        if let Some(character) = rustidian::structure::find_anchor(&tab.document.text, fragment) {
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
        self.settle_saves();
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
            self.create_note();
            return false;
        }
        if let (Some(vault), Some(tab)) = (&mut self.vault, self.tabs.get_mut(index)) {
            match vault.save(&mut tab.document) {
                Ok(()) => {
                    self.saved(index);
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

    /// Autosave writes notes that exist on disk on a background thread; a blank
    /// tab or new note takes `save`, which creates its file.
    fn autosave(&mut self, index: usize) {
        let Some(tab) = self.tabs.get(index) else {
            return;
        };
        if tab.document.is_blank() || tab.document.saved_text().is_none() {
            self.save(index);
            return;
        }
        let Some(vault) = &self.vault else {
            return;
        };
        match vault.prepare_save(&tab.document) {
            Ok(save) => {
                let wake = self.ctx.clone();
                self.autosave
                    .submit(tab.id(), save, move || wake.request_repaint());
            }
            Err(error) => {
                self.tabs[index].autosave_failed = true;
                self.error = Some(error);
            }
        }
    }

    /// Finishes running autosaves. Anything that writes, moves or deletes notes,
    /// or asks whether edits are unsaved, calls this first.
    fn settle_saves(&mut self) {
        let finished = self.autosave.wait();
        self.commit_saves(finished);
    }

    fn commit_saves(&mut self, finished: Vec<rustidian::autosave::Done>) {
        for done in finished {
            // A tab that has since closed, moved or reloaded keeps its own saved
            // state; the file watcher re-indexes the written note.
            let Some(index) = self.tabs.iter().position(|tab| {
                tab.id() == done.tab
                    && tab.document.path == done.save.path
                    && tab.document.saved_text() == Some(done.save.original())
            }) else {
                continue;
            };
            match (done.result, &mut self.vault) {
                (Ok(()), Some(vault)) => {
                    vault.commit_save(&mut self.tabs[index].document, done.save.text);
                    self.saved(index);
                }
                (Ok(()), None) => {}
                (Err(error), _) => {
                    self.tabs[index].autosave_failed = true;
                    self.error = Some(error);
                }
            }
        }
    }

    /// What follows a note's bytes reaching the disk.
    fn saved(&mut self, index: usize) {
        let tab = &mut self.tabs[index];
        self.status = format!("Saved {}", tab.document.path.display());
        // Text typed while an autosave was writing still needs its own save.
        if !tab.document.dirty() {
            tab.edited_at = None;
        }
        tab.autosave_failed = false;
        self.error = None;
        if let Some(vault) = &self.vault {
            self.tree = Tree::from_vault(vault);
            self.files = rustidian::search::FileMatcher::from_vault(vault);
        }
        self.search_cache.invalidate();
        self.graph.invalidate();
    }

    fn dirty(&mut self) -> bool {
        self.settle_saves();
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
        self.dialog_skip_trash = false;
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
                        .map(rustidian::search::FileMatcher::new)
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
    }

    fn dialog_shortcut(&mut self, ctx: &egui::Context, key: Key, dialog: Dialog) {
        let requested = ctx.input_mut(|input| {
            let trigger = input.events.iter().position(|event| matches!(event,
                egui::Event::Key { key: pressed_key, pressed: true, modifiers, .. }
                    if *pressed_key == key && modifiers.matches_logically(egui::Modifiers::COMMAND)
            ));
            if let Some(trigger) = trigger {
                // Escape before the opening shortcut dismisses the previous
                // dialog. An Escape after it still closes the new dialog.
                let mut index = 0;
                input.events.retain(|event| {
                    let earlier = index < trigger;
                    index += 1;
                    !(earlier
                        && matches!(event,
                            egui::Event::Key { key: Key::Escape, pressed: true, modifiers, .. }
                                if modifiers.matches_logically(egui::Modifiers::NONE)
                        ))
                });
            }
            input.consume_key(egui::Modifiers::COMMAND, key)
        });
        if requested {
            self.ask(dialog);
        }
    }

    fn ask_file(&mut self, dialog: Dialog, path: PathBuf) {
        self.ask(dialog);
        self.dialog_input = path.display().to_string();
        self.dialog_path = Some(path);
        if dialog == Dialog::TrashNote && !self.confirm_trash && self.submit_dialog(dialog) {
            self.dialog = None;
            self.focus_editor();
        }
    }

    fn save_all(&mut self) -> bool {
        self.settle_saves();
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
        self.request_close_other_tabs_for(keep);
    }

    fn request_close_other_tabs_for(&mut self, keep: u64) {
        let indices = rustidian::tab::other_tab_indices(&self.tabs, keep);
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
        let indices = rustidian::tab::other_tab_indices(&self.tabs, keep);
        self.close_tab_group(indices, keep);
    }

    fn request_close_tabs_to_right(&mut self, keep: u64) {
        let indices = rustidian::tab::right_tab_indices(&self.tabs, keep);
        if indices
            .into_iter()
            .all(|index| !self.tabs[index].document.dirty() || self.save(index))
        {
            self.close_tabs_to_right(keep);
        } else {
            self.pending = Some(Pending::CloseTabsToRight(keep));
        }
    }

    fn close_tabs_to_right(&mut self, keep: u64) {
        let indices = rustidian::tab::right_tab_indices(&self.tabs, keep);
        self.close_tab_group(indices, keep);
    }

    fn close_tab_group(&mut self, indices: Vec<usize>, keep: u64) {
        for (removed, index) in indices.into_iter().enumerate() {
            self.close_tab(index - removed);
        }
        if let Some(index) = self.tabs.iter().position(|tab| tab.id() == keep) {
            self.active = index;
        }
        self.focus_editor();
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
            Pending::CloseTabsToRight(keep) => self.close_tabs_to_right(keep),
            Pending::Quit | Pending::ConfirmQuit => self.quit(ctx, "unsaved-changes dialog"),
            Pending::ChangeVault => self.ask(Dialog::OpenVault),
            Pending::OpenVault(root, document) => {
                self.startup_document = document;
                self.load_vault(&root);
            }
            Pending::LoadWorkspace(name) => self.load_workspace(&name),
            Pending::RestoreSnapshot(request) => self.restore_snapshot(*request),
        }
    }

    /// Close now, unsaved edits included: the user chose that in the
    /// unsaved-changes dialog, or a signal asked.
    fn quit(&mut self, ctx: &egui::Context, source: &'static str) {
        self.persist_session();
        self.allow_close = true;
        crate::window::close(ctx, source);
    }

    /// A close request from any source: the quit dialog, the title bar, the
    /// window manager or a signal. Unsaved edits first ask; a close that goes
    /// ahead prints `close: <source>`.
    fn on_close_request(&mut self, ctx: &egui::Context) {
        let source = crate::window::take_close_source(ctx);
        self.open_picker.cancel();
        let dirty = self.dirty();
        if !dirty {
            self.persist_session();
        }
        if dirty && !self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.pending = Some(Pending::Quit);
        } else {
            eprintln!("close: {source}");
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
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
            if ctx.input_mut(|input| {
                input.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::A)
            }) {
                if self.show_right
                    && self.right_tab == 3
                    && ctx.memory(|memory| memory.has_focus(crate::terminal_ui::focus_id()))
                {
                    self.show_right = false;
                    self.focus_editor();
                } else {
                    self.open_agent();
                }
            }
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
        if let Some(question) = crate::window::quit_question(ctx, || self.dirty()) {
            self.pending = Some(match question {
                crate::window::QuitQuestion::Quit => Pending::ConfirmQuit,
                crate::window::QuitQuestion::UnsavedChanges => Pending::Quit,
            });
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
            self.start_open_picker(rustidian::native_picker::Kind::File);
            return;
        }
        if self.dialog.is_none()
            && ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::ALT, Key::O)
            })
        {
            self.start_open_picker(rustidian::native_picker::Kind::Folder);
            return;
        }
        self.dialog_shortcut(ctx, Key::O, Dialog::QuickOpen);
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::N)) {
            self.create_note();
        }
        self.dialog_shortcut(ctx, Key::P, Dialog::Commands);
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

                let new_note = theme::icon_toggle(ui, Icon::NewNote, "New note · Ctrl+N", false);
                crate::ui_probe::record(ui.ctx(), "ribbon-new-note", new_note.rect);
                if new_note.clicked() {
                    self.create_note();
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

    fn start_open_picker(&mut self, kind: rustidian::native_picker::Kind) {
        let title = match kind {
            rustidian::native_picker::Kind::File => "Open file",
            rustidian::native_picker::Kind::Folder => "Open folder",
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

    fn open_system_selection(&mut self, kind: rustidian::native_picker::Kind, path: PathBuf) {
        use rustidian::{native_picker::Kind, vaults};
        let target = match kind {
            Kind::Folder => vaults::folder(&path).map(|root| (root, None)),
            Kind::File => (|| {
                let absolute = path
                    .canonicalize()
                    .map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
                let current = self.vault.as_ref().map(|vault| vault.root.as_path());
                let target =
                    if let Some(current) = current.filter(|root| absolute.starts_with(root)) {
                        // Within the active vault, keep its links and workspace context.
                        vaults::resolve_input(&absolute, Some(current), &[])?
                    } else {
                        let registered = vaults::Vaults::open().ok();
                        let default = rustidian::vaults::default_vault();
                        let roots = registered.as_ref().map_or(&[][..], |list| list.roots());
                        vaults::resolve_input(&absolute, default.as_deref(), roots)?
                    };
                if target.1.is_none() {
                    return Err("Choose an existing Markdown, text or source-code file.".into());
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

    fn save_bookmarks(&mut self, before: rustidian::bookmarks::Bookmarks) -> bool {
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

    fn open_sidebar_note(&mut self, path: &Path, new_tab: bool) {
        self.open_note_at(path, new_tab);
        self.editor.focus = false;
        self.ctx.memory_mut(|memory| {
            let id = crate::file_tree::note_list_id();
            if !memory.has_focus(id) {
                memory.request_focus(id);
            }
        });
        self.ctx.request_repaint();
    }

    fn bookmark_list(
        &mut self,
        ui: &mut egui::Ui,
        visible_files: &mut Option<Vec<PathBuf>>,
    ) -> Option<PathBuf> {
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
        theme::sidebar_scroll()
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
                    if exists && let Some(paths) = visible_files.as_mut() {
                        paths.push(item.path.clone());
                    }
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
                    crate::file_tree::reveal_note_row(ui, &item.path, response.rect);
                    response.context_menu(|ui| {
                        theme::menu_style(ui);
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
        open
    }

    pub(crate) fn workspace_layout(&self) -> rustidian::workspaces::Layout {
        rustidian::workspaces::Layout {
            tabs: self
                .tabs
                .iter()
                .map(|tab| rustidian::workspaces::SavedTab {
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
        let mut list = rustidian::workspaces::Workspaces::open(&vault.root)?;
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
            let mut list = rustidian::workspaces::Workspaces::open(&vault.root)?;
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
                self.right_tab = layout.right_tab.min(3);
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
        // The explorer follows the active note when it changes (open, tab switch,
        // link) and never otherwise, so the user's own scrolling stays put.
        let active = self
            .tabs
            .get(self.active)
            .map(|tab| tab.document.path.clone());
        if active != self.revealed_note {
            if !self.searching
                && !self.show_bookmarks
                && let Some(path) = &active
            {
                crate::file_tree::reveal(ui.ctx(), path);
            }
            self.revealed_note = active;
        }
        let mut selected = None;
        let navigation_enabled = ui.is_enabled() && !self.interaction_blocked();
        let collect_paths = navigation_enabled
            && ui.memory(|memory| memory.has_focus(crate::file_tree::note_list_id()))
            && ui.input(|input| {
                input.modifiers.is_none()
                    && [Key::ArrowUp, Key::ArrowDown, Key::Home, Key::End]
                        .iter()
                        .any(|key| input.key_pressed(*key))
            });
        let mut visible_files = collect_paths.then(Vec::new);
        self.prepare_sidebar(ui, "explorer", 0);
        let pane = egui::Panel::left("explorer")
            .default_size(300.0)
            .min_size(180.0)
            .max_size(450.0)
            .frame(
                egui::Frame::NONE
                    .fill(ui.visuals().panel_fill)
                    .inner_margin(Margin::symmetric(12, 0)),
            )
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
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
                            // Keep all four controls inside even the minimum-width pane.
                            let controls_width = 4.0 * 30.0 + 3.0 * 2.0;
                            ui.add_space((ui.available_width() - controls_width).max(0.0));
                            if theme::icon_toggle(ui, Icon::NewNote, "New note · Ctrl+N", false)
                                .clicked()
                            {
                                self.create_note();
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
                                theme::menu_style(ui);
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
                    selected = self
                        .bookmark_list(ui, &mut visible_files)
                        .map(FileAction::Open);
                } else if self.searching {
                    let input = ui.add(
                        TextEdit::singleline(&mut self.search)
                            .hint_text("Search your notes…")
                            .desired_width(f32::INFINITY)
                            .font(FontId::proportional(14.0)),
                    );
                    crate::ui_probe::record(ui.ctx(), "search-input", input.rect);
                    if self.search_focus {
                        input.request_focus();
                        self.search_focus = false;
                    }
                    ui.add_space(10.0);
                    if let Some(vault) = &self.vault {
                        self.search_cache.update(&self.search, vault);
                        let results = &self.search_cache.paths;
                        if let Some(paths) = visible_files.as_mut() {
                            paths.clone_from(results);
                        }
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
                        let target = crate::file_tree::pending_reveal(ui.ctx());
                        let mut search_scroll = theme::sidebar_scroll().id_salt("search-results");
                        if let Some(index) = target
                            .as_ref()
                            .and_then(|path| results.iter().position(|item| item == path))
                        {
                            search_scroll = search_scroll.vertical_scroll_offset(
                                (index as f32 * 52.0).min(
                                    (results.len() as f32 * 52.0
                                        - (ui.available_height() - 64.0).max(0.0))
                                    .max(0.0),
                                ),
                            );
                        }
                        let scroll = search_scroll
                            .max_height((ui.available_height() - 64.0).max(0.0))
                            .show_rows(ui, 52.0, results.len(), |ui, rows| {
                                for note in &results[rows] {
                                    let row = ui.allocate_ui_with_layout(
                                        egui::vec2(ui.available_width(), 52.0),
                                        egui::Layout::top_down(egui::Align::Min),
                                        |ui| {
                                            // Keep rendered rows aligned with show_rows' virtual stride.
                                            ui.set_min_height(52.0);
                                            let result = ui.add(
                                                egui::Button::new(
                                                    RichText::new(
                                                        note.file_stem()
                                                            .unwrap_or_default()
                                                            .to_string_lossy(),
                                                    )
                                                    .size(14.0),
                                                )
                                                .frame_when_inactive(false)
                                                .truncate(),
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
                                    if target.as_ref() == Some(note) {
                                        crate::file_tree::reveal_note_row(
                                            ui,
                                            note,
                                            row.response.rect,
                                        );
                                    }
                                }
                            });
                        crate::ui_probe::record(ui.ctx(), "search-viewport", scroll.inner_rect);
                    }
                } else {
                    // The vault name and the tree outside every folder take a drop into
                    // the vault root; its highlight is painted behind both.
                    let root_drop = ui.painter().add(egui::Shape::Noop);
                    let root = ui.label(
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
                    crate::ui_probe::record(ui.ctx(), "file-tree-root", root.rect);
                    let root = root.interact(egui::Sense::click());
                    ui.add_space(8.0);
                    let mut tree_scroll = theme::sidebar_scroll()
                        .id_salt("file-tree")
                        .max_height((ui.available_height() - 64.0).max(0.0));
                    if self.reset_tree_scroll {
                        tree_scroll = tree_scroll.vertical_scroll_offset(0.0);
                        self.reset_tree_scroll = false;
                    }
                    let mut drop_folder = None;
                    let scroll = tree_scroll.show(ui, |ui| {
                        let (action, revealed) = self.tree.show(
                            ui,
                            "",
                            self.tabs
                                .get(self.active)
                                .map(|tab| tab.document.path.as_path()),
                            self.sort_descending,
                            self.collapse_epoch,
                            self.reveal_folder.as_deref(),
                            &mut visible_files,
                            &mut drop_folder,
                        );
                        selected = action;
                        if revealed {
                            self.reveal_folder = None;
                        }
                        crate::file_tree::drag_scroll(ui);
                    });
                    if self.vault.is_some() {
                        root.context_menu(|ui| {
                            theme::menu_style(ui);
                            crate::file_tree::creation_menu(ui, Path::new(""), &mut selected);
                            crate::file_tree::reveal_menu(ui, Path::new(""), &mut selected);
                        });
                    }
                    if let Some(action) = crate::file_tree::finish_drag(
                        ui,
                        drop_folder,
                        root.rect.union(scroll.inner_rect),
                        root_drop,
                    ) {
                        selected = Some(action);
                    }
                    crate::ui_probe::record(ui.ctx(), "file-tree-viewport", scroll.inner_rect);
                    crate::ui_probe::metric(
                        ui.ctx(),
                        "file-tree-offset",
                        scroll.state.clamped_offset().y,
                    );
                    // Scrolling the tree by hand drops a reveal still waiting for its row.
                    if ui.rect_contains_pointer(scroll.inner_rect.expand2(egui::vec2(8.0, 0.0)))
                        && ui.input(|input| {
                            input.pointer.any_down()
                                || input
                                    .events
                                    .iter()
                                    .any(|event| matches!(event, egui::Event::MouseWheel { .. }))
                        })
                    {
                        crate::file_tree::cancel_reveal(ui.ctx());
                    }
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
                        // A row as tall as its 30 px controls, clear of the status bar: a plain
                        // `horizontal` row is shorter, and the controls spilled under the bar.
                        ui.add_space(4.0);
                        ui.allocate_ui_with_layout(
                            egui::vec2(ui.available_width(), 30.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                let width = (ui.available_width() - 38.0).max(40.0);
                                let vault = ui.add_sized(
                                    [width, 30.0],
                                    egui::Button::new((
                                        theme::icon_image(
                                            Icon::Disclosure,
                                            ui.visuals().weak_text_color(),
                                        )
                                        .rotate(
                                            std::f32::consts::FRAC_PI_2,
                                            egui::Vec2::splat(0.5),
                                        ),
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
                            },
                        );
                        ui.separator();
                    },
                );
            });
        self.capture_sidebar(ui, "explorer", 0);
        let list = ui.interact(
            pane.response.rect,
            crate::file_tree::note_list_id(),
            egui::Sense::focusable_noninteractive(),
        );
        let visible_files = visible_files.as_deref().unwrap_or_default();
        if list.has_focus()
            && navigation_enabled
            && let Some(key) = crate::file_tree::note_list_key(ui)
        {
            if key == Key::Enter {
                list.surrender_focus();
                self.focus_editor();
            } else if !visible_files.is_empty() {
                let current = self.tabs.get(self.active).and_then(|tab| {
                    visible_files
                        .iter()
                        .position(|path| *path == tab.document.path)
                });
                let last = visible_files.len() - 1;
                let index = match key {
                    Key::Home => 0,
                    Key::End => last,
                    Key::ArrowUp => current.map_or(last, |index| index.saturating_sub(1)),
                    _ => current.map_or(0, |index| (index + 1).min(last)),
                };
                let path = visible_files[index].clone();
                crate::file_tree::reveal(ui.ctx(), &path);
                selected = Some(FileAction::Open(path));
            }
        }
        if let Some(action) = selected {
            match action {
                FileAction::Open(path) => self.open_sidebar_note(
                    &path,
                    self.new_tab_gesture || ui.ctx().input(|i| i.modifiers.command),
                ),
                FileAction::NewNote(folder) => self.create_note_in(&folder),
                FileAction::NewFolder(folder) => {
                    self.ask(Dialog::NewFolder);
                    self.dialog_path = Some(folder);
                    self.dialog_input = "Untitled".into();
                }
                FileAction::OpenInNewTab(path) => self.open_note_at(&path, true),
                FileAction::Copy(path) => {
                    self.duplicate_file(&path);
                }
                FileAction::Move(path) => self.ask_move_file(path),
                FileAction::MoveMany(paths) => {
                    if let Some(vault) = &self.vault {
                        self.file_move.open_many(vault, paths);
                        self.editor.focus = false;
                    }
                }
                FileAction::TrashMany(paths) => {
                    self.file_move.ask_trash(paths);
                    self.editor.focus = false;
                }
                FileAction::MoveManyInto { sources, folder } => {
                    self.move_entries(&sources, &folder, false)
                }
                FileAction::Reveal(path) => self.reveal_in_file_manager(path),
                FileAction::CopyPath(path) => {
                    if let Some(vault) = &self.vault {
                        ui.ctx()
                            .copy_text(vault.root.join(path).display().to_string());
                    }
                }
                FileAction::Rename(path) => self.ask_file(Dialog::RenameNote, path),
                FileAction::Trash(path) => self.ask_file(Dialog::TrashNote, path),
                FileAction::Bookmark(path) => self.add_bookmark(&path),
                FileAction::MoveInto { source, folder } => self.drop_into(&source, &folder),
            }
        }
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        let agent_tab = self.right_tab == 3;
        // The user opened the Agent on purpose; it stays until the editor would lose its minimum.
        let minimum = if agent_tab { 480.0 } else { 700.0 };
        if !self.show_right || ui.available_width() < minimum || self.view() == View::Graph {
            return;
        }
        let document = self.tabs.get(self.active).map(|tab| &tab.document);
        if document.is_none() && !agent_tab {
            return;
        }
        let outline = document
            .filter(|_| self.right_tab == 0)
            .map(|document| rustidian::structure::outline(&document.text))
            .unwrap_or_default();
        let outgoing = document
            .filter(|_| self.right_tab == 2)
            .map(|document| wiki_links(&document.text))
            .unwrap_or_default();
        let backlinks = document
            .filter(|_| self.right_tab == 1)
            .zip(self.vault.as_ref())
            .map(|(document, vault)| {
                let open: Vec<_> = self
                    .tabs
                    .iter()
                    .map(|tab| (tab.document.path.as_path(), tab.document.text.as_str()))
                    .collect();
                vault.backlink_mentions(&document.path, &open)
            })
            .unwrap_or_default();
        let root = agent_tab
            .then(|| self.vault.as_ref().map(|vault| vault.root.clone()))
            .flatten();
        let mut selected_backlink = None;
        let mut follow = None;
        let mut jump = None;
        self.prepare_sidebar(ui, "details", 1);
        egui::Panel::right("details")
            .default_size(234.0)
            .min_size(170.0)
            .max_size((ui.available_width() - 300.0).clamp(170.0, 640.0))
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
                        (3, "Agent", "agent"),
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
                if agent_tab {
                    match &root {
                        Some(root) => {
                            // The editor's code font.
                            let font = ui
                                .scope(|ui| {
                                    theme::document_style(ui, self.document_font_size);
                                    egui::TextStyle::Monospace.resolve(ui.style())
                                })
                                .inner;
                            self.agent.show(ui, root, &font);
                        }
                        None => {
                            ui.label(RichText::new("Open a vault to use the agent").weak());
                        }
                    }
                    return;
                }
                theme::sidebar_scroll().show(ui, |ui| {
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
                        let count = ui.label(
                            RichText::new(format!("{} linked mentions", backlinks.len()))
                                .size(12.0)
                                .weak(),
                        );
                        crate::ui_probe::record(
                            ui.ctx(),
                            format!("backlink-count:{}", backlinks.len()),
                            count.rect,
                        );
                        ui.add_space(9.0);
                        let mut previous_source = None;
                        for mention in backlinks {
                            let first = previous_source.as_ref() != Some(&mention.path);
                            if first {
                                ui.add_space(6.0);
                                ui.label(
                                    RichText::new(rustidian::paths::display_path(&mention.path))
                                        .size(13.0)
                                        .strong(),
                                );
                                previous_source = Some(mention.path.clone());
                            }
                            let format = egui::TextFormat {
                                font_id: egui::FontId::proportional(12.0),
                                color: ui.visuals().text_color(),
                                ..Default::default()
                            };
                            let mut job = egui::text::LayoutJob::default();
                            job.wrap.max_width = (ui.available_width() - 12.0).max(24.0);
                            job.append(
                                &format!("{}: {}", mention.line, mention.before),
                                0.0,
                                format.clone(),
                            );
                            job.append(
                                &mention.link.replace(['\r', '\n'], " "),
                                0.0,
                                egui::TextFormat {
                                    color: theme::accent(ui),
                                    ..format.clone()
                                },
                            );
                            job.append(&mention.after, 0.0, format);
                            let row = ui
                                .add(egui::Button::new(job).frame_when_inactive(false))
                                .on_hover_text(mention.path.display().to_string());
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!(
                                    "backlink-mention:{}:{}",
                                    mention.path.display(),
                                    mention.range.start
                                ),
                                row.rect,
                            );
                            if first {
                                crate::ui_probe::record(
                                    ui.ctx(),
                                    format!("backlink:{}", mention.path.display()),
                                    row.rect,
                                );
                            }
                            if theme::link_clicked(&row) {
                                selected_backlink = Some(mention);
                            }
                            ui.add_space(3.0);
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
        if let Some(mention) = selected_backlink {
            self.open_linked_mention(mention);
        }
        if let Some(target) = follow {
            self.follow_link(&target);
        }
    }

    fn open_linked_mention(&mut self, mention: rustidian::vault::LinkedMention) {
        let new_tab = self.new_tab_gesture
            || self.ctx.input(|input| {
                input.modifiers.command || input.pointer.button_clicked(egui::PointerButton::Middle)
            });
        self.open_note_at(&mention.path, new_tab);
        let Some(tab) = self
            .tabs
            .get_mut(self.active)
            .filter(|tab| tab.document.path == mention.path)
        else {
            return;
        };
        if tab.document.text.get(mention.range.clone()) != Some(mention.link.as_str()) {
            self.error = Some(
                "The linked mention changed. Select it again to open its current location.".into(),
            );
            return;
        }
        if matches!(tab.view, View::Read | View::Graph) {
            tab.set_view(tab.editing_view());
        }
        let start = tab.document.text[..mention.range.start].chars().count();
        let end = start + mention.link.chars().count();
        self.editor.reveal = Some((tab.id(), start..end));
        self.focus_editor();
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
        self.graph.set_active_note(
            self.tabs
                .get(self.active)
                .map(|tab| tab.document.path.as_path()),
        );
        let root = local.then(|| self.tabs[self.active].document.path.clone());
        let action = self
            .vault
            .as_ref()
            .and_then(|vault| self.graph.show(ui, vault, root, local, blocked));
        match action {
            Some(crate::graph_ui::Action::CloseLocal) => self.graph.local_open = false,
            Some(crate::graph_ui::Action::Open { path, kind }) => match kind {
                rustidian::graph::Kind::Note | rustidian::graph::Kind::Attachment => {
                    self.open_note(&path)
                }
                rustidian::graph::Kind::Unresolved => {
                    self.follow_link_from(Path::new(""), &path.to_string_lossy())
                }
                rustidian::graph::Kind::Tag => {
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
        let background = ui.visuals().window_fill;
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
                            self.create_note();
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
                                let path = rustidian::paths::display_path(
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
                                    use rustidian::tab::ImageScale;
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
                                        theme::menu_style(ui);
                                        for (label, dialog) in [
                                            ("Rename or move…", Dialog::RenameNote),
                                            ("Delete file", Dialog::TrashNote),
                                        ] {
                                            let activated = if dialog == Dialog::TrashNote {
                                                theme::delete_menu_item(ui, label).1
                                            } else {
                                                ui.button(label).clicked()
                                            };
                                            if activated {
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
                                    theme::menu_style(ui);
                                    if ui.button("Save · Ctrl+S").clicked() {
                                        self.save(self.active);
                                        ui.close();
                                    }
                                    if ui.button("Save a copy…").clicked() {
                                        self.ask(Dialog::SaveCopy);
                                        ui.close();
                                    }
                                    ui.separator();
                                    for (label, dialog) in [
                                        ("Rename or move…", Dialog::RenameNote),
                                        ("Delete file", Dialog::TrashNote),
                                    ] {
                                        let (response, activated) = if dialog == Dialog::TrashNote {
                                            theme::delete_menu_item(ui, label)
                                        } else {
                                            let response = ui.button(label);
                                            let activated = response.clicked();
                                            (response, activated)
                                        };
                                        crate::ui_probe::record(
                                            ui.ctx(),
                                            format!("note-menu:{label}"),
                                            response.rect,
                                        );
                                        if activated {
                                            self.ask_file(
                                                dialog,
                                                self.tabs[self.active].document.path.clone(),
                                            );
                                            ui.close();
                                        }
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
                                    if ui.button(rustidian::commands::OPEN_GRAPH).clicked() {
                                        self.open_global_graph();
                                        ui.close();
                                    }
                                    if self.local_graph_available()
                                        && ui
                                            .button(rustidian::commands::OPEN_LOCAL_GRAPH)
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
        if self.active >= self.tabs.len() {
            return;
        }
        let (before, after) = self.tabs.split_at_mut(self.active);
        let (tab, after) = after.split_first_mut().expect("active tab was checked");
        let open_notes: Vec<_> = before
            .iter()
            .chain(after.iter())
            .map(|tab| (tab.document.path.as_path(), tab.document.text.as_str()))
            .collect();
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
        if let Some(vault) = &self.vault {
            let shared =
                crate::editor_ui::document_history(ui.ctx(), &vault.root, &tab.document.path);
            let mut state =
                egui::widgets::text_edit::TextEditState::load(ui.ctx(), id).unwrap_or_default();
            state.share_undoer_with(&shared);
            let history_key =
                ui.input(|input| input.key_pressed(Key::Z) || input.key_pressed(Key::Y));
            let shortcuts = history_key
                && self.dialog.is_none()
                && self.pending.is_none()
                && !self.open_picker.active()
                && !self.vault_manager.open
                && !self.workspace_manager.open
                && !self.recovery_manager.active()
                && !self.file_move.active()
                && ui.is_enabled()
                && ui.memory(|memory| memory.allows_interaction(ui.layer_id()))
                && (self.editor.has_focus(ui.ctx(), id)
                    || crate::table_layout::has_focus(ui.ctx(), &tab.document.path)
                    || view == View::Read && !ui.ctx().egui_wants_keyboard_input());
            while crate::table_layout::history(
                ui.ctx(),
                &tab.document.path,
                &tab.document.text,
                &mut state,
                shortcuts,
            ) {}
            state.store(ui.ctx(), id);
        }
        crate::editor_ui::find_bar(ui, &mut self.find, tab, id);
        let mut follow = None;
        let anchor = self.pending_anchor.take();
        let mut add_property = false;
        let mut renamed_title = None;
        let title = tab.document.title();
        let area = ScrollArea::vertical()
            .id_salt(("document", tab.id(), tab.editor_path()))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .auto_shrink([false, false]);
        let scroll_id =
            ui.make_persistent_id(egui::IdSalt::new(("document", tab.id(), tab.editor_path())));
        let scroll = self.page_pan.scroll_area(scroll_id, area).show(ui, |ui| {
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
                        self.vault.as_mut(),
                        &open_notes,
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
                                && let Some(name) = self.title.show(ui, &tab.document.path, &title)
                            {
                                renamed_title = Some((tab.document.path.clone(), name));
                            }
                            if self.title.take_body_focus() {
                                self.editor.focus = true;
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
                                    self.vault.as_mut(),
                                    &open_notes,
                                );
                            }
                            ui.add_space(ui.available_height().max(80.0));
                        },
                    );
                });
            }
        });
        crate::ui_probe::record(ui.ctx(), "document-viewport", scroll.inner_rect);
        self.page_pan.viewport(&scroll);
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
        if let Some(request) = self.editor.block_completion.take() {
            if let Err(error) = self.complete_block_link(ui.ctx(), request) {
                self.error = Some(error);
                self.focus_editor();
            }
        }
        for action in std::mem::take(&mut self.editor.embed_actions) {
            self.apply_preview_action(ui.ctx(), Some(action));
        }
        if let Some(target) = follow {
            self.follow_link(&target);
        }
    }

    fn complete_block_link(
        &mut self,
        ctx: &egui::Context,
        request: crate::editor_ui::BlockCompletion,
    ) -> Result<(), String> {
        self.settle_saves();
        let source_index = self
            .tabs
            .iter()
            .position(|tab| tab.id() == request.tab)
            .ok_or("The source tab is no longer open.")?;
        let source = self.tabs[source_index].document.clone();
        let query = format!("[[{}", request.completion.query);
        let typed = source
            .text
            .get(request.completion.range.clone())
            .ok_or("The link changed. Choose the block again.")?;
        if typed != query && typed != format!("{query}]]") {
            return Err("The link changed. Choose the block again.".into());
        }
        let target_indices: Vec<_> = self
            .tabs
            .iter()
            .enumerate()
            .filter_map(|(index, tab)| (tab.document.path == request.path).then_some(index))
            .collect();
        let target = if let Some(&index) = target_indices.first() {
            self.tabs[index].document.clone()
        } else {
            self.vault
                .as_ref()
                .ok_or("No vault is open.")?
                .read(&request.path)?
        };
        if target.text.get(request.block.range.clone()) != Some(request.block.text.as_str())
            || target_indices
                .iter()
                .any(|&index| self.tabs[index].document.text != target.text)
            || self
                .tabs
                .iter()
                .any(|tab| tab.document.path == source.path && tab.document.text != source.text)
        {
            return Err("The block or an open draft changed. Choose the block again; nothing was overwritten.".into());
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos();
        let mut id = format!("r{stamp:x}");
        while rustidian::structure::find_anchor(&target.text, &format!("^{id}")).is_some() {
            id.push('a');
        }
        let (position, insertion) = request.block.id_edit(&id);
        let mut changed = target.clone();
        if position > changed.text.len() || !changed.text.is_char_boundary(position) {
            return Err("The block changed. Choose it again.".into());
        }
        changed.text.insert_str(position, &insertion);
        let mut completion = request.completion;
        let destination = format!("{}{id}", request.prefix);
        let mut caret = None;
        if source.path == target.path {
            if position <= completion.range.start {
                completion.range.start += insertion.len();
                completion.range.end += insertion.len();
            }
            caret = Some(rustidian::editor::complete_link(
                &mut changed.text,
                &completion,
                &destination,
            ));
        }
        // A failed target write leaves both drafts and the link untouched.
        self.vault
            .as_mut()
            .ok_or("No vault is open.")?
            .save(&mut changed)?;
        let root = self.vault.as_ref().expect("vault was checked").root.clone();
        crate::editor_ui::record_document_edit(ctx, &root, &target, &changed);
        for &index in &target_indices {
            self.tabs[index].replace(changed.clone());
        }
        if source.path != target.path {
            let mut after = source.clone();
            caret = Some(rustidian::editor::complete_link(
                &mut after.text,
                &completion,
                &destination,
            ));
            crate::editor_ui::record_document_edit(ctx, &root, &source, &after);
            for tab in &mut self.tabs {
                if tab.document.path == source.path {
                    tab.document.text.clone_from(&after.text);
                    tab.edited();
                }
            }
        }
        self.editor.jump = caret;
        self.active = source_index;
        self.focus_editor();
        self.refresh_file_views();
        self.error = None;
        self.status = "Block reference inserted".into();
        Ok(())
    }

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        crate::ui_probe::metric(ui.ctx(), "tab-count", self.tabs.len() as f32);
        let mut close = None;
        let mut close_others = None;
        let mut close_right = None;
        let mut pin = None;
        let mut moved = None;
        let strip = egui::Frame::NONE
            .fill(ui.visuals().panel_fill)
            .inner_margin(Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.spacing_mut().item_spacing = egui::vec2(4.0, 0.0);
                ui.horizontal(|ui| {
                    let count = self.tabs.len().max(1) as f32;
                    // With no tabs the strip takes no room, so New tab sits at the left as in Obsidian.
                    let width = if self.tabs.is_empty() {
                        0.0
                    } else {
                        (ui.available_width() - 148.0)
                            .max(80.0)
                            .min(200.0 * count + 4.0 * (count - 1.0))
                    };
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
                                            let (_, rect) =
                                                ui.allocate_space(egui::vec2(tab_width, 33.0));
                                            let response = ui.interact(
                                                rect,
                                                ui.scope_id().with(("tab", tab.id())),
                                                egui::Sense::click_and_drag(),
                                            );
                                            response.dnd_set_drag_payload(DraggedTab(tab.id()));
                                            if response.drag_started() {
                                                self.active = index;
                                                self.editor.focus = false;
                                                response.request_focus();
                                            }
                                            response.widget_info(|| {
                                                egui::WidgetInfo::selected(
                                                    egui::Role::Tab,
                                                    ui.is_enabled(),
                                                    index == self.active,
                                                    tab.title(),
                                                )
                                            });
                                            let active = index == self.active;
                                            let show_close = tab.pinned
                                                || active
                                                || ui.rect_contains_pointer(rect);
                                            if active
                                                && reveal_active
                                                && !egui::DragAndDrop::has_payload_of_type::<
                                                    DraggedTab,
                                                >(
                                                    ui.ctx()
                                                )
                                            {
                                                ui.scroll_to_rect_animation(
                                                    rect,
                                                    Some(Align::Center),
                                                    egui::style::ScrollAnimation::none(),
                                                );
                                            }
                                            if rect.intersects(ui.clip_rect()) {
                                                let probe_path =
                                                    tab.document.path.display().to_string();
                                                crate::ui_probe::record(
                                                    ui.ctx(),
                                                    format!("tab:{probe_path}"),
                                                    rect.intersect(ui.clip_rect()),
                                                );
                                                crate::ui_probe::record(
                                                    ui.ctx(),
                                                    format!("tab-full:{probe_path}"),
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
                                                self.editor.focus = false;
                                                response.request_focus();
                                            }
                                            if response.has_focus() {
                                                ui.memory_mut(|memory| {
                                                    memory.set_focus_lock_filter(
                                                        response.id,
                                                        egui::EventFilter {
                                                            horizontal_arrows: true,
                                                            ..Default::default()
                                                        },
                                                    );
                                                });
                                                let target = ui.input_mut(|input| {
                                                    if input.consume_key(
                                                        egui::Modifiers::NONE,
                                                        Key::ArrowLeft,
                                                    ) {
                                                        Some(
                                                            (index + self.tabs.len() - 1)
                                                                % self.tabs.len(),
                                                        )
                                                    } else if input.consume_key(
                                                        egui::Modifiers::NONE,
                                                        Key::ArrowRight,
                                                    ) {
                                                        Some((index + 1) % self.tabs.len())
                                                    } else if input.consume_key(
                                                        egui::Modifiers::NONE,
                                                        Key::Home,
                                                    ) {
                                                        Some(0)
                                                    } else if input.consume_key(
                                                        egui::Modifiers::NONE,
                                                        Key::End,
                                                    ) {
                                                        Some(self.tabs.len() - 1)
                                                    } else {
                                                        None
                                                    }
                                                });
                                                if let Some(target) = target {
                                                    self.active = target;
                                                    self.editor.focus = false;
                                                    ui.memory_mut(|memory| {
                                                        memory.request_focus(
                                                            ui.scope_id().with((
                                                                "tab",
                                                                self.tabs[target].id(),
                                                            )),
                                                        )
                                                    });
                                                    ui.ctx().request_repaint();
                                                }
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
                                                let action = ui
                                                    .add_enabled_ui(
                                                        !rustidian::tab::other_tab_indices(
                                                            &self.tabs,
                                                            tab.id(),
                                                        )
                                                        .is_empty(),
                                                        |ui| {
                                                            theme::menu_item(
                                                                ui,
                                                                Icon::Close,
                                                                "Close others",
                                                            )
                                                        },
                                                    )
                                                    .inner;
                                                crate::ui_probe::record(
                                                    ui.ctx(),
                                                    "tab-menu-close-others",
                                                    action.rect,
                                                );
                                                if action.clicked() {
                                                    close_others = Some(tab.id());
                                                    ui.close();
                                                }
                                                let action = ui
                                                    .add_enabled_ui(
                                                        !rustidian::tab::right_tab_indices(
                                                            &self.tabs,
                                                            tab.id(),
                                                        )
                                                        .is_empty(),
                                                        |ui| {
                                                            theme::menu_item(
                                                                ui,
                                                                Icon::Close,
                                                                "Close tabs to the right",
                                                            )
                                                        },
                                                    )
                                                    .inner;
                                                crate::ui_probe::record(
                                                    ui.ctx(),
                                                    "tab-menu-close-right",
                                                    action.rect,
                                                );
                                                if action.clicked() {
                                                    close_right = Some(tab.id());
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
                                            if response.dnd_hover_payload::<DraggedTab>().is_some()
                                            {
                                                let after = ui
                                                    .input(|input| input.pointer.latest_pos())
                                                    .is_some_and(|pointer| {
                                                        pointer.x > rect.center().x
                                                    });
                                                let x =
                                                    if after { rect.right() } else { rect.left() };
                                                ui.painter().line_segment(
                                                    [
                                                        egui::pos2(x, rect.top() + 3.0),
                                                        egui::pos2(x, rect.bottom() - 3.0),
                                                    ],
                                                    Stroke::new(3.0, theme::accent(ui)),
                                                );
                                                if let Some(payload) =
                                                    response.dnd_release_payload::<DraggedTab>()
                                                {
                                                    moved = Some((
                                                        payload.0,
                                                        index + usize::from(after),
                                                    ));
                                                }
                                            }
                                            if active {
                                                ui.painter().line_segment(
                                                    [rect.left_bottom(), rect.right_bottom()],
                                                    Stroke::new(2.0, theme::interactive(ui)),
                                                );
                                            }
                                        }
                                    });
                                    if egui::DragAndDrop::has_payload_of_type::<DraggedTab>(
                                        ui.ctx(),
                                    ) && let Some(pointer) =
                                        ui.input(|input| input.pointer.latest_pos())
                                    {
                                        let clip = ui.clip_rect();
                                        if (clip.top()..=clip.bottom()).contains(&pointer.y) {
                                            let direction = if pointer.x < clip.left() + 24.0 {
                                                1.0
                                            } else if pointer.x > clip.right() - 24.0 {
                                                -1.0
                                            } else {
                                                0.0
                                            };
                                            if direction != 0.0 {
                                                ui.scroll_with_delta(egui::vec2(
                                                    direction * 12.0,
                                                    0.0,
                                                ));
                                                ui.ctx().request_repaint();
                                            }
                                        }
                                    }
                                });
                        },
                    );
                    let new_tab = icon_button(ui, Icon::NewTab, "New tab · Ctrl+T");
                    crate::ui_probe::record(ui.ctx(), "new-tab", new_tab.rect);
                    if new_tab.clicked() {
                        self.new_tab();
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let toggle = icon_button(ui, Icon::SidebarRight, "Toggle right sidebar");
                        crate::ui_probe::record(ui.ctx(), "toggle-right-sidebar", toggle.rect);
                        if toggle.clicked() {
                            self.show_right = !self.show_right;
                        }
                        let showing = self.show_right && self.right_tab == 3;
                        let agent = ui
                            .add_enabled_ui(self.vault.is_some(), |ui| {
                                ui.selectable_label(showing, RichText::new("Agent").size(13.0))
                                    .on_hover_text("Agent terminal")
                                    .on_disabled_hover_text("Open a vault first.")
                            })
                            .inner;
                        crate::ui_probe::record(ui.ctx(), "toggle-agent", agent.rect);
                        if agent.clicked() {
                            if showing {
                                self.show_right = false;
                            } else {
                                self.open_agent();
                            }
                        }
                    });
                });
            });
        crate::ui_probe::record(ui.ctx(), "tab-strip-background", strip.response.rect);
        if let Some((id, slot)) = moved
            && let Some(from) = self.tabs.iter().position(|tab| tab.id() == id)
        {
            let to = slot.saturating_sub(usize::from(from < slot));
            if from != to {
                let tab = self.tabs.remove(from);
                self.tabs.insert(to, tab);
                self.active = to;
                self.persist_session();
            }
        }
        if let Some(index) = close {
            self.request_close_tab(index);
        }
        if let Some(keep) = close_others {
            self.request_close_other_tabs_for(keep);
        }
        if let Some(keep) = close_right {
            self.request_close_tabs_to_right(keep);
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
                let response = egui::Modal::new(egui::Id::unique("system-open-picker"))
                    .frame(theme::dialog_frame(ctx))
                    .show(ctx, |ui| {
                        theme::dialog_style(ui);
                        ui.set_width(400.0);
                        ui.heading(match kind {
                            rustidian::native_picker::Kind::File => "Open file",
                            rustidian::native_picker::Kind::Folder => "Open folder",
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
        if matches!(self.pending, Some(Pending::ConfirmQuit)) {
            if crate::window::quit_dialog(ctx).is_some() {
                self.pending = None;
            }
            if self.pending.is_none() && self.dialog.is_none() && !self.allow_close {
                self.focus_editor();
            }
            return;
        }
        if let Some(pending) = self.pending.clone() {
            let response = egui::Modal::new(egui::Id::unique("unsaved-changes"))
            .frame(theme::dialog_frame(ctx)).show(ctx, |ui| {
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
                            Pending::CloseOtherTabs(keep) => rustidian::tab::other_tab_indices(&self.tabs, *keep)
                                .into_iter().filter(|&index| self.tabs[index].document.dirty()).collect(),
                            Pending::CloseTabsToRight(keep) => rustidian::tab::right_tab_indices(&self.tabs, *keep)
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
                        sources,
                        folder,
                        create,
                    } => {
                        if sources.len() == 1
                            && self
                                .vault
                                .as_ref()
                                .is_some_and(|vault| !vault.folders.contains(&sources[0]))
                        {
                            self.move_file(&sources[0], &folder, create);
                        } else {
                            self.move_entries(&sources, &folder, create);
                        }
                    }
                    crate::file_move_ui::Action::Trash(paths) => self.trash_entries(ctx, &paths),
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
        // egui's top-modal layer comes from the previous frame. Handle a
        // newly opened dialog's Escape here before its first modal pass.
        if self.dialog_focus
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            self.dialog = None;
            self.template_source = None;
            self.focus_editor();
            return;
        }
        let title = match dialog {
            Dialog::QuickOpen => "Open a file",
            Dialog::NewFolder => "New folder",
            Dialog::SaveCopy => "Save a copy",
            Dialog::OpenVault => "Open vault",
            Dialog::Commands => "Command palette",
            Dialog::RenameNote => "Rename or move",
            Dialog::TrashNote => "Move to Trash?",
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
        let mut create_note = false;
        let mut run_command = None;
        let mut recovery_action = None;
        let mut configure_templates = false;
        let mut modal =
            egui::Modal::new(egui::Id::unique("main-dialog")).frame(theme::dialog_frame(ctx));
        if dialog == Dialog::Settings {
            modal = modal.frame(theme::dialog_frame(ctx).inner_margin(0));
        }
        let response = modal.show(ctx, |ui| {
            theme::dialog_style(ui);
            if dialog == Dialog::Settings {
                self.settings_ui(ui, ctx, &mut close, &mut recovery_action);
                return;
            }
            ui.set_width(if matches!(dialog, Dialog::Commands | Dialog::Settings) {
                650.0
            } else {
                500.0
            });
            if dialog != Dialog::Commands {
                close |= theme::dialog_header(ui, title, "Close dialog").clicked();
                ui.add_space(8.0);
            }
            if dialog == Dialog::NewFolder && let Some(parent) = &self.dialog_path {
                let location = if parent.as_os_str().is_empty() {
                    "Vault root".to_owned()
                } else {
                    parent.display().to_string()
                };
                let label = ui.label(RichText::new(format!("Location: {location}")).weak());
                crate::ui_probe::record(ui.ctx(), format!("folder-create-location:{}", parent.display()), label.rect);
            }
            let hint = match dialog {
                Dialog::QuickOpen => "Find a file…",
                Dialog::SaveCopy => "Folder/Note.md",
                Dialog::NewFolder if self.dialog_path.is_some() => "New folder",
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
            if dialog != Dialog::TrashNote
                && self.dialog_focus && !ui.memory(|memory| memory.has_focus(input_id)) {
                ui.memory_mut(|memory| memory.request_focus(input_id));
            }
            let input = if dialog == Dialog::TrashNote {
                let response = ui.strong(&self.dialog_input);
                crate::ui_probe::record(ctx, "trash-target", response.rect);
                response
            } else {
                ui.add(TextEdit::singleline(&mut self.dialog_input)
                    .id(input_id)
                    .desired_width(f32::INFINITY)
                    .hint_text(hint))
            };
            if self.dialog_focus && dialog != Dialog::TrashNote {
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
            if dialog != Dialog::TrashNote {
                crate::ui_probe::record(ui.ctx(), "dialog-input", input.rect);
            }
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
                                egui::Button::new(rustidian::paths::display_path(path))
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
                        if !self.dialog_input.trim().is_empty() {
                            let create = ui.button("Create note");
                            crate::ui_probe::record(ui.ctx(), "quick-create", create.rect);
                            create_note = create.clicked() || enter;
                        }
                    }
                    ui.label(
                        RichText::new(if matches.is_empty() && !self.dialog_input.trim().is_empty() {
                            "Enter to create · Ctrl+Enter for a new tab"
                        } else {
                            "↑ ↓ to navigate · Enter to open · Ctrl+Enter for a new tab"
                        })
                            .size(11.0)
                            .weak(),
                    );
                }
                Dialog::InsertTemplate => {
                    match &mut self.template_choices {
                        Err(error) => {
                            let label = ui.label(error.as_str());
                            crate::ui_probe::record(ctx, "template-error", label.rect);
                            let configure = ui.button("Open Templates settings");
                            crate::ui_probe::record(ctx, "template-configure", configure.rect);
                            configure_templates = configure.clicked();
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
                                    let label = rustidian::paths::display_path(
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
                    let mut matches = rustidian::commands::ranked(&self.dialog_input, scope);
                    if !self.local_graph_available() {
                        matches.retain(|command| {
                            command.name != rustidian::commands::OPEN_LOCAL_GRAPH
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
                        rustidian::tab::other_tab_indices(&self.tabs, tab.id()).is_empty()
                    }) {
                        matches.retain(|command| {
                            command.name != rustidian::commands::CLOSE_OTHER_TABS
                        });
                    }
                    if self.closed_tabs.is_empty() {
                        matches
                            .retain(|command| command.name != rustidian::commands::UNDO_CLOSE_TAB);
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
                        ui.label(if self.dialog_path.as_ref().is_some_and(|path| {
                            self.vault
                                .as_ref()
                                .is_some_and(|vault| vault.folders.contains(path))
                        }) {
                            "The folder and everything in it will be recoverable from your desktop’s Trash."
                        } else {
                            "The file will be recoverable from your desktop’s Trash."
                        });
                    }
                    if dialog == Dialog::TrashNote {
                        let response = ui.checkbox(&mut self.dialog_skip_trash, "Don’t ask again");
                        crate::ui_probe::record(ui.ctx(), "trash-dont-ask", response.rect);
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
                    let (action, cancel) = theme::dialog_actions(ui, match dialog {
                        Dialog::OpenVault => "Open folder",
                        Dialog::NewFolder => "Create folder",
                        Dialog::RenameNote => "Rename",
                        Dialog::TrashNote => "Move to Trash",
                        Dialog::AddProperty => "Add property",
                        Dialog::SaveCopy => "Save copy",
                        Dialog::BookmarkTitle => "Save title",
                        _ => unreachable!(),
                    }, dialog == Dialog::TrashNote);
                    submit = action.clicked() || (enter && !cancel.has_focus());
                    close |= cancel.clicked() || (enter && cancel.has_focus());
                }
            }
        });
        crate::ui_probe::record(ctx, "dialog-bounds", response.response.rect);
        if response.should_close() {
            close = true;
        }
        if configure_templates && !close {
            self.template_source = None;
            self.ask(Dialog::Settings);
            self.settings_page = settings_ui::Page::Templates;
            return;
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
        if create_note {
            let name = self.dialog_input.clone();
            close = self
                .create_quick_note(&name, open_in_new_tab || ctx.input(|i| i.modifiers.command));
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
        if submit && !close {
            close = self.submit_dialog(dialog);
        }
        if close {
            self.dialog = None;
            self.template_source = None;
            if matches!(dialog, Dialog::RenameNote | Dialog::TrashNote) {
                if !submit && self.tabs.iter().all(|tab| !tab.document.dirty()) {
                    self.error = None;
                }
            }
            self.focus_editor();
        }
    }

    fn relocate_tabs(&mut self, source: &Path, destination: &Path) {
        crate::explorer_selection::moved(&self.ctx, source, destination);
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
                rustidian::paths::moved_path(&tab.document.path, source, destination, true);
            if let Some(vault) = &self.vault
                && !tab.document.is_blank()
                && !tab.document.dirty()
                && let Ok(document) = vault.open_document(&tab.document.path)
            {
                tab.replace(document);
            }
        }
    }

    fn refresh_file_views(&mut self) {
        if let Some(vault) = &self.vault {
            self.tree = Tree::from_vault(vault);
            self.files = rustidian::search::FileMatcher::from_vault(vault);
        }
        self.search_cache.invalidate();
        self.graph.invalidate();
        self.cache = CommonMarkCache::default();
        self.ctx.request_repaint();
    }

    fn show_page_preview(&mut self, ctx: &egui::Context) {
        let blocked = self.interaction_blocked() || self.view() == View::Graph;
        let action = self.page_preview.show(
            ctx,
            self.vault.as_ref(),
            &self.tabs,
            self.active,
            blocked,
            self.document_font_size,
        );
        self.apply_preview_action(ctx, action);
    }

    fn apply_preview_action(
        &mut self,
        ctx: &egui::Context,
        action: Option<crate::page_preview::Action>,
    ) {
        match action {
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

    fn move_entries(&mut self, sources: &[PathBuf], folder: &Path, create: bool) {
        if !self.save_all() {
            self.move_failed(
                self.error
                    .clone()
                    .unwrap_or_else(|| "Save your changes before moving files.".into()),
            );
            return;
        }
        let result = self
            .vault
            .as_mut()
            .ok_or_else(|| "Open a vault first.".to_owned())
            .and_then(|vault| vault.begin_move_entries(sources, folder, create));
        match result {
            Ok(Some(moved)) => {
                let count = moved.paths.len();
                for (from, to) in &moved.paths {
                    self.relocate_tabs(from, to);
                }
                self.reveal_folder = Some(folder.to_owned());
                if self.always_update_links || moved.updates.counts().0 == 0 {
                    self.file_move.moved(None);
                    self.apply_move_links(moved.updates);
                } else {
                    self.file_move.moved(Some(moved.updates));
                }
                self.status = format!(
                    "Moved {count} items to {}",
                    if folder.as_os_str().is_empty() {
                        "/".into()
                    } else {
                        folder.display().to_string()
                    }
                );
                self.refresh_file_views();
            }
            Ok(None) => self.file_move.moved(None),
            Err(error) => {
                self.move_failed(error);
                self.refresh_file_views();
            }
        }
        if let Err(error) = self.write_session() {
            self.error = Some(error);
        }
    }

    fn trash_entries(&mut self, ctx: &egui::Context, sources: &[PathBuf]) {
        let paths = match self
            .vault
            .as_ref()
            .ok_or_else(|| "Open a vault first.".to_owned())
            .and_then(|vault| vault.selected_entries(sources))
        {
            Ok(paths) => paths,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        if !self.save_all() {
            return;
        }
        let active = self.tabs.get(self.active).map(Tab::id);
        let mut removed = Vec::new();
        let mut failures = Vec::new();
        if let Some(vault) = &mut self.vault {
            for path in paths {
                match vault.trash_entry(&path) {
                    Ok(()) => removed.push(path),
                    Err(error) => failures.push(format!("{}: {error}", path.display())),
                }
            }
        }
        self.tabs.retain(|tab| {
            !removed
                .iter()
                .any(|path| tab.document.path.starts_with(path))
        });
        if let Some(index) = self.tabs.iter().position(|tab| Some(tab.id()) == active) {
            self.active = index;
        } else {
            self.new_tab();
        }
        self.status = format!("Moved {} items to system Trash", removed.len());
        self.error = (!failures.is_empty())
            .then(|| format!("Some items were not deleted: {}", failures.join("; ")));
        self.refresh_file_views();
        crate::explorer_selection::reset(ctx);
        if let Err(error) = self.write_session() {
            self.error = Some(error);
        }
    }

    fn reveal_in_file_manager(&mut self, relative: PathBuf) {
        let Some(vault) = &self.vault else {
            return;
        };
        let path = vault.root.join(&relative);
        if relative
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || !path.exists()
        {
            self.error = Some("This file or folder is no longer in the vault.".into());
            return;
        }
        let (sender, result) = mpsc::channel();
        self.reveal_result = Some(result);
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(rustidian::native_picker::reveal(&path));
            ctx.request_repaint();
        });
        self.error = None;
        self.status = "Opening system file manager…".into();
    }

    fn ask_move_file(&mut self, source: PathBuf) {
        if let Some(vault) = &self.vault {
            self.file_move.open(vault, source);
            self.editor.focus = false;
        }
    }

    fn apply_move_links(&mut self, updates: rustidian::operations::LinkUpdates) {
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
            let error = self
                .error
                .clone()
                .unwrap_or_else(|| "Save your changes before moving the file.".into());
            self.move_failed(error);
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
            Err(error) => self.move_failed(error),
        }
        if let Err(error) = self.write_session() {
            self.error = Some(error);
        }
    }

    /// The Move picker shows a failed move; a drag in the explorer has no picker.
    fn move_failed(&mut self, error: String) {
        if self.file_move.active() {
            self.file_move.error(error);
        } else {
            self.error = Some(error);
        }
    }

    /// A drag in the explorer let go of `source` over `folder` ("" is the vault root).
    /// A file takes the Move dialog's operation, a folder the folder rename, with a
    /// number after a taken folder name as Obsidian does. The tree stays where it is.
    fn drop_into(&mut self, source: &Path, folder: &Path) {
        let Some(vault) = &self.vault else {
            return;
        };
        if vault.folders.iter().any(|path| path == source) {
            let name = source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let mut target = folder.join(&name);
            let mut counter = 0;
            while std::fs::symlink_metadata(vault.root.join(&target)).is_ok() {
                counter += 1;
                target = folder.join(format!("{name} {counter}"));
            }
            let previous = self.dialog_path.replace(source.to_owned());
            let previous_input =
                std::mem::replace(&mut self.dialog_input, target.display().to_string());
            self.submit_dialog(Dialog::RenameNote);
            self.dialog_path = previous;
            self.dialog_input = previous_input;
        } else {
            self.move_file(source, folder, false);
        }
        self.reveal_folder = None;
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
            if input.is_empty() {
                self.error = Some("Enter a folder name.".into());
                return false;
            }
            let path = self
                .dialog_path
                .as_ref()
                .map_or_else(|| PathBuf::from(&input), |parent| parent.join(&input));
            let result = self
                .vault
                .as_mut()
                .ok_or("Open a vault first.".to_owned())
                .and_then(|vault| vault.create_folder(&path));
            return match result {
                Ok(()) => {
                    self.tree = Tree::from_vault(self.vault.as_ref().unwrap());
                    self.reveal_folder = Some(path.clone());
                    self.searching = false;
                    self.show_bookmarks = false;
                    self.status = format!("Created folder {}", path.display());
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
                rustidian::metadata::frontmatter(&tab.document.text).and_then(|existing| {
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
                    rustidian::metadata::set_property(&mut tab.document.text, &input, value)
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
                    .and_then(|vault| vault.trash_entry(&source).map(|()| None))
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
                    if self.dialog_skip_trash {
                        self.confirm_trash = false;
                    }
                    let active_id = self.tabs.get(self.active).map(Tab::id);
                    let deleted_active = self
                        .tabs
                        .get(self.active)
                        .is_some_and(|tab| tab.document.path.starts_with(&source));
                    self.tabs
                        .retain(|tab| !tab.document.path.starts_with(&source));
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
                self.files = rustidian::search::FileMatcher::from_vault(vault);
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
            match rustidian::vaults::folder(Path::new(&input)) {
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
        document.text = self
            .tabs
            .get(self.active)
            .map(|tab| tab.document.text.clone())
            .unwrap_or_default();
        let Some(vault) = &mut self.vault else {
            self.error = Some("Open a vault before creating a note.".into());
            return false;
        };
        // A successful create validates containment and prevents name collisions.
        match vault.save(&mut document) {
            Ok(()) => {
                self.tree = Tree::from_vault(vault);
                self.files = rustidian::search::FileMatcher::from_vault(vault);
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
        if let Some(color) = rustidian::highlight::NativeColor::ALL
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
        let highlight = rustidian::highlight::Color::ALL
            .into_iter()
            .find(|color| color.command() == command)
            .or_else(|| {
                (command == "Highlight selection").then_some(rustidian::highlight::Color::Yellow)
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
            rustidian::commands::OPEN_AGENT => self.open_agent(),
            rustidian::commands::OPEN_GRAPH => self.open_global_graph(),
            rustidian::commands::OPEN_LOCAL_GRAPH if self.local_graph_available() => {
                self.graph.local_open = true;
                self.ctx.request_repaint();
            }
            rustidian::commands::MOVE_FILE
            | rustidian::commands::DUPLICATE_FILE
            | "Rename or move file"
            | "Move file to system Trash"
            | "Bookmark current file"
                if self.view() == View::Graph => {}
            rustidian::commands::MOVE_FILE => {
                if let Some(path) = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.path.clone())
                {
                    self.ask_move_file(path);
                }
            }
            rustidian::commands::DUPLICATE_FILE => {
                if let Some(path) = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.document.path.clone())
                {
                    self.duplicate_file(&path);
                }
            }
            "Toggle pin" => self.toggle_pin(self.active),
            rustidian::commands::UNDO_CLOSE_TAB => self.undo_close_tab(),
            rustidian::commands::CLOSE_OTHER_TABS => self.request_close_other_tabs(),
            rustidian::commands::INCREASE_FONT_SIZE => {
                self.set_document_font_size(self.document_font_size.saturating_add(1))
            }
            rustidian::commands::DECREASE_FONT_SIZE => {
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
                self.open_created_note(rustidian::note_aids::Config::create_unique);
            }
            rustidian::commands::ADD_UNIQUE_LINK if self.editable_note() => {
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
                    .and_then(rustidian::note_aids::random_note)
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
            "New note" => self.create_note(),
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
            rustidian::commands::OPEN_FILE => {
                self.start_open_picker(rustidian::native_picker::Kind::File)
            }
            rustidian::commands::OPEN_FOLDER => {
                self.start_open_picker(rustidian::native_picker::Kind::Folder)
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
                self.appearance = theme::Appearance::from_dark(self.dark);
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
                self.files = rustidian::search::FileMatcher::from_vault(vault);
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

    fn create_quick_note(&mut self, name: &str, new_tab: bool) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let path = rustidian::paths::markdown_name(Path::new(name));
        if path
            .components()
            .any(|part| !matches!(part, std::path::Component::Normal(_)))
            || rustidian::vault::is_ignored_path(&path)
        {
            self.error = Some("Choose a visible note path inside this vault.".into());
            return false;
        }
        let new_tab = new_tab || self.tabs.get(self.active).is_some_and(|tab| tab.pinned);
        if !new_tab
            && self
                .tabs
                .get(self.active)
                .is_some_and(|tab| tab.document.dirty())
            && !self.save(self.active)
        {
            return false;
        }
        let Some(vault) = &mut self.vault else {
            self.error = Some("Open a vault before creating a note.".into());
            return false;
        };
        let mut document = Document::new(path.clone());
        if let Err(error) = vault.save(&mut document) {
            self.error = Some(error);
            return false;
        }
        self.refresh_file_views();
        self.error = None;
        self.open_note_at(&path, new_tab);
        self.set_view(View::Live);
        self.status = format!("Created {}", path.display());
        true
    }

    fn create_note(&mut self) {
        self.create_note_in(Path::new(""));
    }

    fn create_note_in(&mut self, folder: &Path) {
        let Some(vault) = &mut self.vault else {
            self.error = Some("Open a vault before creating a note.".into());
            return;
        };
        match vault.create_untitled_in(folder) {
            Ok(document) => {
                let path = document.path.clone();
                let title = document.title();
                self.refresh_file_views();
                self.dialog = None;
                self.error = None;
                self.status = format!("Created {}", path.display());
                if self
                    .tabs
                    .get(self.active)
                    .is_some_and(|tab| tab.document.is_blank() && !tab.pinned)
                {
                    self.tabs[self.active] = Tab::new(document);
                } else {
                    self.tabs.push(Tab::new(document));
                    self.active = self.tabs.len() - 1;
                }
                self.set_view(View::Live);
                if self.editor_preferences.inline_title {
                    self.editor.focus = false;
                    self.title.begin_edit(&path, &title);
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn open_daily(&mut self) {
        self.open_created_note(rustidian::note_aids::Config::create_daily);
    }

    fn open_created_note(
        &mut self,
        create: impl FnOnce(
            &rustidian::note_aids::Config,
            &mut Vault,
            chrono::DateTime<chrono::FixedOffset>,
        ) -> rustidian::vault::Result<PathBuf>,
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
                self.files = rustidian::search::FileMatcher::from_vault(vault);
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
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.agent.raw_input(ctx, raw);
        self.system_theme.update(ctx, raw, self.appearance);
        self.page_pan.raw_input(
            ctx,
            raw,
            !self.interaction_blocked() && self.view() != View::Graph,
        );
        let editing_tab = self
            .tabs
            .get(self.active)
            .filter(|tab| {
                matches!(tab.view, View::Live | View::Source | View::Split)
                    && !tab.document.is_image()
            })
            .map(Tab::id);
        self.editor
            .raw_input(ctx, raw, editing_tab, self.view() == View::Live);
    }

    /// Runs before every `ui` pass and alone while the window is hidden, so
    /// hand-offs, signals and close requests never wait for a drawn frame.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let result = self
            .reveal_result
            .as_ref()
            .and_then(|receiver| match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => Some(Err(
                    "The file-manager request was interrupted. Try again.".into(),
                )),
            });
        if let Some(result) = result {
            self.reveal_result = None;
            match result {
                Ok(()) => self.status = "Revealed in system file manager".into(),
                Err(error) => {
                    self.status.clear();
                    self.error = Some(error);
                }
            }
        }
        #[cfg(unix)]
        let launches: Vec<_> = self
            .launches
            .as_ref()
            .map(|launches| launches.try_iter().collect())
            .unwrap_or_default();
        #[cfg(unix)]
        for crate::instance::Request { path, token, shown } in launches {
            crate::window::bring_forward(ctx, frame, token.as_deref());
            self.showing.push(shown);
            if let Some(path) = path {
                let kind = if path.is_dir() {
                    rustidian::native_picker::Kind::Folder
                } else {
                    rustidian::native_picker::Kind::File
                };
                self.open_system_selection(kind, path);
            }
        }
        // A launch exits once this window has the focus. One that waits in
        // vain opens a window of its own instead; see `instance`.
        #[cfg(unix)]
        if !self.showing.is_empty() && ctx.input(|input| input.viewport().focused) == Some(true) {
            for shown in self.showing.drain(..) {
                let _ = shown.send(());
            }
        }
        if let Some(signal) = crate::window::take_signal() {
            self.quit(ctx, signal);
        }
        if ctx.input(|input| input.viewport().close_requested()) {
            self.on_close_request(ctx);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let frame_started = Instant::now();
        let ctx = ui.ctx().clone();
        if let Some(due) = self.attention_check {
            let now = Instant::now();
            if now < due {
                ctx.request_repaint_after(due - now);
            } else {
                self.attention_check = None;
                if ctx.input(|input| input.viewport().focused) != Some(true) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(
                        egui::UserAttentionType::Informational,
                    ));
                }
            }
        }
        self.dark = ctx.theme() == egui::Theme::Dark;
        crate::ui_probe::metric(
            &ctx,
            "appearance-system",
            if self.appearance == theme::Appearance::System {
                1.0
            } else {
                0.0
            },
        );
        crate::ui_probe::metric(&ctx, "appearance-dark", if self.dark { 1.0 } else { 0.0 });
        let local_graph_open = self.graph.local_open;
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
        if crate::table_layout::take_changed(&ctx)
            && let Err(error) = self.write_session()
        {
            self.error = Some(format!("Table widths weren’t saved: {error}"));
        }
        self.shortcuts(&ctx);
        let title = format!(
            "{} — {} — {}",
            self.tabs
                .get(self.active)
                .map(Tab::title)
                .unwrap_or("New tab".into()),
            self.vault.as_ref().map(Vault::name).unwrap_or_default(),
            rustidian::APP_NAME
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
        let status = egui::Panel::bottom("status")
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
        crate::ui_probe::record(ui.ctx(), "status-bar", status.response.rect);
        if self.dialog.is_none()
            && let Some(error) = self.error.clone()
        {
            egui::Area::new(egui::Id::new("error"))
                .order(egui::Order::Foreground)
                .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-16.0, 44.0))
                .show(&ctx, |ui| {
                    theme::notice_frame(&ctx).show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.add(
                                theme::icon_image(
                                    theme::Icon::CalloutWarning,
                                    ui.visuals().warn_fg_color,
                                )
                                .fit_to_exact_size(egui::vec2(16.0, 16.0)),
                            );
                            ui.vertical(|ui| {
                                ui.set_max_width(320.0);
                                let message = ui
                                    .add(egui::Label::new(RichText::new(error).size(13.0)).wrap());
                                crate::ui_probe::record(ui.ctx(), "error-notice", message.rect);
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
                            let dismiss = theme::icon_button(ui, theme::Icon::Close, "Dismiss");
                            crate::ui_probe::record(ui.ctx(), "error-dismiss", dismiss.rect);
                            if dismiss.clicked() {
                                self.error = None;
                            }
                        });
                    });
                });
        }
        self.ribbon(ui);
        self.sidebar(ui);
        self.details(ui);
        self.content(ui);
        if let Some(query) = self.editor.search_request.take() {
            self.search = query;
            self.show_left = true;
            self.searching = true;
            self.show_bookmarks = false;
            self.search_focus = true;
            self.editor.focus = false;
        }
        if !ctx.input(|input| input.pointer.any_down())
            && (self.graph.take_changed() || local_graph_open != self.graph.local_open)
            && let Err(error) = self.write_session()
        {
            self.error = Some(format!("Graph settings weren’t saved: {error}"));
        }
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
        crate::ui_probe::frame_time(&ctx, frame_started);
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

impl NotesApp {
    /// The Agent terminal lives in the right sidebar beside the open note.
    fn open_agent(&mut self) {
        if self.vault.is_none() {
            self.error = Some("Open a vault before using the agent.".into());
            return;
        }
        self.show_right = true;
        self.right_tab = 3;
        if self.sidebar_widths[1] < 360.0 {
            self.sidebar_widths[1] = 380.0;
            self.restore_sidebars[1] = true;
        }
        self.agent.focus();
        self.editor.focus = false;
        self.page_preview.close();
        self.ctx.request_repaint();
    }
}
