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
            ui.style_mut().text_styles.insert(
                egui::TextStyle::Heading,
                FontId::new(14.0, egui::FontFamily::Name("semibold".into())),
            );
            ui.set_width((ctx.content_rect().width() - 96.0).clamp(160.0, 560.0));
            // Area remembers its last measured size. Give scrolling content room
            // to grow naturally instead of trapping it in the previous height.
            ui.set_max_height((ctx.content_rect().height() - 96.0).max(160.0));
            let button = theme::dialog_header(ui, "Open another vault", "Close vault chooser");
            ui_probe::record(ctx, "vault-manager-close", button.rect);
            close |= button.clicked();
            let busy = loading || self.picker.active();
            let has_error = self.error.is_some() || self.list.is_err();
            let body_height = (ctx.content_rect().height()
                - 160.0
                - if busy { 44.0 } else { 0.0 }
                - if has_error { 84.0 } else { 0.0 })
            .clamp(120.0, 480.0);
            let body = ScrollArea::vertical()
                .id_salt("vault-manager-body")
                .max_height(body_height)
                .content_margin(egui::Margin::same(1))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.add_enabled_ui(!busy, |ui| {
                        ui.label(RichText::new("Recent vaults").strong());
                        ScrollArea::vertical()
                            .id_salt("recent-vaults")
                            .max_height(208.0)
                            .show(ui, |ui| {
                                ui.set_min_width(ui.available_width());
                                ui.spacing_mut().item_spacing.y = 4.0;
                                if let Ok(list) = &self.list {
                                    if list.roots().is_empty() {
                                        ui.label(
                                            RichText::new("No vaults opened yet.").small().weak(),
                                        );
                                    }
                                    for root in list.roots() {
                                        ui.push_id(root, |ui| {
                                            ui.allocate_ui_with_layout(
                                                egui::vec2(ui.available_width(), 48.0),
                                                Layout::right_to_left(Align::Center),
                                                |ui| {
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
                                                    let width = ui.available_width();
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
                                                            color: ui.visuals().weak_text_color(),
                                                            ..Default::default()
                                                        },
                                                    );
                                                    label.wrap.max_width = (width - 28.0).max(1.0);
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
                                                },
                                            );
                                        });
                                    }
                                }
                            });
                        ui.separator();
                        match self.page {
                            Page::Home => {
                                let card = egui::Frame::group(ui.style()).inner_margin(12).show(
                                    ui,
                                    |ui| {
                                        if action_row(
                                            ui,
                                            "Open a folder",
                                            "Use an existing folder of notes.",
                                            "Open",
                                            "vault-open",
                                        ) {
                                            self.page = Page::Open;
                                            self.focus = true;
                                            self.browse();
                                        }
                                        ui.separator();
                                        if action_row(
                                            ui,
                                            "Create a vault",
                                            "Start a new folder for your notes.",
                                            "Create",
                                            "vault-create",
                                        ) {
                                            self.page = Page::Create;
                                            self.folder = current
                                                .and_then(Path::parent)
                                                .map(|path| path.display().to_string())
                                                .unwrap_or_else(|| {
                                                    std::env::var("HOME").unwrap_or_default()
                                                });
                                            self.focus = true;
                                        }
                                    },
                                );
                                ui_probe::record(ctx, "vault-actions-card", card.response.rect);
                                ui_probe::record(ctx, "vault-actions-clip", ui.clip_rect());
                            }
                            Page::Open | Page::Create => {
                                let creating = self.page == Page::Create;
                                ui.label(
                                    RichText::new(if creating {
                                        "Create a vault"
                                    } else {
                                        "Open a folder"
                                    })
                                    .strong(),
                                );
                                if creating {
                                    ui.label("Vault name");
                                    self.focus =
                                        text_field(ui, "vault-name", &mut self.name, self.focus);
                                }
                                ui.label(if creating { "Location" } else { "Folder" });
                                ui.allocate_ui_with_layout(
                                    egui::vec2(ui.available_width(), 32.0),
                                    Layout::right_to_left(Align::Center),
                                    |ui| {
                                        let browse = ui
                                            .add_sized([96.0, 32.0], egui::Button::new("Browse…"));
                                        ui_probe::record(ctx, "vault-browse", browse.rect);
                                        if browse.clicked() {
                                            self.browse();
                                        }
                                        let focus = text_field(
                                            ui,
                                            "vault-folder",
                                            &mut self.folder,
                                            !creating && self.focus,
                                        );
                                        if !creating {
                                            self.focus = focus;
                                        }
                                    },
                                );
                                if creating {
                                    let destination = Path::new(&self.folder).join(&self.name);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(format!(
                                                "New vault: {}",
                                                destination.display()
                                            ))
                                            .small()
                                            .weak(),
                                        )
                                        .truncate(),
                                    )
                                    .on_hover_text(destination.display().to_string());
                                }
                                ui.horizontal(|ui| {
                                    let confirm = ui.add_sized(
                                        [96.0, 32.0],
                                        egui::Button::new(if creating { "Create" } else { "Open" }),
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
                                    let back =
                                        ui.add_sized([96.0, 32.0], egui::Button::new("Back"));
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
            ui_probe::record(ctx, "vault-body", body.inner_rect);
            ui_probe::metric(
                ctx,
                "vault-body-overflow",
                (body.content_size.y - body.inner_rect.height()).max(0.0),
            );
            if busy {
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), 32.0),
                    Layout::right_to_left(Align::Center),
                    |ui| {
                        let cancel = ui.add_sized([96.0, 32.0], egui::Button::new("Cancel"));
                        ui_probe::record(ctx, "vault-cancel", cancel.rect);
                        close |= cancel.clicked();
                        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                            let spinner = ui.add(egui::Spinner::new().size(14.0));
                            ui_probe::record(ctx, "vault-loading-spinner", spinner.rect);
                            ui.label(
                                RichText::new(if loading {
                                    "Opening your notes…"
                                } else {
                                    "Choose a folder in the file browser…"
                                })
                                .small()
                                .weak(),
                            );
                        });
                    },
                );
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            if let Some(error) = self.error.as_ref().or_else(|| self.list.as_ref().err()) {
                ScrollArea::vertical()
                    .id_salt("vault-error")
                    .max_height(72.0)
                    .show(ui, |ui| {
                        let label = ui.add(
                            egui::Label::new(
                                RichText::new(error).color(ui.visuals().error_fg_color),
                            )
                            .wrap()
                            .selectable(true),
                        );
                        ui_probe::record(ctx, "vault-error", label.rect.intersect(ui.clip_rect()));
                    });
            }
        });
        ui_probe::record(ctx, "vault-manager", response.response.rect);
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

fn action_row(ui: &mut egui::Ui, title: &str, detail: &str, caption: &str, probe: &str) -> bool {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 48.0),
        Layout::right_to_left(Align::Center),
        |ui| {
            let button = ui.add_sized([96.0, 32.0], egui::Button::new(caption));
            ui_probe::record(ui.ctx(), probe, button.rect);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(RichText::new(title).strong());
                ui.label(RichText::new(detail).small().weak());
            });
            button.clicked()
        },
    )
    .inner
}

fn text_field(ui: &mut egui::Ui, name: &str, text: &mut String, focus: bool) -> bool {
    let id = ui.make_persistent_id(name);
    if focus && !ui.memory(|memory| memory.has_focus(id)) {
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    let response = ui.add_sized(
        [ui.available_width(), 32.0],
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
