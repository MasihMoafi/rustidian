use crate::{theme, ui_probe};
use eframe::egui::{self, Align2, FontId, Key, Margin, RichText, Stroke, TextEdit};
use rust_notes::{operations::LinkUpdates, search::FileMatcher, vault::Vault};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Manager {
    picker: Option<Picker>,
    updates: Option<LinkUpdates>,
    error: Option<String>,
}

struct Picker {
    source: PathBuf,
    query: String,
    folders: FileMatcher,
    paths: Vec<PathBuf>,
    selected: usize,
    focus: bool,
}

pub enum Action {
    Move {
        source: PathBuf,
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
        self.picker.is_some() || self.updates.is_some()
    }

    pub fn open(&mut self, vault: &Vault, source: PathBuf) {
        let current = source.parent().unwrap_or_else(|| Path::new(""));
        let mut folders = FileMatcher::folders(vault, current);
        let paths = folders.rank_folders("", usize::MAX);
        self.picker = Some(Picker {
            folders,
            paths,
            source,
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
    }

    pub fn error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
        if let Some(updates) = &self.updates {
            let (links, files) = updates.counts();
            let mut choice = None;
            let response =
                egui::Modal::new(egui::Id::unique("move-update-links")).show(ctx, |ui| {
                    ui.set_width((ctx.content_rect().width() - 56.0).clamp(220.0, 526.0));
                    ui.spacing_mut().item_spacing.y = 16.0;
                    ui.heading("Update links");
                    ui.label("Do you want to update internal links that link to this file?");
                    ui.label(format!(
                        "This will affect {links} {} in {files} {}.",
                        if links == 1 { "link" } else { "links" },
                        if files == 1 { "file" } else { "files" },
                    ));
                    ui.horizontal_wrapped(|ui| {
                        for (label, name, value) in [
                            ("Always update", "move-links-always", Some(true)),
                            ("Just once", "move-links-once", Some(false)),
                            ("Do not update", "move-links-no", None),
                        ] {
                            let button = ui.button(label);
                            ui_probe::record(ctx, name, button.rect);
                            if button.clicked() {
                                choice = Some(value);
                            }
                        }
                    });
                    ui_probe::record(ctx, "move-links-dialog", ui.max_rect());
                });
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
        let frame = egui::Frame::popup(&ctx.global_style())
            .inner_margin(Margin::same(12))
            .corner_radius(16);
        let response = egui::Modal::new(egui::Id::unique("move-file-picker"))
            .area(area)
            .frame(frame)
            .show(ctx, |ui| {
                ui.set_width(width - 24.0);
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
                            .font(FontId::proportional(15.0))
                            .hint_text("Type a folder")
                            .frame(egui::Frame::NONE)
                            .desired_width((width - 70.0).max(100.0)),
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
                        source: picker.source.clone(),
                        folder: if query == "/" {
                            PathBuf::new()
                        } else {
                            PathBuf::from(query)
                        },
                        create: true,
                    });
                } else if enter && let Some(folder) = paths.get(picker.selected) {
                    action = Some(Action::Move {
                        source: picker.source.clone(),
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
                                    RichText::new(&label).size(15.0).strong()
                                } else {
                                    RichText::new(&label).size(15.0)
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
                                source: picker.source.clone(),
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
                            .left_text(RichText::new(query).size(15.0))
                            .right_text(RichText::new("Enter to create").size(12.0))
                            .selected(true)
                            .stroke(Stroke::NONE),
                    );
                    ui_probe::record(ctx, "move-create-folder", button.rect);
                    if button.clicked() {
                        action = Some(Action::Move {
                            source: picker.source.clone(),
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
