use crate::{theme, ui_probe};
use eframe::egui::{self, Align2, Key, RichText, Stroke, TextEdit};
use rustidian::{operations::LinkUpdates, search::FileMatcher, vault::Vault};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Manager {
    picker: Option<Picker>,
    trash: Option<Vec<PathBuf>>,
    trash_focus: bool,
    updates: Option<LinkUpdates>,
    updates_focus: bool,
    error: Option<String>,
}

struct Picker {
    sources: Vec<PathBuf>,
    query: String,
    folders: FileMatcher,
    paths: Vec<PathBuf>,
    selected: usize,
    focus: bool,
}

pub enum Action {
    Trash(Vec<PathBuf>),
    Move {
        sources: Vec<PathBuf>,
        folder: PathBuf,
        create: bool,
    },
    Update {
        updates: LinkUpdates,
        always: bool,
    },
}

impl Manager {
    pub fn active(&self) -> bool {
        self.picker.is_some() || self.updates.is_some() || self.trash.is_some()
    }

    pub fn open(&mut self, vault: &Vault, source: PathBuf) {
        self.open_many(vault, vec![source]);
    }

    pub fn open_many(&mut self, vault: &Vault, sources: Vec<PathBuf>) {
        let current = sources
            .first()
            .and_then(|source| source.parent())
            .unwrap_or_else(|| Path::new(""));
        let mut folders = FileMatcher::folders(vault, current);
        let paths = folders.rank_folders("", usize::MAX);
        self.picker = Some(Picker {
            folders,
            paths,
            sources,
            query: String::new(),
            selected: 0,
            focus: true,
        });
        self.error = None;
    }

    pub fn moved(&mut self, updates: Option<LinkUpdates>) {
        self.picker = None;
        self.error = None;
        self.updates = updates;
        self.updates_focus = self.updates.is_some();
    }

