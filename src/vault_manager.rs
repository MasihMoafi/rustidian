use crate::{theme, ui_probe};
use eframe::egui::{self, Align, FontId, Layout, RichText, ScrollArea, TextEdit};
use rustidian::{
    native_picker::{Kind, Picker},
    vaults::{self, Vaults},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Default, PartialEq)]
enum Page {
    #[default]
    Home,
    Open,
    Create,
}

pub struct Manager {
    pub open: bool,
    list: Result<Vaults, String>,
    page: Page,
    folder: String,
    name: String,
    error: Option<String>,
    picker: Picker,
    focus: bool,
    parent_enter_observed: bool,
}

impl Default for Manager {
    fn default() -> Self {
        Self {
            open: false,
            list: Vaults::open(),
            page: Page::Home,
            folder: String::new(),
            name: String::new(),
            error: None,
            picker: Picker::default(),
            focus: false,
            parent_enter_observed: false,
        }
    }
}

impl Manager {
    pub fn show_chooser(&mut self, current: Option<&Path>) {
        self.list = Vaults::open();
        self.open = true;
        self.page = Page::Home;
        self.folder = current
            .map(|root| root.display().to_string())
            .unwrap_or_default();
        self.name.clear();
        self.error = None;
        self.parent_enter_observed = false;
    }

    pub fn remember(&mut self, root: &Path) -> Result<(), String> {
        self.list
            .as_mut()
            .map_err(|error| error.clone())?
            .remember(root)
    }

