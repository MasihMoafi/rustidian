use crate::{
    file_tree::{FileAction, Tree},
    preview,
    theme::{self, Icon, icon_button},
};
use eframe::egui::{
    self, Align, Color32, FontId, Key, Layout, Margin, RichText, ScrollArea, Stroke, TextEdit,
};
use egui_commonmark::CommonMarkCache;
use rust_notes::tab::{Tab, View};
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
    Settings,
}
#[derive(Clone, Copy)]
enum Pending {
    CloseTab(usize),
    Quit,
    ChangeVault,
}

pub struct NotesApp {
    pub(crate) vault: Option<Vault>,
    tree: Tree,
    pub(crate) tabs: Vec<Tab>,
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
    dialog_selected: usize,
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
    pending_anchor: Option<String>,
    properties: crate::properties_ui::Properties,
    pub(crate) sort_descending: bool,
    collapse_epoch: u64,
    reveal_folder: Option<PathBuf>,
    reset_tree_scroll: bool,
    pub(crate) bookmarks: rust_notes::bookmarks::Bookmarks,
    pub(crate) show_bookmarks: bool,
    pub(crate) right_tab: u8,
    pub(crate) ui_zoom: f32,
    pub(crate) autosave_enabled: bool,
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
            dialog_selected: 0,
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
            pending_anchor: None,
            properties: Default::default(),
            sort_descending: false,
            collapse_epoch: 0,
            reveal_folder: None,
            reset_tree_scroll: false,
            bookmarks: Default::default(),
            show_bookmarks: false,
            right_tab: 0,
            ui_zoom: 1.0,
            autosave_enabled: true,
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
            tab.view != View::Read && !tab.document.is_blank() && !tab.document.is_image()
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
            match result {
                Ok(vault) => {
                    let opening = self.loading_root.take().is_some();
                    if opening {
                        self.bookmarks = Default::default();
                        self.right_tab = 0;
                        self.ui_zoom = 1.0;
                        self.autosave_enabled = true;
                        ctx.set_zoom_factor(1.0);
                        self.show_bookmarks = false;
                        self.sort_descending = false;
                        self.collapse_epoch = 0;
                        self.reveal_folder = None;
                        self.reset_tree_scroll = true;
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
                        self.vault = Some(vault);
                        if !self.restore_session(ctx)
                            && let Some(path) = first
                        {
                            self.open_note(&path);
                        }
                        if let Some(path) = self.startup_document.take() {
                            self.open_note_at(&path, true);
                        }
                    } else {
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
                    self.loading_root = None;
                    self.error = Some(error);
                }
            }
        }
        let mut saves = Vec::new();
        for (index, tab) in self.tabs.iter().enumerate() {
            if self.autosave_enabled
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
    }

    fn open_note(&mut self, path: &Path) {
        self.open_note_at(path, false);
    }

