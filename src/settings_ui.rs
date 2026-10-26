use super::*;

#[derive(Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct EditorPreferences {
    pub reading_view: bool,
    pub source_mode: bool,
    pub show_mode: bool,
    pub inline_title: bool,
    pub readable_line_length: bool,
}

impl Default for EditorPreferences {
    fn default() -> Self {
        Self {
            reading_view: false,
            source_mode: false,
            show_mode: true,
            inline_title: true,
            readable_line_length: true,
        }
    }
}

impl EditorPreferences {
    pub fn apply(&self, tab: &mut Tab) {
        if !tab.document.is_image() && !tab.document.is_plain_text() {
            tab.set_view(if self.source_mode {
                View::Source
            } else {
                View::Live
            });
            if self.reading_view {
                tab.set_view(View::Read);
            }
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    Appearance,
    Interface,
    Editor,
    Files,
    Hotkeys,
    Daily,
    Templates,
    Unique,
    Recovery,
}

impl Page {
    const ALL: [Self; 9] = [
        Self::Appearance,
        Self::Interface,
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
            Self::Interface => "Interface",
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
            Self::Appearance => "appearance base color scheme theme dark light font text size",
            Self::Interface => "interface zoom",
            Self::Editor => {
                "editor editing files save autosave default view source live preview inline title readable line length status mode"
            }
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
    fn editor_settings(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        group(ui, |ui| {
            changed |= choice(
                ui,
                "Default view for new tabs",
                "The default view that a new Markdown tab gets opened in.",
                "settings-default-view",
                &mut self.editor_preferences.reading_view,
                ["Editing view", "Reading view"],
            );
            ui.separator();
            changed |= choice(
                ui,
                "Default editing mode",
                "The default editing mode a new tab will start with.",
                "settings-editing-mode",
                &mut self.editor_preferences.source_mode,
                ["Live Preview", "Source mode"],
            );
            ui.separator();
            changed |= toggle(
                ui,
                "Show editing mode in status bar",
                "Show the editing mode in the status bar.",
                "settings-show-mode",
                &mut self.editor_preferences.show_mode,
            );
        });
        ui.add_space(20.0);
        ui.strong("Display");
        ui.add_space(10.0);
        group(ui, |ui| {
            changed |= toggle(
                ui,
                "Inline title",
                "Display the filename as an editable title inline with the file contents.",
                "settings-inline-title",
                &mut self.editor_preferences.inline_title,
            );
            ui.separator();
            changed |= toggle(
                ui,
                "Readable line length",
                "Limit maximum line length. Less content fits onscreen, but long blocks of text are more readable.",
                "settings-readable-line-length",
                &mut self.editor_preferences.readable_line_length,
            );
        });
        ui.add_space(20.0);
        ui.strong("Saving");
        ui.add_space(10.0);
        group(ui, |ui| {
            changed |= toggle(
                ui,
                "Autosave",
                "Save changes after 600 ms without typing.",
                "settings-autosave",
                &mut self.autosave_enabled,
            );
        });
        changed
    }

    pub(super) fn settings_ui(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        close: &mut bool,
        recovery_action: &mut Option<bool>,
    ) {
        theme::dialog_style(ui);
        ui.style_mut().spacing.slider_width = 120.0;
        ui.style_mut().visuals.faint_bg_color = if self.dark {
            Color32::from_rgb(35, 35, 35)
        } else {
            Color32::from_rgb(249, 249, 249)
        };
        let width = (ctx.content_rect().width() - 48.0).clamp(360.0, 900.0);
        let height = (ctx.content_rect().height() - 100.0).clamp(260.0, 672.0);
        ui.set_width(width);
        let before = (
            self.dark,
            self.palette,
            self.ui_zoom,
            self.document_font_size,
            self.editor_preferences.clone(),
            self.autosave_enabled,
            self.note_aids.clone(),
            self.recovery_config.clone(),
            self.always_update_links,
            self.confirm_trash,
        );
        let mut changed = false;
        ui.horizontal_top(|ui| {
            egui::Frame::new()
                .fill(if self.dark {
                    Color32::from_rgb(38, 38, 38)
                } else {
                    Color32::from_rgb(246, 246, 246)
                })
                .inner_margin(12)
                .show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.set_width(if width < 600.0 { 152.0 } else { 188.0 });
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.set_min_height(height - 24.0);
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
                                        ui.add_space(12.0);
                                        ui.weak("Core plugins");
                                        ui.add_space(4.0);
                                    }
                                    if page.matches(&query) {
                                        let response =
                                            navigation_row(ui, page, page == self.settings_page);
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
                ui.set_width((ui.available_width() - 24.0).max(140.0));
                ui.set_min_height(height);
                ui.add_space(24.0);
                let button = theme::dialog_header(ui, "Settings", "Close settings");
                crate::ui_probe::record(ctx, "settings-close", button.rect);
                *close |= button.clicked();
                ui.add_space(12.0);
                let query = self.dialog_input.to_lowercase();
                if !self.settings_page.matches(&query) {
                    ui.weak("No settings found.");
                    return;
                }
                ScrollArea::vertical()
                    .id_salt(("settings-content", self.settings_page as u8))
                    .max_height(height - 100.0)
                    .show(ui, |ui| {
                        if self.settings_page == Page::Editor {
                            changed |= self.editor_settings(ui);
                            return;
                        }
                        if self.settings_page == Page::Appearance {
                            changed |= theme::palette::picker(ui, &mut self.palette, self.dark);
                        }
                        group(ui, |ui| match self.settings_page {
                            Page::Appearance => {
                                changed |=
                                    row(
                                        ui,
                                        "Base color scheme",
                                        "Choose the default color scheme.",
                                        |ui| {
                                            let before = self.dark;
                                            let response = egui::ComboBox::from_id_salt(
                                                "settings-color-scheme",
                                            )
                                            .icon(combo_chevron)
                                            .selected_text(if self.dark { "Dark" } else { "Light" })
                                            .show_ui(ui, |ui| {
                                                theme::menu_style(ui);
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
                                    "Font size",
                                    "Font size in pixels that affects editing and reading views.",
                                    |ui| {
                                        let response = theme::slider(
                                            ui,
                                            egui::Slider::new(&mut self.document_font_size, 10..=30),
                                        );
                                        crate::ui_probe::record(
                                            ctx,
                                            "settings-font-size",
                                            response.rect,
                                        );
                                        response.changed()
                                    },
                                );
                            }
                            Page::Interface => {
                                changed |= row(
                                    ui,
                                    "Zoom level",
                                    "Scale the interface and document together.",
                                    |ui| {
                                        let response = theme::slider(
                                            ui,
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
                            Page::Editor => unreachable!(),
                            Page::Files => {
                                changed |= toggle(
                                    ui,
                                    "Automatically update internal links",
                                    "Update links when moving or renaming files.",
                                    "settings-update-move-links",
                                    &mut self.always_update_links,
                                );
                                ui.separator();
                                changed |= toggle(
                                    ui,
                                    "Confirm before moving to Trash",
                                    "Show a confirmation before moving files or folders to the system Trash.",
                                    "settings-confirm-trash",
                                    &mut self.confirm_trash,
                                );
                            }
                            Page::Hotkeys => {
                                for command in rustidian::commands::ranked("", Scope::Editor) {
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
                                changed |=
                                    row(ui, "History length", "Days to keep snapshots.", |ui| {
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
                                    });
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
                                        let view = ui.add_enabled(ready, egui::Button::new("View"));
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
                    self.palette,
                    self.ui_zoom,
                    self.document_font_size,
                    self.editor_preferences,
                    self.autosave_enabled,
                    self.note_aids,
                    self.recovery_config,
                    self.always_update_links,
                    self.confirm_trash,
                ) = before;
                self.error = Some(format!("Settings weren’t saved: {error}"));
            } else {
                self.settings_saved();
            }
            theme::palette::select(ctx, self.palette);
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
    ui.add_space(12.0);
    let label_width = (ui.available_width() - 180.0).max(100.0);
    let changed = ui
        .horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(label_width, 0.0),
                Layout::top_down(Align::LEFT),
                |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(title);
                    if !description.is_empty() {
                        ui.label(RichText::new(description).size(13.0).weak());
                    }
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), control)
                .inner
        })
        .inner;
    ui.add_space(12.0);
    changed
}

fn toggle(ui: &mut egui::Ui, title: &str, description: &str, id: &str, value: &mut bool) -> bool {
    row(ui, title, description, |ui| {
        let response = switch(ui, value);
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

fn navigation_row(ui: &mut egui::Ui, page: Page, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 25.0), egui::Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Button, ui.is_enabled(), selected, page.label())
    });
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            6,
            if selected {
                Color32::from_gray(if ui.visuals().dark_mode { 45 } else { 216 })
            } else {
                theme::hover_fill(ui)
            },
        );
    }
    let icon = match page {
        Page::Appearance => Icon::Appearance,
        Page::Interface => Icon::SidebarLeft,
        Page::Editor => Icon::Edit,
        Page::Files => Icon::Files,
        Page::Hotkeys => Icon::Commands,
        Page::Daily => Icon::DailyNote,
        Page::Templates => Icon::Templates,
        Page::Unique => Icon::NewNote,
        Page::Recovery => Icon::Recovery,
    };
    theme::icon_image(icon, ui.visuals().weak_text_color()).paint_at(
        ui,
        egui::Rect::from_center_size(
            egui::pos2(rect.left() + 16.0, rect.center().y),
            egui::vec2(16.0, 16.0),
        ),
    );
    ui.painter().text(
        egui::pos2(rect.left() + 32.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        page.label(),
        FontId::proportional(13.0),
        ui.visuals().text_color(),
    );
    response
}

fn switch(ui: &mut egui::Ui, value: &mut bool) -> egui::Response {
    let (rect, mut response) = ui.allocate_exact_size(egui::vec2(40.0, 22.0), egui::Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::Switch, ui.is_enabled(), *value, "")
    });
    let fill = if *value {
        theme::accent(ui)
    } else {
        Color32::from_gray(if ui.visuals().dark_mode { 63 } else { 190 })
    };
    ui.painter().rect_filled(rect, 11, fill);
    ui.painter().circle_filled(
        egui::pos2(
            if *value {
                rect.right() - 11.0
            } else {
                rect.left() + 11.0
            },
            rect.center().y,
        ),
        9.0,
        Color32::WHITE,
    );
    if response.has_focus() {
        ui.painter().rect_stroke(
            rect.expand(2.0),
            13,
            Stroke::new(1.0, theme::accent(ui)),
            egui::StrokeKind::Outside,
        );
    }
    response
}

fn choice(
    ui: &mut egui::Ui,
    title: &str,
    description: &str,
    id: &str,
    value: &mut bool,
    labels: [&str; 2],
) -> bool {
    row(ui, title, description, |ui| {
        let before = *value;
        let response = egui::ComboBox::from_id_salt(id)
            .icon(combo_chevron)
            .selected_text(labels[usize::from(*value)])
            .show_ui(ui, |ui| {
                theme::menu_style(ui);
                for (i, label) in labels.into_iter().enumerate() {
                    let r = ui.selectable_value(value, i == 1, label);
                    crate::ui_probe::record(ui.ctx(), format!("{id}:{label}"), r.rect);
                }
            });
        crate::ui_probe::record(ui.ctx(), id, response.response.rect);
        *value != before
    })
}

fn combo_chevron(
    ui: &egui::Ui,
    rect: egui::Rect,
    visuals: &egui::style::WidgetVisuals,
    open: bool,
) {
    theme::icon_image(Icon::Disclosure, visuals.text_color())
        .rotate(
            if open {
                -std::f32::consts::FRAC_PI_2
            } else {
                std::f32::consts::FRAC_PI_2
            },
            egui::Vec2::splat(0.5),
        )
        .paint_at(
            ui,
            egui::Rect::from_center_size(rect.center(), egui::vec2(12.0, 12.0)),
        );
}

fn group<R>(ui: &mut egui::Ui, contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(ui.visuals().faint_bg_color)
        .corner_radius(12)
        .inner_margin(egui::Margin::symmetric(20, 0))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            contents(ui)
        })
        .inner
}
