use super::*;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    Appearance,
    Editor,
    Files,
    Hotkeys,
    Daily,
    Templates,
    Unique,
    Recovery,
}

impl Page {
    const ALL: [Self; 8] = [
        Self::Appearance,
        Self::Editor,
        Self::Files,
        Self::Hotkeys,
        Self::Daily,
        Self::Templates,
        Self::Unique,
        Self::Recovery,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Editor => "Editor",
            Self::Files => "Files and links",
            Self::Hotkeys => "Hotkeys",
            Self::Daily => "Daily notes",
            Self::Templates => "Templates",
            Self::Unique => "Unique note creator",
            Self::Recovery => "File recovery",
        }
    }

    fn matches(self, query: &str) -> bool {
        let terms = match self {
            Self::Appearance => {
                "appearance base color scheme theme dark light interface zoom text size"
            }
            Self::Editor => "editor editing files save autosave",
            Self::Files => "files and links automatically update move folder",
            Self::Hotkeys => "hotkeys keyboard shortcuts commands",
            Self::Daily => "daily notes date format folder template new file location",
            Self::Templates => "templates date time format folder location",
            Self::Unique => "unique notes prefix format folder template new file location",
            Self::Recovery => "file recovery snapshots history interval length days",
        };
        query.split_whitespace().all(|word| terms.contains(word))
    }
}

