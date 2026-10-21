use crate::{theme, ui_probe};
use eframe::egui::{self, Align, FontId, Layout, RichText, ScrollArea, TextEdit};
use rust_notes::{
    recovery::{Change, History, Version},
    recovery_capture::Recorder,
    vault::{Document, Vault},
};
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
};

#[derive(Clone)]
pub struct Restore {
    pub expected: Document,
    pub version: u64,
}

struct Loaded {
    files: Vec<PathBuf>,
    path: Option<PathBuf>,
    versions: Vec<Version>,
    selected: Option<u64>,
    text: Option<String>,
    changes: Vec<(Change, String)>,
}

#[derive(Default)]
pub struct Manager {
    pub open: bool,
    history: Option<History>,
    files: Vec<PathBuf>,
    matcher: Option<rust_notes::search::FileMatcher>,
    path: Option<PathBuf>,
    versions: Vec<Version>,
    selected: Option<u64>,
    text: Option<String>,
    changes: Vec<(Change, String)>,
    expected: Option<Document>,
    input: String,
    choosing_file: bool,
    show_changes: bool,
    loading: Option<Receiver<Result<Loaded, String>>>,
    clearing: Option<Receiver<Result<(), String>>>,
    confirm_clear: bool,
    error: Option<String>,
    notice: Option<String>,
    refresh_due: bool,
    focus: bool,
    selected_file: usize,
    pick_when_ready: bool,
}

impl Manager {
    pub fn changed(&mut self, ctx: &egui::Context) {
        if self.open {
            self.refresh_due = true;
            ctx.request_repaint();
        }
    }
    pub fn active(&self) -> bool {
        self.open || self.confirm_clear || self.clearing.is_some()
    }
    pub fn open(
        &mut self,
        history: History,
        vault: &Vault,
        path: Option<PathBuf>,
        ctx: &egui::Context,
    ) {
        *self = Self {
            open: true,
            history: Some(history),
            focus: true,
            ..Default::default()
        };
        self.select_file(path, vault, ctx);
    }

    pub fn request_clear(&mut self) {
        self.confirm_clear = true;
    }

    pub fn error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    pub fn restored(&mut self, document: Document, ctx: &egui::Context) {
        self.expected = Some(document);
        self.notice = Some("Snapshot restored".into());
        self.reload(ctx);
    }

    fn select_file(&mut self, path: Option<PathBuf>, vault: &Vault, ctx: &egui::Context) {
        self.expected = path.as_ref().and_then(|path| {
            vault.read(path).ok().or_else(|| {
                matches!(vault.root.join(path).try_exists(), Ok(false))
                    .then(|| Document::new(path.clone()))
            })
        });
        self.input = path
            .as_ref()
            .map(|p| rust_notes::paths::display_path(p))
            .unwrap_or_default();
        self.path = path;
        self.selected = None;
        self.text = None;
        self.versions.clear();
        self.changes.clear();
        self.selected_file = 0;
        self.choosing_file = self.path.is_none();
        self.show_changes = false;
        self.reload(ctx);
    }

