use crate::{theme, ui_probe};
use eframe::egui::{self, Align, Layout, RichText, ScrollArea, TextEdit};
use nucleo_matcher::{
    Config, Matcher,
    pattern::{AtomKind, CaseMatching, Normalization, Pattern},
};
use rustidian::workspaces::{Layout as SavedLayout, Workspaces};
use std::path::Path;

pub struct Manager {
    pub open: bool,
    list: Result<Workspaces, String>,
    input: String,
    focus: bool,
    load_only: bool,
    selected: usize,
    error: Option<String>,
}

impl Default for Manager {
    fn default() -> Self {
        Self {
            open: false,
            list: Err(String::new()),
            input: String::new(),
            focus: false,
            load_only: false,
            selected: 0,
            error: None,
        }
    }
}

impl Manager {
    pub fn show_manager(&mut self, root: &Path, load_only: bool) {
        self.list = Workspaces::open(root);
        self.open = true;
        self.input.clear();
        self.focus = true;
        self.load_only = load_only;
        self.selected = 0;
        self.error = None;
    }

    pub fn show_error(&mut self, error: String) {
        self.error = Some(error);
    }

    pub fn show(&mut self, ctx: &egui::Context, layout: &SavedLayout) -> Option<String> {
        let mut load = None;
        let mut save = false;
        let mut remove = None;
        let mut close = false;
        let id = egui::Id::unique(if self.load_only {
            "workspace-picker"
        } else {
            "workspace-manager"
        });
        let mut modal = egui::Modal::new(id);
        if self.load_only {
            modal = modal.area(
                egui::Modal::default_area(id)
                    .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 80.0)),
            );
        }
        let response = modal.show(ctx, |ui| {
            theme::dialog_style(ui);
            let width: f32 = if self.load_only { 686.0 } else { 546.0 };
            ui.set_width(width.min((ctx.content_rect().width() - 48.0).max(240.0)));
            if !self.load_only {
                ui.horizontal(|ui| {
                    ui.heading("Manage workspace layouts");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let button = theme::icon_button(ui, theme::Icon::Close, "Close");
                        ui_probe::record(ctx, "workspace-close", button.rect);
                        close = button.clicked();
                    });
                });
                ui.add_space(12.0);
            }
            ui.horizontal(|ui| {
                let input = ui.add(
                    TextEdit::singleline(&mut self.input)
                        .hint_text(if self.load_only {
                            "Type to search..."
                        } else {
                            "Save current workspace layout as..."
                        })
                        .desired_width(
                            (ui.available_width() - if self.load_only { 0.0 } else { 66.0 })
                                .max(160.0),
                        ),
                );
                ui_probe::record(ctx, "workspace-name", input.rect);
                if self.focus {
                    input.request_focus();
                    self.focus = false;
                }
                if input.changed() {
                    self.selected = 0;
                }
                if !self.load_only {
                    let button = ui.add_enabled(
                        self.list.is_ok() && !self.input.trim().is_empty(),
                        egui::Button::new("Save").min_size(egui::vec2(54.0, 30.0)),
                    );
                    ui_probe::record(ctx, "workspace-save", button.rect);
                    save = button.clicked()
                        || ((input.has_focus() || input.lost_focus())
                            && ui.input_mut(|i| {
                                i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)
                            })
                            && !self.input.trim().is_empty());
                }
            });
            ui.add_space(12.0);
            match &self.list {
                Err(error) => {
                    let response = ui.colored_label(ui.visuals().error_fg_color, error);
                    ui_probe::record(ctx, "workspace-error", response.rect);
                }
                Ok(list) => {
                    let entries: Vec<_> = if self.load_only {
                        Pattern::new(
                            &self.input,
                            CaseMatching::Ignore,
                            Normalization::Smart,
                            AtomKind::Fuzzy,
                        )
                        .match_list(list.entries().iter(), &mut Matcher::new(Config::DEFAULT))
                        .into_iter()
                        .map(|(entry, _)| entry)
                        .collect()
                    } else {
                        list.entries().iter().collect()
                    };
                    self.selected = self.selected.min(entries.len().saturating_sub(1));
                    if self.load_only {
                        if ui.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
                        }) {
                            self.selected =
                                (self.selected + 1).min(entries.len().saturating_sub(1));
                        }
                        if ui
                            .input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
                        {
                            self.selected = self.selected.saturating_sub(1);
                        }
                        if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                        {
                            load = entries.get(self.selected).map(|entry| entry.name.clone());
                        }
                    }
                    if entries.is_empty() && self.load_only {
                        ui.label(if list.entries().is_empty() {
                            "No saved workspace layouts."
                        } else {
                            "No matching layouts."
                        });
                    }
                    let max_height = (ctx.content_rect().height() - 240.0).clamp(90.0, 360.0);
                    let row_height = if self.load_only { 40.0 } else { 68.0 };
                    ScrollArea::vertical()
                        .id_salt(if self.load_only {
                            "workspace-picker-rows"
                        } else {
                            "workspace-manager-rows"
                        })
                        .min_scrolled_height((entries.len() as f32 * row_height).min(max_height))
                        .max_height(max_height)
                        .show(ui, |ui| {
                            for (index, entry) in entries.iter().enumerate() {
                                if self.load_only {
                                    let button =
                                        egui::Button::selectable(index == self.selected, "");
                                    let button = if list.active() == Some(entry.name.as_str()) {
                                        button.left_text((
                                            RichText::new(&entry.name),
                                            RichText::new("ACTIVE")
                                                .small()
                                                .color(theme::accent(ui)),
                                        ))
                                    } else {
                                        button.left_text(&entry.name)
                                    };
                                    let row = ui.add(
                                        button
                                            .min_size(egui::vec2(ui.available_width(), 32.0))
                                            .corner_radius(4),
                                    );
                                    ui_probe::record(
                                        ctx,
                                        format!("workspace-load:{}", entry.name),
                                        row.rect.intersect(ui.clip_rect()),
                                    );
                                    if row.clicked() {
                                        load = Some(entry.name.clone());
                                    }
                                } else {
                                    ui.horizontal(|ui| {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(
                                                (ui.available_width() - 112.0).max(100.0),
                                                40.0,
                                            ),
                                            Layout::top_down(Align::Min),
                                            |ui| {
                                                ui.set_min_width(ui.max_rect().width());
                                                let name = ui.add(
                                                    egui::Label::new(
                                                        RichText::new(&entry.name).strong(),
                                                    )
                                                    .truncate(),
                                                );
                                                name.on_hover_text(&entry.name);
                                                ui.horizontal(|ui| {
                                                    let age = (chrono::Local::now()
                                                        .timestamp_millis()
                                                        - entry.modified_ms)
                                                        .max(0)
                                                        / 60_000;
                                                    ui.label(
                                                        RichText::new(if age == 0 {
                                                            "Modified just now".into()
                                                        } else {
                                                            format!("Modified {age} min ago")
                                                        })
                                                        .small()
                                                        .weak(),
                                                    );
                                                    if list.active() == Some(entry.name.as_str()) {
                                                        ui.label(
                                                            RichText::new("Active")
                                                                .small()
                                                                .color(theme::accent(ui)),
                                                        );
                                                    }
                                                });
                                            },
                                        );
                                        let button = ui.button("Load");
                                        ui_probe::record(
                                            ctx,
                                            format!("workspace-load:{}", entry.name),
                                            button.rect.intersect(ui.clip_rect()),
                                        );
                                        if button.clicked() {
                                            load = Some(entry.name.clone());
                                        }
                                        let delete = theme::icon_button(
                                            ui,
                                            theme::Icon::Close,
                                            "Delete layout",
                                        );
                                        ui_probe::record(
                                            ctx,
                                            format!("workspace-delete:{}", entry.name),
                                            delete.rect.intersect(ui.clip_rect()),
                                        );
                                        if delete.clicked() {
                                            remove = Some(entry.name.clone());
                                        }
                                    });
                                    ui.separator();
                                }
                            }
                        });
                    if self.load_only {
                        ui.separator();
                        ui.label(
                            RichText::new("↑↓ to navigate    ↵ to open    esc to dismiss")
                                .small()
                                .weak(),
                        );
                    }
                }
            }
            if let Some(error) = &self.error {
                let response = ui.colored_label(ui.visuals().error_fg_color, error);
                ui_probe::record(ctx, "workspace-error", response.rect);
            }
        });
        ui_probe::record(ctx, "workspace-manager", response.response.rect);
        if close || response.should_close() {
            self.open = false;
            load = None;
        }
        if save {
            let result = self
                .list
                .as_mut()
                .map_err(|error| error.clone())
                .and_then(|list| {
                    list.save(
                        &self.input,
                        layout.clone(),
                        chrono::Local::now().timestamp_millis(),
                    )
                });
            match result {
                Ok(()) => {
                    self.input.clear();
                    self.error = None;
                    self.focus = true;
                }
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(name) = remove {
            let result = self
                .list
                .as_mut()
                .map_err(|error| error.clone())
                .and_then(|list| list.remove(&name));
            match result {
                Ok(()) => self.error = None,
                Err(error) => self.error = Some(error),
            }
        }
        load
    }
}
