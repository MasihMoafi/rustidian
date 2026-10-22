use crate::{preview, theme, ui_probe};
use eframe::egui::{self, Margin, Rect};
use egui_commonmark::CommonMarkCache;
use rust_notes::{
    tab::Tab,
    vault::{Document, Vault},
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Clone)]
struct Request {
    from: PathBuf,
    target: String,
    anchor: Rect,
}

pub fn offer(ui: &egui::Ui, from: &Path, target: &str, anchor: Rect) {
    if target.contains(':')
        || ui
            .ctx()
            .data(|data| data.get_temp::<bool>(egui::Id::unique("inside-page-preview")))
            .unwrap_or(false)
    {
        return;
    }
    ui.ctx().data_mut(|data| {
        data.insert_temp(
            egui::Id::unique("page-preview-request"),
            Request {
                from: from.to_owned(),
                target: target.to_owned(),
                anchor,
            },
        )
    });
}

#[derive(Default)]
pub struct Manager {
    pending: Option<(Request, f64)>,
    card: Option<Card>,
    cache: CommonMarkCache,
}

struct Card {
    request: Request,
    path: Option<PathBuf>,
    tab: Option<Tab>,
    rect: Rect,
    contact: f64,
    focused: bool,
    error: Option<String>,
}

pub enum Action {
    Open { from: PathBuf, target: String },
    Edit { before: Document, after: Document },
}

impl Manager {
    pub fn close(&mut self) {
        self.pending = None;
        self.card = None;
    }