impl NotesApp {
    pub(super) fn settings_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        close: &mut bool,
        recovery_action: &mut Option<bool>,
    ) {
        theme::dialog_style(ui);
        let width = (ctx.content_rect().width() - 48.0).clamp(360.0, 880.0);
        let height = (ctx.content_rect().height() - 100.0).clamp(260.0, 680.0);
        ui.set_width(width);
        let before = (
            self.dark,
            self.ui_zoom,
            self.document_font_size,
            self.autosave_enabled,
            self.note_aids.clone(),
            self.recovery_config.clone(),
            self.always_update_links,
        );
        let mut changed = false;
        ui.horizontal_top(|ui| {
            egui::Frame::new()
                .fill(if self.dark {
                    theme::DARK_SIDE
                } else {
                    Color32::from_rgb(238, 239, 234)
                })
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(if width < 600.0 { 152.0 } else { 184.0 });
                        ui.set_min_height(height);
                        let input = ui.add(
                            TextEdit::singleline(&mut self.dialog_input)
                                .hint_text("Search settings…")
                                .desired_width(f32::INFINITY),
                        );
                        crate::ui_probe::record(ctx, "dialog-input", input.rect);
                        if self.dialog_focus {
                            input.request_focus();
                            self.dialog_focus = false;
                        }
                        if input.has_focus() {
                            crate::ui_probe::record(ctx, "dialog-input-focused", input.rect);
                        }
                        let query = self.dialog_input.to_lowercase();
                        if !query.is_empty()
                            && !self.settings_page.matches(&query)
                            && let Some(page) =
                                Page::ALL.into_iter().find(|page| page.matches(&query))
                        {
                            self.settings_page = page;
                        }
                        ui.add_space(14.0);
                        ScrollArea::vertical()
                            .id_salt("settings-navigation")
                            .max_height(height - 60.0)
                            .show(ui, |ui| {
                                for page in Page::ALL {
                                    if page == Page::Appearance {
                                        ui.weak("Options");
                                        ui.add_space(4.0);
                                    }
                                    if page == Page::Daily {
                                        ui.add_space(18.0);
                                        ui.weak("Core plugins");
                                        ui.add_space(4.0);
                                    }
                                    if page.matches(&query) {
                                        let response = ui.add_sized(
                                            [ui.available_width(), 28.0],
                                            egui::Button::new(page.label())
                                                .selected(page == self.settings_page)
                                                .frame(false),
                                        );
                                        crate::ui_probe::record(
                                            ctx,
                                            format!("settings-page:{}", page.label()),
                                            response.rect,
                                        );
                                        if response.clicked() {
                                            self.settings_page = page;
                                        }
                                    }
                                }
                            });
                    });
                });
            ui.add_space(16.0);
            ui.vertical(|ui| {
                ui.set_width((width - if width < 600.0 { 212.0 } else { 244.0 }).max(140.0));
                ui.set_min_height(height);
                ui.horizontal(|ui| {
                    ui.heading(self.settings_page.label());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let button = icon_button(ui, Icon::Close, "Close settings");
                        crate::ui_probe::record(ctx, "settings-close", button.rect);
                        *close |= button.clicked();
                    });
                });
                ui.add_space(18.0);
                let query = self.dialog_input.to_lowercase();
                if !self.settings_page.matches(&query) {
                    ui.weak("No settings found.");
                    return;
                }
                ScrollArea::vertical()
                    .id_salt(("settings-content", self.settings_page as u8))
                    .max_height(height - 100.0)
                    .show(ui, |ui| {
                        egui::Frame::new()
                            .fill(ui.visuals().faint_bg_color)
                            .corner_radius(10)
                            .inner_margin(16)
                            .show(ui, |ui| match self.settings_page {
                                Page::Appearance => {
                                    changed |= row(
                                        ui,
                                        "Base color scheme",
                                        "Choose the default color scheme.",
                                        |ui| {
                                            let before = self.dark;
                                            let response = egui::ComboBox::from_id_salt(
                                                "settings-color-scheme",
                                            )
                                            .selected_text(if self.dark { "Dark" } else { "Light" })
                                            .show_ui(ui, |ui| {
                                                for (dark, label) in
                                                    [(true, "Dark"), (false, "Light")]
                                                {
                                                    let response = ui.selectable_value(
                                                        &mut self.dark,
                                                        dark,
                                                        label,
                                                    );
                                                    crate::ui_probe::record(
                                                        ctx,
                                                        format!("settings-theme:{}", label),
                                                        response.rect,
                                                    );
                                                }
                                            });
                                            crate::ui_probe::record(
                                                ctx,
                                                "settings-dark",
                                                response.response.rect,
                                            );
                                            self.dark != before
                                        },
                                    );
                                    ui.separator();
                                    changed |= row(
                                        ui,
                                        "Interface zoom",
                                        "Scale the interface and document together.",
                                        |ui| {
                                            let response = ui.add(
                                                egui::Slider::new(&mut self.ui_zoom, 0.75..=1.5)
                                                    .step_by(0.05),
                                            );
                                            crate::ui_probe::record(
                                                ctx,
                                                "settings-zoom",
                                                response.rect,
                                            );
                                            response.changed()
                                        },
                                    );
                                }
                                Page::Editor => {
                                    changed |= toggle(
                                        ui,
                                        "Autosave",
                                        "Save changes after 600 ms without typing.",
                                        "settings-autosave",
                                        &mut self.autosave_enabled,
                                    );
                                }
                                Page::Files => {
                                    changed |= toggle(
                                        ui,
                                        "Automatically update internal links",
                                        "Update links when moving or renaming files.",
                                        "settings-update-move-links",
                                        &mut self.always_update_links,
                                    );
                                }
                                Page::Hotkeys => {
                                    for command in rust_notes::commands::ranked("", Scope::Editor) {
                                        if !command.shortcut.is_empty() {
                                            row(ui, command.name, "", |ui| {
                                                ui.weak(command.shortcut);
                                                false
                                            });
                                            ui.separator();
                                        }
                                    }
                                }
                                Page::Daily => {
                                    changed |= text(
                                        ui,
                                        "Date format",
                                        "How daily note file names are formatted.",
                                        "settings-daily-format",
                                        &mut self.note_aids.daily.format,
                                        "YYYY-MM-DD",
                                    );
                                    ui.separator();
                                    changed |= text(
                                        ui,
                                        "New file location",
                                        "Folder for new daily notes.",
                                        "settings-daily-folder",
                                        &mut self.note_aids.daily.folder,
                                        "Vault folder",
                                    );
                                    ui.separator();
                                    changed |= text(
                                        ui,
                                        "Template file location",
                                        "A note to use as the daily template.",
                                        "settings-daily-template",
                                        &mut self.note_aids.daily.template,
                                        "Optional template note",
                                    );
                                }
                                Page::Templates => {
                                    changed |= text(
                                        ui,
                                        "Template folder location",
                                        "Folder containing your templates.",
                                        "settings-template-folder",
                                        &mut self.note_aids.templates.folder,
                                        "Choose a vault folder",
                                    );
                                    ui.separator();
                                    changed |= text(
                                        ui,
                                        "Date format",
                                        "Format for the date variable.",
                                        "settings-template-date",
                                        &mut self.note_aids.templates.date_format,
                                        "YYYY-MM-DD",
                                    );
                                    ui.separator();
                                    changed |= text(
                                        ui,
                                        "Time format",
                                        "Format for the time variable.",
                                        "settings-template-time",
                                        &mut self.note_aids.templates.time_format,
                                        "HH:mm",
                                    );
                                }
                                Page::Unique => {
                                    changed |= text(
                                        ui,
                                        "New file location",
                                        "Folder for unique notes.",
                                        "settings-unique-folder",
                                        &mut self.note_aids.unique.folder,
                                        "Vault folder",
                                    );
                                    ui.separator();
                                    changed |= text(
                                        ui,
                                        "Template file location",
                                        "A note to use as the template.",
                                        "settings-unique-template",
                                        &mut self.note_aids.unique.template,
                                        "Optional template note",
                                    );
                                    ui.separator();
                                    changed |= text(
                                        ui,
                                        "Unique prefix format",
                                        "Date format for unique note names.",
                                        "settings-unique-format",
                                        &mut self.note_aids.unique.format,
                                        "YYYYMMDDHHmm",
                                    );
                                }
                                Page::Recovery => {
                                    changed |= row(
                                        ui,
                                        "Snapshot interval",
                                        "Minutes between recovery snapshots.",
                                        |ui| {
                                            let response = ui.add(
                                                egui::DragValue::new(
                                                    &mut self.recovery_config.interval_minutes,
                                                )
                                                .range(0.0..=f64::MAX)
                                                .speed(0.1),
                                            );
                                            crate::ui_probe::record(
                                                ctx,
                                                "settings-recovery-interval",
                                                response.rect,
                                            );
                                            response.changed()
                                        },
                                    );
                                    ui.separator();
                                    changed |= row(
                                        ui,
                                        "History length",
                                        "Days to keep snapshots.",
                                        |ui| {
                                            let response = ui.add(
                                                egui::DragValue::new(
                                                    &mut self.recovery_config.keep_days,
                                                )
                                                .range(1.0..=f64::MAX),
                                            );
                                            crate::ui_probe::record(
                                                ctx,
                                                "settings-recovery-days",
                                                response.rect,
                                            );
                                            response.changed()
                                        },
                                    );
                                    ui.separator();
                                    row(
                                        ui,
                                        "Snapshots",
                                        "Review or clear the saved recovery history.",
                                        |ui| {
                                            let ready = !self.recovery_manager.active();
                                            let clear =
                                                ui.add_enabled(ready, egui::Button::new("Clear"));
                                            crate::ui_probe::record(
                                                ctx,
                                                "settings-recovery-clear",
                                                clear.rect,
                                            );
                                            if clear.clicked() {
                                                *recovery_action = Some(true);
                                            }
                                            let view =
                                                ui.add_enabled(ready, egui::Button::new("View"));
                                            crate::ui_probe::record(
                                                ctx,
                                                "settings-recovery-view",
                                                view.rect,
                                            );
                                            if ready {
                                                crate::ui_probe::record(
                                                    ctx,
                                                    "settings-recovery-view-ready",
                                                    view.rect,
                                                );
                                            }
                                            if view.clicked() {
                                                *recovery_action = Some(false);
                                            }
                                            false
                                        },
                                    );
                                }
                            });
                        if self.settings_page == Page::Hotkeys {
                            ui.add_space(12.0);
                            ui.weak("Shortcut customization is not available yet.");
                        }
                    });
                if let Some(error) = &self.error {
                    ui.add_space(12.0);
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    crate::ui_probe::record(ctx, "dialog-error", ui.min_rect());
                }
            });
        });
        if changed {
            if let Err(error) = self.write_session() {
                (
                    self.dark,
                    self.ui_zoom,
                    self.document_font_size,
                    self.autosave_enabled,
                    self.note_aids,
                    self.recovery_config,
                    self.always_update_links,
                ) = before;
                self.error = Some(format!("Settings weren’t saved: {error}"));
            } else {
                self.settings_saved();
            }
            theme::apply(ctx, self.dark);
            ctx.set_zoom_factor(self.ui_zoom);
            ctx.request_repaint();
        }
    }
}

fn row(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    control: impl FnOnce(&mut egui::Ui) -> bool,
) -> bool {
    ui.add_space(8.0);
    let label_width = (ui.available_width() * 0.56).min(320.0);
    let changed = ui
        .horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 0.0),
                Layout::top_down(Align::LEFT),
                |ui| {
                    ui.label(title);
                    if !description.is_empty() {
                        ui.add_space(3.0);
                        ui.label(RichText::new(description).size(12.0).weak());
                    }
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), control)
                .inner
        })
        .inner;
    ui.add_space(8.0);
    changed
}

fn toggle(ui: &mut egui::Ui, title: &str, description: &str, id: &str, value: &mut bool) -> bool {
    row(ui, title, description, |ui| {
        let response = theme::checkbox(ui, value);
        crate::ui_probe::record(ui.ctx(), id, response.rect);
        response.changed()
    })
}

fn text(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    id: &str,
    value: &mut String,
    hint: &str,
) -> bool {
    row(ui, title, description, |ui| {
        let response = ui.add(
            TextEdit::singleline(value)
                .id_salt(id)
                .hint_text(hint)
                .desired_width(ui.available_width()),
        );
        crate::ui_probe::record(ui.ctx(), id, response.rect);
        response.changed()
    })
}