    pub fn show_create(&mut self, current: Option<&Path>) {
        self.show_chooser(current);
        self.page = Page::Create;
        self.folder = current
            .and_then(Path::parent)
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| std::env::var("HOME").unwrap_or_default());
        self.focus = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.cancel_picker();
    }

    pub fn show_error(&mut self, error: String) {
        self.error = Some(error);
    }

    fn cancel_picker(&mut self) {
        self.picker.cancel();
    }

    fn browse(&mut self) {
        if self.picker.active() {
            return;
        }
        let title = if self.page == Page::Create {
            "Choose vault location"
        } else {
            "Open folder as vault"
        };
        if let Err(error) = self
            .picker
            .start(Kind::Folder, Some(Path::new(&self.folder)), title)
        {
            self.error = Some(format!("{error}. Enter a folder path below."));
        }
    }

    fn poll_picker(&mut self) -> Option<PathBuf> {
        match self.picker.poll()? {
            Ok(Some(path)) => {
                self.folder = path.display().to_string();
                (self.page == Page::Open).then_some(path)
            }
            Ok(None) => None,
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        current: Option<&Path>,
        loading: bool,
    ) -> Option<PathBuf> {
        if !self.open {
            return None;
        }
        if ui_probe::enabled() {
            if self.picker.active() && ctx.input(|input| input.key_pressed(egui::Key::Enter)) {
                self.parent_enter_observed = true;
            }
            ui_probe::metric(
                ctx,
                "vault-parent-enter-observed",
                if self.parent_enter_observed { 1.0 } else { 0.0 },
            );
        }
        let mut selected = self.poll_picker();
        let mut close = false;
        let mut forget = None;
        let modal =
            egui::Modal::new(egui::Id::unique("vault-manager")).frame(theme::dialog_frame(ctx));
        let response = modal.show(ctx, |ui| {
            theme::dialog_style(ui);
            ui.set_width(740.0);
            ui_probe::record(ctx, "vault-manager", ui.max_rect());
            let button = theme::dialog_header(ui, "Open another vault", "Close vault chooser");
            ui_probe::record(ctx, "vault-manager-close", button.rect);
            close |= button.clicked();
            ui.add_space(16.0);
            ui.add_enabled_ui(!loading && !self.picker.active(), |ui| {
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(250.0, 330.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.label(RichText::new("Vaults").strong());
                            ui.add_space(8.0);
                            ScrollArea::vertical()
                                .id_salt("recent-vaults")
                                .max_height(330.0)
                                .show(ui, |ui| {
                                    if let Ok(list) = &self.list {
                                        if list.roots().is_empty() {
                                            ui.label(RichText::new("No vaults opened yet.").weak());
                                        }
                                        for root in list.roots() {
                                            ui.push_id(root, |ui| {
                                                ui.horizontal(|ui| {
                                                    let name = root
                                                        .file_name()
                                                        .unwrap_or(root.as_os_str())
                                                        .to_string_lossy();
                                                    let is_current =
                                                        current == Some(root.as_path());
                                                    let mut label =
                                                        egui::text::LayoutJob::default();
                                                    label.append(
                                                        &format!("{name}\n"),
                                                        0.0,
                                                        egui::TextFormat {
                                                            font_id: FontId::proportional(14.0),
                                                            color: ui.visuals().text_color(),
                                                            ..Default::default()
                                                        },
                                                    );
                                                    label.append(
                                                        &root.display().to_string(),
                                                        0.0,
                                                        egui::TextFormat {
                                                            font_id: FontId::proportional(12.0),
                                                            color: if is_current {
                                                                ui.visuals().text_color()
                                                            } else {
                                                                ui.visuals().weak_text_color()
                                                            },
                                                            ..Default::default()
                                                        },
                                                    );
                                                    let width: f32 = 200.0;
                                                    label.wrap.max_width = (width - 16.0).max(1.0);
                                                    label.wrap.max_rows = 2;
                                                    let galley = ui
                                                        .fonts_mut(|fonts| fonts.layout_job(label));
                                                    let button = ui.add_sized(
                                                        [width, 48.0],
                                                        egui::Button::new("")
                                                            .left_text(egui::WidgetText::Galley(
                                                                galley,
                                                            ))
                                                            .selected(is_current),
                                                    );
                                                    ui_probe::record(
                                                        ctx,
                                                        format!("vault-recent:{}", root.display()),
                                                        button.rect,
                                                    );
                                                    if button.clicked() {
                                                        selected = Some(root.clone());
                                                    }
                                                    button
                                                        .on_hover_text(root.display().to_string());
                                                    let more = ui.menu_button(
                                                        theme::icon_image(
                                                            theme::Icon::More,
                                                            ui.visuals().text_color(),
                                                        ),
                                                        |ui| {
                                                            theme::menu_style(ui);
                                                            let remove =
                                                                ui.button("Remove from list");
                                                            ui_probe::record(
                                                                ctx,
                                                                format!(
                                                                    "vault-forget:{}",
                                                                    root.display()
                                                                ),
                                                                remove.rect,
                                                            );
                                                            if remove.clicked() {
                                                                forget = Some(root.clone());
                                                                ui.close();
                                                            }
                                                        },
                                                    );
                                                    ui_probe::record(
                                                        ctx,
                                                        format!("vault-more:{}", root.display()),
                                                        more.response.rect,
                                                    );
                                                });
                                            });
                                        }
                                    }
                                });
                        },
                    );
                    ui.add_sized([1.0, 330.0], egui::Separator::default().vertical());
                    ui.vertical(|ui| {
                        ui.set_width(460.0);
                        ui.set_min_height(280.0);
                        let button_fill = ui.visuals().widgets.inactive.bg_fill;
                        ui.visuals_mut().widgets.inactive.weak_bg_fill = button_fill;
                        ui.style_mut().text_styles.insert(
                            egui::TextStyle::Heading,
                            FontId::new(14.0, egui::FontFamily::Name("semibold".into())),
                        );
                        match self.page {
                            Page::Home => {
                                ui.vertical_centered(|ui| {
                                    ui.label(
                                        RichText::new(rustidian::APP_NAME).size(24.0).strong(),
                                    );
                                });
                                ui.add_space(28.0);
                                egui::Frame::group(ui.style())
                                    .inner_margin(12.0)
                                    .show(ui, |ui| {
                                        ui.horizontal_top(|ui| {
                                            ui.vertical(|ui| {
                                                ui.set_width(310.0);
                                                ui.heading("Create new vault");
                                                ui.label(
                                                    RichText::new(
                                                        "Create a new vault under a folder.",
                                                    )
                                                    .weak(),
                                                );
                                            });
                                            let create = ui.add_sized(
                                                [100.0, 30.0],
                                                egui::Button::new("Create").frame(true),
                                            );
                                            ui_probe::record(ctx, "vault-create", create.rect);
                                            if create.clicked() {
                                                self.page = Page::Create;
                                                self.folder = current
                                                    .and_then(Path::parent)
                                                    .map(|path| path.display().to_string())
                                                    .unwrap_or_else(|| {
                                                        std::env::var("HOME").unwrap_or_default()
                                                    });
                                                self.focus = true;
                                            }
                                        });
                                        ui.add_space(12.0);
                                        ui.separator();
                                        ui.add_space(12.0);
                                        ui.horizontal_top(|ui| {
                                            ui.vertical(|ui| {
                                                ui.set_width(310.0);
                                                ui.heading("Open folder as vault");
                                                ui.label(
                                                    RichText::new(
                                                        "Choose an existing folder of notes.",
                                                    )
                                                    .weak(),
                                                );
                                            });
                                            let open = ui.add_sized(
                                                [100.0, 30.0],
                                                egui::Button::new("Open").frame(true),
                                            );
                                            ui_probe::record(ctx, "vault-open", open.rect);
                                            if open.clicked() {
                                                self.page = Page::Open;
                                                self.focus = true;
                                                self.browse();
                                            }
                                        });
                                    });
                            }
                            Page::Open | Page::Create => {
                                let creating = self.page == Page::Create;
                                ui.heading(if creating {
                                    "Create vault"
                                } else {
                                    "Open folder as vault"
                                });
                                if creating {
                                    ui.label("Vault name");
                                    self.focus =
                                        text_field(ui, "vault-name", &mut self.name, self.focus);
                                    ui.add_space(12.0);
                                }
                                ui.label(if creating { "Location" } else { "Folder" });
                                let focus = text_field(
                                    ui,
                                    "vault-folder",
                                    &mut self.folder,
                                    !creating && self.focus,
                                );
                                if !creating {
                                    self.focus = focus;
                                }
                                let browse = ui.add(egui::Button::new("Browse…").frame(true));
                                ui_probe::record(ctx, "vault-browse", browse.rect);
                                if browse.clicked() {
                                    self.browse();
                                }
                                if creating {
                                    ui.add_space(8.0);
                                    ui.label(
                                        RichText::new(format!(
                                            "New vault: {}",
                                            Path::new(&self.folder).join(&self.name).display()
                                        ))
                                        .small()
                                        .weak(),
                                    );
                                }
                                ui.add_space(16.0);
                                ui.horizontal(|ui| {
                                    let confirm = ui.add(
                                        egui::Button::new(if creating { "Create" } else { "Open" })
                                            .frame(true),
                                    );
                                    ui_probe::record(ctx, "vault-confirm", confirm.rect);
                                    let enter = ui.is_enabled()
                                        && ui.input_mut(|input| {
                                            input.consume_key(
                                                egui::Modifiers::NONE,
                                                egui::Key::Enter,
                                            )
                                        });
                                    if confirm.clicked() || enter {
                                        let result = if creating {
                                            vaults::create(Path::new(&self.folder), &self.name)
                                        } else {
                                            vaults::folder(Path::new(&self.folder))
                                        };
                                        match result {
                                            Ok(root) => selected = Some(root),
                                            Err(error) => self.error = Some(error),
                                        }
                                    }
                                    let back = ui.button("Back");
                                    ui_probe::record(ctx, "vault-back", back.rect);
                                    if back.clicked() {
                                        self.page = Page::Home;
                                        self.error = None;
                                    }
                                });
                            }
                        }
                    });
                });
            });
            if loading || self.picker.active() {
                ui.label(if loading {
                    "Opening vault…"
                } else {
                    "Choose a folder in the file browser…"
                });
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if let Some(error) = self.error.as_ref().or_else(|| self.list.as_ref().err()) {
                let error = ui.colored_label(ui.visuals().error_fg_color, error);
                ui_probe::record(ctx, "vault-error", error.rect);
            }
        });
        if forget.is_some() || selected.is_some() {
            ctx.request_repaint();
        }
        if let Some(root) = forget
            && let Ok(list) = &mut self.list
            && let Err(error) = list.forget(&root)
        {
            self.error = Some(error);
        }
        if close || response.should_close() {
            self.close();
            return None;
        }
        selected.and_then(|root| match vaults::folder(&root) {
            Ok(root) => Some(root),
            Err(error) => {
                self.error = Some(error);
                None
            }
        })
    }
}

fn text_field(ui: &mut egui::Ui, name: &str, text: &mut String, focus: bool) -> bool {
    let id = ui.make_persistent_id(name);
    if focus && !ui.memory(|memory| memory.has_focus(id)) {
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    let response = ui.add(
        TextEdit::singleline(text)
            .id(id)
            .desired_width(f32::INFINITY),
    );
    ui_probe::record(ui.ctx(), name, response.rect);
    if focus {
        response.request_focus();
        ui.ctx().request_repaint();
    }
    focus
        && (ui.is_sizing_pass()
            || !ui.memory(|memory| memory.has_focus(id) && memory.had_focus_last_frame(id)))
}