    pub fn error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn ask_trash(&mut self, sources: Vec<PathBuf>) {
        self.trash = Some(sources);
        self.trash_focus = true;
        self.error = None;
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
        if let Some(paths) = self.trash.clone() {
            let focus = std::mem::take(&mut self.trash_focus);
            let mut confirm = false;
            let mut close = false;
            let response = egui::Modal::new(egui::Id::unique("trash-selection"))
                .frame(theme::dialog_frame(ctx))
                .show(ctx, |ui| {
                    ui.set_width((ctx.content_rect().width() - 80.0).clamp(220.0, 500.0));
                    theme::dialog_style(ui);
                    close = theme::dialog_header(ui, "Move selected items to Trash?", "Cancel")
                        .clicked();
                    ui.label(format!(
                        "Move {} selected items and any folder contents to system Trash?",
                        paths.len()
                    ));
                    egui::ScrollArea::vertical()
                        .max_height(180.0)
                        .show(ui, |ui| {
                            for path in &paths {
                                ui.label(path.display().to_string());
                            }
                        });
                    ui.label("You can recover these items from system Trash.");
                    let (submit, cancel) = theme::dialog_actions(ui, "Move to Trash", true);
                    if focus {
                        cancel.request_focus();
                    }
                    crate::ui_probe::record(ctx, "explorer-trash-cancel", cancel.rect);
                    crate::ui_probe::record(ctx, "explorer-trash-confirm", submit.rect);
                    close |= cancel.clicked();
                    confirm = submit.clicked();
                });
            if close || response.should_close() {
                self.trash = None;
            } else if confirm {
                self.trash = None;
                return Some(Action::Trash(paths));
            }
            return None;
        }
        if let Some(updates) = &self.updates {
            let (links, files) = updates.counts();
            let mut choice = None;
            let focus = std::mem::take(&mut self.updates_focus);
            let dark = ctx.global_style().visuals.dark_mode;
            let frame = theme::dialog_frame(ctx);
            let response = egui::Modal::new(egui::Id::unique("move-update-links"))
                .frame(frame)
                .show(ctx, |ui| {
                    let width = (ctx.content_rect().width() - 48.0).clamp(220.0, 560.0);
                    ui.set_width(width - frame.total_margin().sum().x);
                    theme::dialog_style(ui);
                    let close = theme::dialog_header(ui, "Update links", "Close");
                    ui_probe::record(ctx, "move-links-title", ui.min_rect());
                    ui_probe::record(ctx, "move-links-close", close.rect);
                    if close.clicked() {
                        choice = Some(None);
                    }
                    ui.label("Do you want to update internal links that link to this file?");
                    ui.label(format!(
                        "This will affect {links} {} in {files} {}.",
                        if links == 1 { "link" } else { "links" },
                        if files == 1 { "file" } else { "files" },
                    ));
                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        let choices = [
                            ("Always update", "move-links-always", Some(true)),
                            ("Just once", "move-links-once", Some(false)),
                            ("Do not update", "move-links-no", None),
                        ];
                        let required = choices
                            .iter()
                            .map(|(label, _, _)| {
                                ui.painter()
                                    .layout_no_wrap(
                                        (*label).into(),
                                        egui::TextStyle::Button.resolve(ui.style()),
                                        ui.visuals().text_color(),
                                    )
                                    .size()
                                    .x
                                    + 2.0 * ui.spacing().button_padding.x
                            })
                            .sum::<f32>()
                            + 16.0;
                        ui.add_space((ui.available_width() - required).max(0.0));
                        for (label, name, value) in choices {
                            let button =
                                ui.add(egui::Button::new(label).min_size(egui::vec2(0.0, 36.0)));
                            if focus && value == Some(true) {
                                button.request_focus();
                            }
                            ui_probe::record(ctx, name, button.rect);
                            if button.has_focus() {
                                ui_probe::record(ctx, format!("{name}-focused"), button.rect);
                                ui.painter().rect_stroke(
                                    button.rect.expand(1.5),
                                    10,
                                    Stroke::new(
                                        3.0,
                                        egui::Color32::from_gray(if dark { 85 } else { 189 }),
                                    ),
                                    egui::StrokeKind::Middle,
                                );
                            }
                            if button.clicked() {
                                choice = Some(value);
                            }
                        }
                    });
                });
            ui_probe::record(ctx, "move-links-dialog", response.response.rect);
            if response.should_close() || choice == Some(None) {
                self.updates = None;
            } else if let Some(Some(always)) = choice {
                return self
                    .updates
                    .take()
                    .map(|updates| Action::Update { updates, always });
            }
            return None;
        }
        let picker = self.picker.as_mut()?;
        let mut action = None;
        let mut close = false;
        let down = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::ArrowDown));
        let up = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::ArrowUp));
        let create = ctx.input_mut(|input| input.consume_key(egui::Modifiers::SHIFT, Key::Enter));
        let enter = ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Enter));
        let width = (ctx.content_rect().width() - 48.0).clamp(220.0, 700.0);
        let area = egui::Modal::default_area(egui::Id::unique("move-file-picker")).anchor(
            Align2::CENTER_TOP,
            [0.0, ctx.content_rect().height() * 0.15],
        );
        let frame = theme::dialog_frame(ctx);
        let response = egui::Modal::new(egui::Id::unique("move-file-picker"))
            .area(area)
            .frame(frame)
            .show(ctx, |ui| {
                theme::dialog_style(ui);
                ui.set_width(width - frame.total_margin().sum().x);
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.visuals_mut().selection.bg_fill = theme::hover_fill(ui);
                ui.visuals_mut().selection.stroke = Stroke::new(1.0, ui.visuals().text_color());
                let id = ui.make_persistent_id("move-input");
                if picker.focus {
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
                ui.horizontal(|ui| {
                    let field = ui.add(
                        TextEdit::singleline(&mut picker.query)
                            .id(id)
                            .hint_text("Type a folder")
                            .frame(egui::Frame::NONE)
                            .desired_width((ui.available_width() - 46.0).max(100.0)),
                    );
                    ui_probe::record(ctx, "move-input", field.rect);
                    if picker.focus {
                        field.request_focus();
                        picker.focus = ui.is_sizing_pass()
                            || !ui.memory(|memory| {
                                memory.has_focus(id) && memory.had_focus_last_frame(id)
                            });
                        ctx.request_repaint();
                    }
                    if field.changed() {
                        picker.selected = 0;
                        picker.paths = picker.folders.rank_folders(picker.query.trim(), usize::MAX);
                        self.error = None;
                    }
                    if theme::icon_button(ui, theme::Icon::Close, "Dismiss").clicked() {
                        close = true;
                    }
                });
                ui.separator();
                let query = picker.query.trim();
                let paths = &picker.paths;
                picker.selected = picker.selected.min(paths.len().saturating_sub(1));
                if down && !paths.is_empty() {
                    picker.selected = (picker.selected + 1) % paths.len();
                }
                if up && !paths.is_empty() {
                    picker.selected = (picker.selected + paths.len() - 1) % paths.len();
                }
                if (create || (enter && paths.is_empty())) && !query.is_empty() {
                    action = Some(Action::Move {
                        sources: picker.sources.clone(),
                        folder: if query == "/" {
                            PathBuf::new()
                        } else {
                            PathBuf::from(query)
                        },
                        create: true,
                    });
                } else if enter && let Some(folder) = paths.get(picker.selected) {
                    action = Some(Action::Move {
                        sources: picker.sources.clone(),
                        folder: folder.clone(),
                        create: false,
                    });
                }
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt("move-folders")
                    .max_height((ctx.content_rect().height() - 250.0).clamp(48.0, 336.0));
                if up || down {
                    scroll = scroll.vertical_scroll_offset(picker.selected as f32 * 32.0);
                }
                scroll.show_rows(ui, 32.0, paths.len(), |ui, rows| {
                    for index in rows {
                        let folder = &paths[index];
                        let label = if folder.as_os_str().is_empty() {
                            "/".into()
                        } else {
                            folder.display().to_string()
                        };
                        let button = ui.add_sized(
                            [ui.available_width(), 32.0],
                            egui::Button::new("")
                                .left_text(if index == picker.selected {
                                    RichText::new(&label).strong()
                                } else {
                                    RichText::new(&label)
                                })
                                .selected(index == picker.selected)
                                .stroke(Stroke::NONE),
                        );
                        ui_probe::record(ctx, format!("move-folder:{label}"), button.rect);
                        if (up || down) && index == picker.selected {
                            button.scroll_to_me(None);
                        }
                        if button.clicked() {
                            action = Some(Action::Move {
                                sources: picker.sources.clone(),
                                folder: folder.clone(),
                                create: false,
                            });
                        }
                    }
                });
                if paths.is_empty() {
                    let button = ui.add_sized(
                        [ui.available_width(), 32.0],
                        egui::Button::new("")
                            .left_text(RichText::new(query))
                            .right_text(RichText::new("Enter to create").size(12.0))
                            .selected(true)
                            .stroke(Stroke::NONE),
                    );
                    ui_probe::record(ctx, "move-create-folder", button.rect);
                    if button.clicked() {
                        action = Some(Action::Move {
                            sources: picker.sources.clone(),
                            folder: PathBuf::from(query),
                            create: true,
                        });
                    }
                    ui.add_space(12.0);
                    ui.vertical_centered(|ui| {
                        ui.weak("No existing folders found.");
                    });
                }
                if let Some(error) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                }
                ui.add_space(8.0);
                ui.horizontal_wrapped(|ui| {
                    ui.add_space((ui.available_width() - 380.0).max(0.0) / 2.0);
                    ui.label(
                        RichText::new(
                            "↑↓ to navigate   ↵ to move   shift ↵ to create   esc to dismiss",
                        )
                        .size(12.0)
                        .color(ui.visuals().weak_text_color()),
                    );
                });
                ui_probe::record(ctx, "move-picker", ui.max_rect());
            });
        if close || response.should_close() {
            self.picker = None;
        }
        action
    }
}
