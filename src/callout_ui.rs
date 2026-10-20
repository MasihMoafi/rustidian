use eframe::egui;
use rust_notes::callouts::{Callout, Kind};

pub fn color(ui: &egui::Ui, kind: Kind) -> egui::Color32 {
    // Installed Obsidian, Masih's appearance/snippets; measured 21 October 2026.
    let dark = ui.visuals().dark_mode;
    let (r, g, b) = match (kind, dark) {
        (Kind::Note | Kind::Info | Kind::Todo, true) => (2, 122, 255),
        (Kind::Note | Kind::Info | Kind::Todo, false) => (8, 109, 221),
        (Kind::Abstract | Kind::Tip, true) => (83, 223, 221),
        (Kind::Abstract | Kind::Tip, false) => (0, 191, 188),
        (Kind::Success, true) => (68, 207, 110),
        (Kind::Success, false) => (8, 185, 78),
        (Kind::Question | Kind::Warning, true) => (233, 151, 63),
        (Kind::Question | Kind::Warning, false) => (236, 117, 0),
        (Kind::Failure | Kind::Danger | Kind::Bug, true) => (251, 70, 76),
        (Kind::Failure | Kind::Danger | Kind::Bug, false) => (233, 49, 71),
        (Kind::Example, true) => (168, 130, 255),
        (Kind::Example, false) => (120, 82, 238),
        (Kind::Quote, _) => (158, 158, 158),
    };
    egui::Color32::from_rgb(r, g, b)
}

pub fn background(ui: &egui::Ui, kind: Kind) -> egui::Color32 {
    let color = color(ui, kind);
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 26)
}

pub fn header(
    ui: &mut egui::Ui,
    model: &Callout,
    ordinal: usize,
    folded: bool,
) -> (egui::Response, Option<String>) {
    use crate::theme::Icon;
    let icon = match model.kind {
        Kind::Note => Icon::CalloutNote,
        Kind::Abstract => Icon::CalloutAbstract,
        Kind::Info => Icon::CalloutInfo,
        Kind::Todo => Icon::CalloutTodo,
        Kind::Tip => Icon::CalloutTip,
        Kind::Success => Icon::CalloutSuccess,
        Kind::Question => Icon::CalloutQuestion,
        Kind::Warning => Icon::CalloutWarning,
        Kind::Failure => Icon::CalloutFailure,
        Kind::Danger => Icon::CalloutDanger,
        Kind::Bug => Icon::CalloutBug,
        Kind::Example => Icon::CalloutExample,
        Kind::Quote => Icon::CalloutQuote,
    };
    let ink = color(ui, model.kind);
    let title = crate::preview::TableCell::new(&model.title);
    let mut hovered_link = None;
    let response = ui
        .horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add(crate::theme::icon_image(icon, ink));
            let galley =
                title.layout_color(ui, true, (ui.available_width() - 24.0).max(1.0), Some(ink));
            let response = ui.add(egui::Label::new(galley.clone()).sense(egui::Sense::hover()));
            if let Some(pointer) = ui
                .ctx()
                .pointer_hover_pos()
                .filter(|pointer| response.rect.contains(*pointer))
            {
                let character = galley.cursor_from_pos(pointer - response.rect.min).index.0;
                if let Some(destination) = title.link_at(character) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    let (_, links) = crate::preview::prepare(&model.title);
                    hovered_link = Some(
                        links
                            .iter()
                            .find(|(id, _)| id == destination)
                            .map_or(destination, |(_, target)| target)
                            .to_owned(),
                    );
                }
            }
            if model.folded.is_some() {
                let (_, toggle) =
                    ui.allocate_exact_size(egui::vec2(16.0, 23.4), egui::Sense::hover());
                crate::theme::disclosure(ui, if folded { 0.0 } else { 1.0 }, &toggle);
            }
        })
        .response;
    let response = ui.interact(response.rect, ui.id().with("header"), egui::Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &model.title));
    crate::ui_probe::record(ui.ctx(), format!("callout:{ordinal}"), response.rect);
    let link = response.clicked().then_some(hovered_link).flatten();
    (response, link)
}
