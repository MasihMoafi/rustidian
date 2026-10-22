//! Fenced-code typography, color spans and generated clipboard control shared by both views.
use eframe::egui::{self, FontId, text::LayoutJob};

pub const FONT_SIZE: f32 = 15.75;
pub const LINE_HEIGHT: f32 = 25.515;
pub const HORIZONTAL_INSET: f32 = 16.0;

pub fn copy(
    ui: &egui::Ui,
    id: egui::Id,
    panel: egui::Rect,
    body: &str,
    ordinal: usize,
    always_visible: bool,
) {
    let rect = egui::Rect::from_min_size(
        panel.right_top() + egui::vec2(-30.0, 3.0),
        egui::vec2(24.0, 24.0),
    );
    let response = ui
        .interact(rect, id.with("copy-code"), egui::Sense::click())
        .on_hover_text("Copy code");
    if always_visible || ui.rect_contains_pointer(panel) {
        crate::theme::icon_image(crate::theme::Icon::Copy, ui.visuals().weak_text_color())
            .paint_at(
                ui,
                egui::Rect::from_center_size(rect.center(), egui::vec2(18.0, 18.0)),
            );
        crate::ui_probe::record(ui.ctx(), format!("copy-code:{ordinal}"), rect);
    }
    if response.clicked() {
        ui.ctx().copy_text(body.to_owned());
    }
}

pub fn reading(ui: &mut egui::Ui, body: &str, language: &str, ordinal: usize) -> egui::Response {
    // The final line ending terminates the last code line rather than adding a blank row.
    // Normalize only the displayed string; clipboard bytes retain the original endings.
    let display = body
        .strip_suffix("\r\n")
        .or_else(|| body.strip_suffix('\n'))
        .unwrap_or(body)
        .replace("\r\n", "\n");
    let mut job = LayoutJob::default();
    job.wrap.max_width = (ui.available_width() - 2.0 * HORIZONTAL_INSET).max(1.0);
    let colors = crate::syntax::code_tokens(ui, &display, language, true);
    let format = egui::TextFormat {
        font_id: FontId::monospace(crate::theme::document_size(ui, FONT_SIZE)),
        color: ui.visuals().weak_text_color(),
        line_height: Some(crate::theme::document_size(ui, LINE_HEIGHT)),
        ..Default::default()
    };
    if display.is_empty() {
        job.append(" ", 0.0, format);
    } else {
        append_colored(&mut job, &display, 0, 0.0, format, &colors);
    }
    let mut galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    crate::theme::set_line_height(&mut galley, crate::theme::document_size(ui, LINE_HEIGHT));
    let frame = egui::Frame::NONE
        .fill(ui.visuals().code_bg_color)
        .corner_radius(7)
        .inner_margin(egui::Margin::symmetric(HORIZONTAL_INSET as i8, 12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.add(egui::Label::new(galley).selectable(true))
        });
    let panel = frame.response.rect;
    if panel.intersects(ui.clip_rect()) {
        crate::ui_probe::record(ui.ctx(), format!("code-block:{ordinal}"), panel);
        copy(ui, frame.response.id, panel, body, ordinal, true);
    }
    frame.response
}

pub fn append_colored(
    job: &mut LayoutJob,
    text: &str,
    start: usize,
    mut leading: f32,
    format: egui::TextFormat,
    colors: &[crate::syntax::Token],
) {
    let end = start + text.len();
    let mut previous = start;
    let first = colors.partition_point(|token| token.range.end <= start);
    for token in colors[first..]
        .iter()
        .take_while(|token| token.range.start < end)
    {
        let from = token.range.start.max(start);
        let to = token.range.end.min(end);
        if previous < from {
            job.append(
                &text[previous - start..from - start],
                leading,
                format.clone(),
            );
            leading = 0.0;
            previous = from;
        }
        if from < to {
            let mut colored = format.clone();
            colored.color = token.color;
            job.append(&text[from - start..to - start], leading, colored);
            leading = 0.0;
            previous = to;
        }
    }
    if previous < end {
        job.append(&text[previous - start..], leading, format);
    }
}