    fn open_note_at(&mut self, path: &Path, new_tab: bool) {
        if !new_tab
            && self
                .tabs
                .get(self.active)
                .is_some_and(|t| t.document.path == path)
        {
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
            self.focus_editor();
            return;
        }
        let Some(vault) = &self.vault else {
            return;
        };
        match vault.open_document(path) {
            Ok(document) => {
                if new_tab || self.tabs.is_empty() {
                    self.tabs.push(Tab::new(document));
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
                    tab.history = history;
                    self.cache = CommonMarkCache::default();
                    self.focus_editor();
                }
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn follow_link(&mut self, target: &str) {
        if target.starts_with("https://")
            || target.starts_with("http://")
            || target.starts_with("mailto:")
        {
            self.ctx.open_url(egui::OpenUrl::new_tab(target));
            return;
        }
        let new_tab = self.ctx.input(|input| {
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
        let Some(from) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.document.path.clone())
        else {
            return;
        };
        let result = self
            .vault
            .as_mut()
            .map(|vault| vault.open_link(&from, target));
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
        self.tabs.push(Tab::new(Document::blank()));
        self.active = self.tabs.len() - 1;
        self.editor.focus = false;
    }

    fn ask(&mut self, dialog: Dialog) {
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
        if self.tabs[index].document.dirty() && !self.save(index) {
            self.pending = Some(Pending::CloseTab(index));
        } else {
            self.close_tab(index);
        }
    }

    fn close_tab(&mut self, index: usize) {
        self.tabs.remove(index);
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len().saturating_sub(1));
        self.focus_editor();
    }

    fn run_pending(&mut self, pending: Pending, ctx: &egui::Context) {
        match pending {
            Pending::CloseTab(index) => self.close_tab(index),
            Pending::Quit => {
                self.persist_session();
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Pending::ChangeVault => self.ask(Dialog::OpenVault),
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
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
        if self.loading_root.is_some() {
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
                    .is_some_and(|tab| !tab.document.is_image())
            {
                self.find.open = true;
                self.find.focus = true;
                self.editor.focus = false;
            }
            if ctx.input_mut(|i| {
                i.consume_key(egui::Modifiers::CTRL | egui::Modifiers::SHIFT, Key::Tab)
            }) && !self.tabs.is_empty()
            {
                self.active = (self.active + self.tabs.len() - 1) % self.tabs.len();
                self.focus_editor();
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Tab))
                && !self.tabs.is_empty()
            {
                self.active = (self.active + 1) % self.tabs.len();
                self.focus_editor();
            }
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Q)) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::T)) {
            self.new_tab();
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

                if icon_button(ui, Icon::NewNote, "New note · Ctrl+N").clicked() {
                    self.ask(Dialog::NewNote);
                }
                ui.allocate_ui_with_layout(
                    ui.available_size(),
                    Layout::bottom_up(Align::Center),
                    |ui| {
                        if icon_button(ui, Icon::Commands, "Command palette · Ctrl+P").clicked() {
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
            egui::Button::new(RichText::new("Bookmark current file").size(13.0)).frame(false),
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

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        if !self.show_left {
            return;
        }
        let mut selected = None;
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
                        let files = icon_button(ui, Icon::Files, "File explorer");
                        if files.clicked() {
                            self.searching = false;
                            self.show_bookmarks = false;
                        }
                        let search = icon_button(ui, Icon::Search, "Search notes");
                        if search.clicked() {
                            self.searching = true;
                            self.show_bookmarks = false;
                            self.search_focus = true;
                        }
                        let bookmarks = icon_button(ui, Icon::Bookmark, "Bookmarks");
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
                            if icon_button(ui, Icon::NewNote, "New note · Ctrl+N").clicked() {
                                self.ask(Dialog::NewNote);
                            }
                            let new_folder = icon_button(ui, Icon::NewFolder, "New folder");
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
                                                .frame(false),
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
                        ui.separator();
                        if ui.button(RichText::new("Open vault…").size(13.0)).clicked() {
                            self.change_vault();
                        }
                        ui.label(RichText::new("Ctrl+O  Quick switcher").size(11.0).weak());
                    },
                );
            });
        if let Some(action) = selected {
            match action {
                FileAction::Open(path) => {
                    self.open_note_at(&path, ui.ctx().input(|i| i.modifiers.command))
                }
                FileAction::Rename(path) => self.ask_file(Dialog::RenameNote, path),
                FileAction::Trash(path) => self.ask_file(Dialog::TrashNote, path),
                FileAction::Bookmark(path) => self.add_bookmark(&path),
            }
        }
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        if !self.show_right || ui.available_width() < 700.0 {
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
                                .frame(false),
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
                                .frame(false),
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
                if self.loading_root.is_some() {
                    ui.centered_and_justified(|ui| {
                        ui.label("Opening vault…");
                    });
                    return;
                }
                self.tab_strip(ui);
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
                                });
                            },
                        );
                    });
                self.document_view(ui);
            });
    }

    fn document_view(&mut self, ui: &mut egui::Ui) {
        let view = self.view();
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
            .with(("note-editor", tab.id(), &tab.document.path));
        crate::editor_ui::find_bar(ui, &mut self.find, tab, id);
        let mut follow = None;
        let anchor = self.pending_anchor.take();
        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        ui.spacing_mut().scroll.bar_width = 6.0;
        ui.spacing_mut().scroll.bar_inner_margin = 4.0;
        ui.spacing_mut().scroll.bar_outer_margin = 2.0;
        let mut add_property = false;
        let title = tab.document.title();
        ScrollArea::vertical()
            .id_salt(("document", tab.id(), &tab.document.path))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let width = ui.available_width();
                let content_width = width.min(920.0).min((width - 48.0).max(100.0));
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
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(title)
                                            .color(theme::heading(ui, 1))
                                            .font(FontId::new(
                                                32.436,
                                                egui::FontFamily::Name("bold".into()),
                                            ))
                                            .line_height(Some(38.9232)),
                                    )
                                    .selectable(false),
                                );
                                ui.add_space(16.218);
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
        if add_property {
            self.ask(Dialog::AddProperty);
        }
        if let Some(target) = follow {
            self.follow_link(&target);
        }
    }

    fn tab_strip(&mut self, ui: &mut egui::Ui) {
        let mut close = None;
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
                                            let show_close =
                                                active || ui.rect_contains_pointer(rect);
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
                                                tab.document.title(),
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
                                                .on_hover_text("Close tab · Ctrl+W");
                                            if show_close {
                                                theme::icon_image(
                                                    Icon::Close,
                                                    ui.visuals().weak_text_color(),
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
                                                        "close-tab:{}",
                                                        tab.document.path.display()
                                                    ),
                                                    cross,
                                                );
                                            }
                                            if closing.clicked() {
                                                close = Some(index);
                                            } else if response.clicked() {
                                                self.active = index;
                                                self.editor.focus = tab.view != View::Read;
                                            }
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
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(pending) = self.pending {
            let response = egui::Modal::new(egui::Id::unique("unsaved-changes")).show(ctx, |ui| {
                ui.set_width(440.0);
                ui.heading("Keep your changes?");
                ui.label("You have unsaved edits. Save them before continuing, or discard them explicitly.");
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.button("Save & continue").clicked() {
                        let indices: Vec<usize> = match pending {
                            Pending::CloseTab(index) => vec![index],
                            _ => self.tabs.iter().enumerate().filter(|(_, tab)| tab.document.dirty()).map(|(i, _)| i).collect(),
                        };
                        if indices.into_iter().all(|index| self.save(index)) {
                            self.pending = None; self.run_pending(pending, ctx);
                        }
                    }
                    if ui.button("Discard").clicked() { self.pending = None; self.run_pending(pending, ctx); }
                    if ui.button("Cancel").clicked() { self.pending = None; }
                });
                if let Some(error) = &self.error { ui.colored_label(Color32::from_rgb(235, 157, 132), error); }
            });
            if response.should_close() {
                self.pending = None;
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
        let navigate_down = matches!(dialog, Dialog::QuickOpen | Dialog::Commands)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
        let navigate_up = matches!(dialog, Dialog::QuickOpen | Dialog::Commands)
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
        let mut select = None;
        let mut run_command = None;
        let response = egui::Modal::new(egui::Id::unique("main-dialog")).show(ctx, |ui| {
            ui.set_width(if matches!(dialog, Dialog::Commands | Dialog::Settings) {
                650.0
            } else {
                500.0
            });
            ui.style_mut()
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(14.0));
            ui.style_mut().text_styles.insert(
                egui::TextStyle::Heading,
                egui::FontId::new(20.0, egui::FontFamily::Name("semibold".into())),
            );
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
                ui.label(
                    RichText::new(error)
                        .size(13.0)
                        .color(Color32::from_rgb(235, 157, 132)),
                );
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
                                .frame(false)
                                .min_size(egui::vec2(ui.available_width(), 34.0)),
                            );
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
                Dialog::Commands => {
                    let image = self
                        .tabs
                        .get(self.active)
                        .is_some_and(|tab| tab.document.is_image());
                    let matches = rust_notes::commands::ranked(&self.dialog_input, image);
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
                                    .frame(false)
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
                Dialog::Settings => {
                    let before = (self.dark, self.ui_zoom, self.autosave_enabled);
                    let query = self.dialog_input.to_lowercase();
                    let show = |terms: &str| query.is_empty() || terms.contains(query.as_str());
                    let mut changed = false;
                    if show("appearance theme dark light interface zoom text size") {
                        ui.strong("Appearance");
                        let dark = ui.checkbox(&mut self.dark, "Dark theme");
                        crate::ui_probe::record(ctx, "settings-dark", dark.rect);
                        changed |= dark.changed();
                        ui.horizontal(|ui| {
                            ui.label("Interface zoom");
                            let zoom = ui.add(
                                egui::Slider::new(&mut self.ui_zoom, 0.75..=1.5).step_by(0.05),
                            );
                            crate::ui_probe::record(ctx, "settings-zoom", zoom.rect);
                            changed |= zoom.changed();
                        });
                    }
                    if show("editor editing files save autosave") {
                        ui.separator();
                        ui.strong("Editor");
                        let autosave =
                            ui.checkbox(&mut self.autosave_enabled, "Autosave after 600 ms");
                        crate::ui_probe::record(ctx, "settings-autosave", autosave.rect);
                        changed |= autosave.changed();
                    }
                    if changed {
                        if let Err(error) = self.write_session() {
                            (self.dark, self.ui_zoom, self.autosave_enabled) = before;
                            self.error = Some(format!("Settings weren’t saved: {error}"));
                        } else {
                            self.status = "Settings saved".into();
                            if self
                                .error
                                .as_deref()
                                .is_some_and(|error| error.starts_with("Settings weren’t saved:"))
                            {
                                self.error = None;
                            }
                        }
                        theme::apply(ctx, self.dark);
                        ctx.set_zoom_factor(self.ui_zoom);
                        ctx.request_repaint();
                    }
                    if show("hotkeys shortcuts commands") || !query.is_empty() {
                        ui.separator();
                        ui.strong("Keyboard shortcuts");
                        ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                            for command in rust_notes::commands::ranked(&self.dialog_input, false) {
                                if !command.shortcut.is_empty() {
                                    ui.horizontal(|ui| {
                                        ui.label(command.name);
                                        ui.with_layout(
                                            Layout::right_to_left(Align::Center),
                                            |ui| {
                                                ui.weak(command.shortcut);
                                            },
                                        );
                                    });
                                }
                            }
                        });
                    }
                    if ui.button("Close").clicked() {
                        close = true;
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
        if let Some(command) = run_command {
            self.dialog = None;
            self.command(command, ctx);
            return;
        }
        if let Some(path) = select {
            self.open_note_at(&path, open_in_new_tab || ctx.input(|i| i.modifiers.command));
            close = true;
        }
        if submit {
            close = self.submit_dialog(dialog);
        }
        if close {
            self.dialog = None;
            if matches!(dialog, Dialog::RenameNote | Dialog::TrashNote) {
                if !submit && self.tabs.iter().all(|tab| !tab.document.dirty()) {
                    self.error = None;
                }
                self.focus_editor();
            }
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
                    self.bookmarks.rename(&source, &outcome.path);
                    for tab in &mut self.tabs {
                        tab.history.rename_path(&source, &outcome.path);
                        tab.document.path = rust_notes::paths::moved_path(
                            &tab.document.path,
                            &source,
                            &outcome.path,
                            true,
                        );
                        if let Some(vault) = &self.vault
                            && !tab.document.is_blank()
                            && let Ok(document) = vault.open_document(&tab.document.path)
                        {
                            tab.replace(document);
                        }
                    }
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
                    self.tabs.retain(|tab| tab.document.path != source);
                    self.active = self.active.min(self.tabs.len().saturating_sub(1));
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
            return true;
        }
        if input.is_empty() {
            self.error = Some("Enter a name or folder path first.".into());
            return false;
        }
        if dialog == Dialog::OpenVault {
            // Validate before dropping any open tabs.
            if let Err(error) = Vault::open(Path::new(&input)) {
                self.error = Some(error);
                return false;
            }
            self.load_vault(Path::new(&input));
            return true;
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
        let highlight = rust_notes::highlight::Color::ALL
            .into_iter()
            .find(|color| color.command() == command)
            .or_else(|| {
                (command == "Highlight selection").then_some(rust_notes::highlight::Color::Yellow)
            });
        if let Some(color) = highlight {
            if let Some(tab) = self
                .tabs
                .get(self.active)
                .filter(|tab| !tab.document.is_image() && tab.view != View::Read)
            {
                self.editor.highlight(tab, color);
                ctx.request_repaint();
            } else {
                self.error = Some("Choose an editable Markdown note to highlight text.".into());
            }
            return;
        }
        match command {
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
            "Open a file" => self.ask(Dialog::QuickOpen),
            "Open vault" => self.change_vault(),
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
}

impl eframe::App for NotesApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.background(&ctx);
        self.shortcuts(&ctx);
        let title = format!(
            "{} — {} — Rust Notes",
            self.tabs
                .get(self.active)
                .map(|tab| tab.document.title())
                .unwrap_or("New tab".into()),
            self.vault.as_ref().map(Vault::name).unwrap_or_default()
        );
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
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
                            .is_some_and(|tab| tab.document.is_image())
                        {
                            ui.label(RichText::new("Image").size(11.0).weak());
                            return;
                        }
                        ui.label(
                            RichText::new(match self.view() {
                                View::Read => "Reading",
                                View::Live => "Live preview",
                                View::Source => "Source mode",
                                View::Split => "Split view",
                            })
                            .size(11.0)
                            .color(theme::accent(ui)),
                        );
                        if let Some(document) = self.tabs.get(self.active).map(|tab| &tab.document)
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
                    ui.colored_label(
                        Color32::from_rgb(235, 157, 132),
                        RichText::new(error).size(13.0),
                    );
                    if ui.button("Dismiss").clicked() {
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
        self.dialogs(&ctx);
        // Apply this frame's text events before saving. Ctrl+S can arrive in
        // the same frame as the final characters of a fast typing burst.
        if self.pending.is_none()
            && self.loading_root.is_none()
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