    fn reload(&mut self, ctx: &egui::Context) {
        let Some(history) = self.history.clone() else {
            return;
        };
        let path = self.path.clone();
        let selected = self.selected;
        let current = self.expected.as_ref().map(|document| document.text.clone());
        let (sender, receiver) = mpsc::channel();
        self.loading = Some(receiver);
        self.error = None;
        // A failed version query must never leave another version's preview
        // copyable or present it as the newly selected snapshot.
        self.text = None;
        self.changes.clear();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| {
                let files = history.files()?;
                let versions = path
                    .as_ref()
                    .map(|path| history.versions(path))
                    .transpose()?
                    .unwrap_or_default();
                let selected = selected
                    .filter(|id| versions.iter().any(|version| version.id == *id))
                    .or_else(|| versions.first().map(|version| version.id));
                let text = match (&path, selected) {
                    (Some(path), Some(id)) => Some(history.content(path, id)?),
                    _ => None,
                };
                let changes = text
                    .as_ref()
                    .map(|text| {
                        rust_notes::recovery::changes(text, current.as_deref().unwrap_or_default())
                    })
                    .unwrap_or_default();
                Ok(Loaded {
                    files,
                    path,
                    versions,
                    selected,
                    text,
                    changes,
                })
            })();
            let _ = sender.send(result);
            ctx.request_repaint();
        });
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        if let Some(receiver) = &self.loading {
            match receiver.try_recv() {
                Ok(Ok(loaded)) => {
                    self.matcher = Some(rust_notes::search::FileMatcher::new(loaded.files.clone()));
                    self.files = loaded.files;
                    self.path = loaded.path;
                    self.versions = loaded.versions;
                    self.selected = loaded.selected;
                    self.text = loaded.text;
                    self.changes = loaded.changes;
                    self.loading = None;
                }
                Ok(Err(error)) => {
                    self.error = Some(error);
                    self.loading = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("Cannot read local history.".into());
                    self.loading = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.clearing {
            match receiver.try_recv() {
                Ok(result) => {
                    self.clearing = None;
                    match result {
                        Ok(()) => {
                            self.files.clear();
                            self.matcher = None;
                            self.versions.clear();
                            self.selected = None;
                            self.text = None;
                            self.changes.clear();
                            self.notice = Some("History cleared".into());
                        }
                        Err(error) => self.error = Some(error),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.clearing = None;
                    self.error = Some("Cannot clear local history.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.refresh_due
            && self.loading.is_none()
            && self.clearing.is_none()
            && !self.confirm_clear
        {
            self.refresh_due = false;
            self.reload(ctx);
        }
        if self.loading.is_some() || self.clearing.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        vault: &Vault,
        recorder: &Recorder,
    ) -> Option<Restore> {
        self.poll(ctx);
        let mut restore = None;
        let mut select_file = None;
        let mut select_version = None;
        let mut close = false;
        if self.open {
            let modal = egui::Modal::new(egui::Id::unique("file-recovery")).show(ctx, |ui| {
                theme::dialog_style(ui);
                ui.set_width(786.0_f32.min((ctx.content_rect().width() - 48.0).max(280.0)));
                let height = (ctx.content_rect().height() - 190.0).clamp(220.0, 580.0);
                let busy = self.loading.is_some() || self.clearing.is_some();
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(225.0_f32.min(ui.available_width() * 0.35), height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_width(ui.max_rect().width());
                            let input_id = ui.make_persistent_id("recovery-file-input");
                            if self.focus {
                                ui.memory_mut(|memory| memory.request_focus(input_id));
                            }
                            let input = ui.add(
                                TextEdit::singleline(&mut self.input)
                                    .id(input_id)
                                    .return_key(None)
                                    .hint_text("Choose a file...")
                                    .desired_width(ui.available_width()),
                            );
                            if self.focus {
                                input.request_focus();
                                self.focus = ui.is_sizing_pass();
                            }
                            ui_probe::record(ctx, "recovery-files", input.rect);
                            if input.changed() {
                                self.choosing_file = true;
                                self.selected_file = 0;
                            }
                            ui.add_space(8.0);
                            let scroll = ScrollArea::vertical()
                                .id_salt(("recovery-versions", &self.path, self.choosing_file))
                                .max_height(height - 50.0)
                                .min_scrolled_height(height - 50.0);
                            if !self.choosing_file && !self.versions.is_empty() {
                                scroll.show_rows(ui, 46.0, self.versions.len(), |ui, rows| {
                                    for version in &self.versions[rows] {
                                        let time = chrono::DateTime::from_timestamp_millis(
                                            version.timestamp_ms,
                                        )
                                        .map(|time| {
                                            time.with_timezone(&chrono::Local)
                                                .format("%a, %b %-d, %Y %-I:%M %p")
                                                .to_string()
                                        })
                                        .unwrap_or_else(|| "Unknown date".into());
                                        let size = if version.bytes < 1024 {
                                            format!("{} B", version.bytes)
                                        } else {
                                            format!("{:.2} KB", version.bytes as f64 / 1024.0)
                                        };
                                        let button = ui.add_enabled(
                                            !busy,
                                            egui::Button::selectable(
                                                self.selected == Some(version.id),
                                                format!("{time}\n{size}"),
                                            )
                                            .min_size(egui::vec2(ui.available_width(), 46.0))
                                            .truncate(),
                                        );
                                        ui_probe::record(
                                            ctx,
                                            format!("recovery-version:{}", version.id),
                                            button.rect.intersect(ui.clip_rect()),
                                        );
                                        if button.clicked() {
                                            select_version = Some(version.id);
                                        }
                                    }
                                });
                            } else {
                                scroll.show(ui, |ui| {
                                    if self.choosing_file {
                                        let matches = self
                                            .matcher
                                            .as_mut()
                                            .map(|matcher| matcher.rank(&self.input, 100))
                                            .unwrap_or_default();
                                        let mut moved = false;
                                        if ui.input_mut(|i| {
                                            i.consume_key(
                                                egui::Modifiers::NONE,
                                                egui::Key::ArrowDown,
                                            )
                                        }) {
                                            self.selected_file = (self.selected_file + 1)
                                                .min(matches.len().saturating_sub(1));
                                            moved = true;
                                        }
                                        if ui.input_mut(|i| {
                                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)
                                        }) {
                                            self.selected_file =
                                                self.selected_file.saturating_sub(1);
                                            moved = true;
                                        }
                                        self.selected_file =
                                            self.selected_file.min(matches.len().saturating_sub(1));
                                        if ui.input_mut(|i| {
                                            i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                                        }) {
                                            self.pick_when_ready = true;
                                        }
                                        if self.pick_when_ready && !busy {
                                            self.pick_when_ready = false;
                                            select_file = matches.get(self.selected_file).cloned();
                                        }
                                        for (index, path) in matches.iter().enumerate() {
                                            let button = ui.add_enabled(
                                                !busy,
                                                egui::Button::selectable(
                                                    index == self.selected_file,
                                                    path.display().to_string(),
                                                )
                                                .min_size(egui::vec2(ui.available_width(), 32.0))
                                                .wrap(),
                                            );
                                            ui_probe::record(
                                                ctx,
                                                format!("recovery-file:{}", path.display()),
                                                button.rect.intersect(ui.clip_rect()),
                                            );
                                            if button.clicked() {
                                                select_file = Some(path.clone());
                                            }
                                            if index == self.selected_file
                                                && (moved || input.changed())
                                            {
                                                button.scroll_to_me(Some(Align::Center));
                                            }
                                        }
                                    }
                                    if self.files.is_empty() && !busy {
                                        ui.label("No snapshots found.");
                                    }
                                });
                            }
                        },
                    );
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let name = self
                                .path
                                .as_ref()
                                .and_then(|path| path.file_stem())
                                .map(|name| name.to_string_lossy())
                                .unwrap_or_default();
                            ui.add_sized(
                                egui::vec2((ui.available_width() - 320.0).max(20.0), 24.0),
                                egui::Label::new(RichText::new(name.as_ref()).strong()).truncate(),
                            )
                            .on_hover_text(name.as_ref());
                            if busy {
                                ui.spinner();
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let button = theme::icon_button(ui, theme::Icon::Close, "Close");
                                ui_probe::record(ctx, "recovery-close", button.rect);
                                close = button.clicked();
                                let button = ui.add_enabled(
                                    !busy && self.text.is_some() && self.expected.is_some(),
                                    egui::Button::new("Restore"),
                                );
                                ui_probe::record(ctx, "recovery-restore", button.rect);
                                if button.clicked()
                                    && let (Some(expected), Some(version)) =
                                        (&self.expected, self.selected)
                                {
                                    restore = Some(Restore {
                                        expected: expected.clone(),
                                        version,
                                    });
                                }
                                let button = ui.add_enabled(
                                    !busy && self.text.is_some(),
                                    egui::Button::new("Copy"),
                                );
                                ui_probe::record(ctx, "recovery-copy", button.rect);
                                if !busy {
                                    ui_probe::record(ctx, "recovery-loaded", button.rect);
                                }
                                if !busy && self.text.is_some() {
                                    ui_probe::record(ctx, "recovery-ready", button.rect);
                                    if let Some(version) = self.selected {
                                        ui_probe::record(
                                            ctx,
                                            format!("recovery-selected:{version}"),
                                            button.rect,
                                        );
                                    }
                                }
                                if button.clicked()
                                    && let Some(text) = &self.text
                                {
                                    ctx.copy_text(text.clone());
                                    self.notice = Some("Copied to your clipboard".into());
                                }
                                let changes = ui.add_enabled(
                                    !busy && self.text.is_some(),
                                    egui::Checkbox::new(&mut self.show_changes, "Show changes"),
                                );
                                ui_probe::record(ctx, "recovery-changes", changes.rect);
                            });
                        });
                        ui.separator();
                        ScrollArea::both()
                            .id_salt(("recovery-text", self.selected))
                            .max_height(height - 48.0)
                            .min_scrolled_height(height - 48.0)
                            .show(ui, |ui| {
                                let response = if self.show_changes {
                                    let mut job = egui::text::LayoutJob::default();
                                    for (change, text) in &self.changes {
                                        let background = match change {
                                            Change::Context => egui::Color32::TRANSPARENT,
                                            Change::Removed => {
                                                if ui.visuals().dark_mode {
                                                    egui::Color32::from_rgb(70, 33, 35)
                                                } else {
                                                    egui::Color32::from_rgb(255, 219, 219)
                                                }
                                            }
                                            Change::Added => {
                                                if ui.visuals().dark_mode {
                                                    egui::Color32::from_rgb(29, 65, 46)
                                                } else {
                                                    egui::Color32::from_rgb(211, 243, 219)
                                                }
                                            }
                                        };
                                        job.append(
                                            text,
                                            0.0,
                                            egui::TextFormat {
                                                font_id: FontId::proportional(18.0),
                                                color: ui.visuals().text_color(),
                                                background,
                                                ..Default::default()
                                            },
                                        );
                                    }
                                    ui.label(job)
                                } else {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(self.text.as_deref().unwrap_or_default())
                                                .size(18.0),
                                        )
                                        .selectable(true)
                                        .wrap(),
                                    )
                                };
                                ui_probe::record(
                                    ctx,
                                    "recovery-text",
                                    response.rect.intersect(ui.clip_rect()),
                                );
                            });
                    });
                });
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    ui_probe::record(ctx, "recovery-error", ui.min_rect());
                }
                if let Some(notice) = &self.notice {
                    ui.label(notice);
                }
            });
            ui_probe::record(ctx, "recovery-dialog", modal.response.rect);
            if (close || modal.should_close()) && !self.confirm_clear {
                self.open = false;
                self.loading = None;
                self.refresh_due = false;
            }
        }
        if self.confirm_clear {
            let mut cancel = false;
            let mut clear = false;
            let modal = egui::Modal::new(egui::Id::unique("clear-local-history")).show(ctx, |ui| {
                theme::dialog_style(ui);
                ui.set_width(390.0);
                ui.heading("Clear history?");
                ui.label("This will permanently delete all local snapshots for this vault.");
                ui.horizontal(|ui| {
                    let button = ui.button("Cancel");
                    ui_probe::record(ctx, "recovery-clear-cancel", button.rect);
                    cancel = button.clicked();
                    let button = ui.button("Clear");
                    ui_probe::record(ctx, "recovery-clear-confirm", button.rect);
                    clear = button.clicked();
                });
            });
            if cancel || modal.should_close() {
                self.confirm_clear = false;
            }
            if clear {
                self.confirm_clear = false;
                self.loading = None;
                match recorder.clear() {
                    Ok(receiver) => self.clearing = Some(receiver),
                    Err(error) => self.error = Some(error),
                }
            }
        }
        if let Some(path) = select_file {
            self.select_file(Some(path), vault, ctx);
        }
        if let Some(id) = select_version {
            self.selected = Some(id);
            self.reload(ctx);
        }
        restore
    }
}