    pub fn show(
        &mut self,
        ctx: &egui::Context,
        vault: Option<&Vault>,
        tabs: &[Tab],
        active: usize,
        blocked: bool,
    ) -> Option<Action> {
        let request = ctx.data_mut(|data| {
            let id = egui::Id::unique("page-preview-request");
            let request = data.get_temp::<Request>(id);
            data.remove::<Request>(id);
            request
        });
        let (Some(vault), Some(source)) = (vault, tabs.get(active)) else {
            self.close();
            return None;
        };
        if blocked
            || self
                .card
                .as_ref()
                .is_some_and(|card| card.request.from != source.document.path)
        {
            self.close();
            if blocked {
                return None;
            }
        }
        let now = ctx.input(|input| input.time);
        let pointer = ctx.pointer_hover_pos();
        if let Some(request) = request.filter(|request| request.from == source.document.path) {
            if let Some(card) = &mut self.card
                && card.request.from == request.from
                && card.request.target == request.target
            {
                card.contact = now;
                card.request.anchor = request.anchor;
                self.pending = None;
            } else if self.card.as_ref().is_none_or(|card| !card.focused) {
                let same = self.pending.as_ref().is_some_and(|(previous, _)| {
                    previous.from == request.from && previous.target == request.target
                });
                if !same {
                    self.pending = Some((request.clone(), now));
                }
                let ready = self
                    .pending
                    .as_ref()
                    .is_some_and(|(_, started)| now - started >= 0.3);
                if ready {
                    let path = vault.resolve_destination(&request.from, &request.target);
                    if path.is_none() {
                        let raw = request.target.split(['#', '|']).next().unwrap_or("").trim();
                        let decoded =
                            rust_notes::paths::decode(raw).unwrap_or_else(|| raw.to_owned());
                        if decoded.is_empty()
                            || rust_notes::assets::is_image(Path::new(&decoded))
                            || !Path::new(&decoded).components().all(|component| {
                                matches!(component, std::path::Component::Normal(_))
                            })
                        {
                            self.pending = None;
                            return None;
                        }
                    }
                    let document = path.as_ref().map(|path| {
                        tabs.iter()
                            .find(|tab| tab.document.path == *path)
                            .map(|tab| Ok(tab.document.clone()))
                            .unwrap_or_else(|| vault.open_document(path))
                    });
                    let (tab, error) = match document {
                        Some(Ok(document)) if !document.is_image() && !document.is_plain_text() => {
                            (Some(Tab::new(document)), None)
                        }
                        Some(Ok(_)) => {
                            self.pending = None;
                            return None;
                        }
                        Some(Err(error)) => (None, Some(error)),
                        None => (None, None),
                    };
                    self.card = Some(Card {
                        request,
                        path,
                        tab,
                        rect: Rect::NOTHING,
                        contact: now,
                        focused: false,
                        error,
                    });
                    self.pending = None;
                } else {
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
            }
        } else {
            self.pending = None;
        }
        let card = self.card.as_mut()?;
        if pointer.is_some_and(|pointer| card.rect.expand(8.0).contains(pointer)) {
            card.contact = now;
        }
        let outside_click = ctx.input(|input| input.pointer.any_pressed())
            && pointer.is_some_and(|pointer| {
                !card.rect.contains(pointer) && !card.request.anchor.contains(pointer)
            });
        if outside_click || (!card.focused && now - card.contact > 0.25) {
            self.close();
            return None;
        }
        if !card.focused {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if let (Some(path), Some(tab)) = (&card.path, &mut card.tab) {
            if let Some(current) = tabs.iter().find(|current| current.document.path == *path) {
                if current.document.text != tab.document.text
                    || current.document.saved_revision() != tab.document.saved_revision()
                {
                    tab.replace(current.document.clone());
                }
            } else if let Some(current) = vault.notes.iter().find(|current| current.path == *path)
                && tab.document.saved_text() != Some(current.text.as_str())
            {
                match vault.open_document(path) {
                    Ok(document) => tab.replace(document),
                    Err(error) => card.error = Some(error),
                }
            }
        }
        let width = (ctx.content_rect().width() - 32.0).clamp(180.0, 452.0);
        let height = (ctx.content_rect().height() - 100.0).clamp(100.0, 400.0);
        let mut action = None;
        let response = egui::Area::new(egui::Id::unique("page-preview-card"))
            .order(egui::Order::Foreground)
            .movable(false)
            .fixed_pos(card.request.anchor.left_bottom() + egui::vec2(0.0, 4.0))
            .constrain(true)
            .show(ctx, |ui| {
                let frame = egui::Frame::popup(ui.style())
                    .inner_margin(Margin::same(32))
                    .corner_radius(12);
                // Area remembers the previous card's size. Let a longer note grow
                // before ScrollArea clamps its viewport to that old height.
                ui.set_max_height(height + frame.total_margin().sum().y);
                let content_width = width - frame.total_margin().sum().x;
                frame.show(ui, |ui| {
                    ui.set_width(content_width);
                    let corner = Rect::from_center_size(
                        egui::pos2(ui.max_rect().right() + 16.0, ui.min_rect().top() - 16.0),
                        egui::vec2(24.0, 24.0),
                    );
                    let open = ui
                        .interact(
                            corner,
                            ui.make_persistent_id("open-link"),
                            egui::Sense::click(),
                        )
                        .on_hover_text("Open link");
                    open.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Open link")
                    });
                    theme::control_feedback(ui, &open);
                    theme::icon_image(theme::Icon::OpenLink, ui.visuals().weak_text_color())
                        .paint_at(
                            ui,
                            Rect::from_center_size(corner.center(), egui::vec2(18.0, 18.0)),
                        );
                    ui_probe::record(ctx, "page-preview-open", open.rect);
                    if open.clicked() {
                        action = Some(Action::Open {
                            from: card.request.from.clone(),
                            target: card.request.target.clone(),
                        });
                    }
                    if let Some(error) = &card.error {
                        ui.label(error);
                    } else if let Some(tab) = &mut card.tab {
                        let before = tab.document.clone();
                        let anchor =
                            card.request.target.split_once('#').map(|(_, fragment)| {
                                fragment.split('|').next().unwrap_or(fragment)
                            });
                        ctx.data_mut(|data| {
                            data.insert_temp(egui::Id::unique("inside-page-preview"), true)
                        });
                        let follow = ui_probe::scope(ctx, "page-preview/", || {
                            egui::ScrollArea::vertical()
                                .id_salt("page-preview-scroll")
                                .max_height(height)
                                .show(ui, |ui| {
                                    preview::show(ui, &mut self.cache, tab, anchor, Some(vault))
                                })
                                .inner
                        });
                        ctx.data_mut(|data| {
                            data.remove::<bool>(egui::Id::unique("inside-page-preview"))
                        });
                        if tab.document.text != before.text {
                            action = Some(Action::Edit {
                                before,
                                after: tab.document.clone(),
                            });
                        } else if let Some(target) = follow {
                            action = Some(Action::Open {
                                from: tab.document.path.clone(),
                                target,
                            });
                        }
                    } else {
                        let missing = ui.add(
                            egui::Button::new(
                                "This note has not been created yet. Click to create.",
                            )
                            .wrap()
                            .frame_when_inactive(false),
                        );
                        ui_probe::record(ctx, "page-preview-create", missing.rect);
                        if missing.clicked() {
                            action = Some(Action::Open {
                                from: card.request.from.clone(),
                                target: card.request.target.clone(),
                            });
                        }
                    }
                });
            });
        card.rect = response.response.rect;
        ui_probe::record(ctx, "page-preview-card", card.rect);
        if ctx.input(|input| input.pointer.any_pressed())
            && pointer.is_some_and(|pointer| card.rect.contains(pointer))
        {
            card.focused = true;
        }
        if matches!(action, Some(Action::Open { .. })) {
            self.close();
        }
        action
    }
}
