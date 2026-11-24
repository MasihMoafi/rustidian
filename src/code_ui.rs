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
        .on_hover_text("Copy code")
        .on_hover_cursor(egui::CursorIcon::PointingHand);
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

pub fn display_text(body: &str) -> String {
    // The final line ending terminates the last code line rather than adding a blank row.
    // Normalize only the displayed string; clipboard bytes retain the original endings.
    body.strip_suffix("\r\n")
        .or_else(|| body.strip_suffix('\n'))
        .unwrap_or(body)
        .replace("\r\n", "\n")
}

pub fn reading(ui: &mut egui::Ui, body: &str, language: &str, ordinal: usize) -> egui::Response {
    reading_with_find(ui, body, language, ordinal, &[]).0
}

pub fn reading_with_find(
    ui: &mut egui::Ui,
    body: &str,
    language: &str,
    ordinal: usize,
    matches: &[(std::ops::Range<usize>, bool)],
) -> (egui::Response, Option<egui::Rect>) {
    let display = display_text(body);
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
    } else if matches.is_empty() {
        append_colored(&mut job, &display, 0, 0.0, format, &colors);
    } else {
        append_colored_with_find(&mut job, &display, 0, 0.0, format, &colors, matches);
    }
    let mut galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
    crate::theme::set_line_height(&mut galley, crate::theme::document_size(ui, LINE_HEIGHT));
    let mut label_rect = None;
    let frame = egui::Frame::NONE
        .fill(ui.visuals().code_bg_color)
        .corner_radius(7)
        .inner_margin(egui::Margin::symmetric(HORIZONTAL_INSET as i8, 12))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let response = ui.add(egui::Label::new(galley.clone()).selectable(true));
            label_rect = Some(response.rect);
            response
        });
    let panel = frame.response.rect;
    if panel.intersects(ui.clip_rect()) {
        crate::ui_probe::record(ui.ctx(), format!("code-block:{ordinal}"), panel);
        copy(ui, frame.response.id, panel, body, ordinal, true);
    }
    let active_rect = matches
        .iter()
        .find(|(_, active)| *active)
        .and_then(|(range, _)| {
            let origin = label_rect?.min.to_vec2();
            let first = galley.pos_from_cursor(egui::text::CCursor::new(range.start));
            let last = galley.pos_from_cursor(egui::text::CCursor::new(range.end));
            let rect = if (first.min.y - last.min.y).abs() < 1.0 {
                first.union(last)
            } else {
                first
            };
            Some(rect.translate(origin).expand2(egui::vec2(1.0, 0.0)))
        });
    (frame.response, active_rect)
}

fn append_colored_with_find(
    job: &mut LayoutJob,
    text: &str,
    start: usize,
    mut leading: f32,
    format: egui::TextFormat,
    colors: &[crate::syntax::Token],
    matches: &[(std::ops::Range<usize>, bool)],
) {
    let offsets: Vec<usize> = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain([text.len()])
        .collect();
    let byte_of = |character: usize| offsets.get(character).copied().unwrap_or(text.len());
    let byte_matches: Vec<(std::ops::Range<usize>, bool)> = matches
        .iter()
        .map(|(range, active)| (byte_of(range.start)..byte_of(range.end), *active))
        .collect();
    let mut boundaries = vec![0, text.len()];
    for token in colors {
        let from = token.range.start.saturating_sub(start).min(text.len());
        let to = token.range.end.saturating_sub(start).min(text.len());
        if from < to {
            boundaries.extend([from, to]);
        }
    }
    for (range, _) in &byte_matches {
        boundaries.extend([range.start, range.end]);
    }
    boundaries.sort_unstable();
    boundaries.dedup();
    let mut token = 0;
    for pair in boundaries.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        while token < colors.len() && colors[token].range.end <= start + from {
            token += 1;
        }
        let mut piece = format.clone();
        if let Some(color) = colors
            .get(token)
            .filter(|token| token.range.contains(&(start + from)))
            .map(|token| token.color)
        {
            piece.color = color;
        }
        if let Some((_, active)) = byte_matches.iter().find(|(range, _)| range.contains(&from)) {
            piece.background = crate::preview::find_match_bg(*active);
        }
        job.append(&text[from..to], leading, piece);
        leading = 0.0;
    }
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
