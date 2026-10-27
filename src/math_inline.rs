//! Reserve inline formula space without replacing the Markdown selection text.
use eframe::egui;

pub struct Placement {
    pub character: usize,
    pub formula: crate::math_ui::Formula,
}

pub fn append(
    ui: &egui::Ui,
    job: &mut egui::text::LayoutJob,
    raw: &str,
    formula: &str,
    display: bool,
    format: egui::TextFormat,
    leading: f32,
) -> Option<Placement> {
    let Ok(Some(mut rendered)) =
        crate::math_ui::image(ui, formula, display, format.font_id.size, format.color)
    else {
        job.append(raw, leading, format);
        return None;
    };
    let limit = if job.wrap.max_width.is_finite() {
        job.wrap.max_width
    } else {
        ui.available_width()
    }
    .max(1.0);
    rendered.size *= (limit / rendered.size.x).min(1.0);
    let placement = Placement {
        character: job.text.chars().count(),
        formula: rendered,
    };
    reserve(job, raw, format, leading, placement.formula.size);
    Some(placement)
}

fn reserve(
    job: &mut egui::text::LayoutJob,
    raw: &str,
    mut format: egui::TextFormat,
    leading: f32,
    size: egui::Vec2,
) {
    // epaint also applies section letter spacing before its first glyph when
    // the paragraph already has text. Reserve the width only before the second
    // original character, keeping the first at the normal prose boundary.
    let first = raw
        .char_indices()
        .nth(1)
        .map_or(raw.len(), |(byte, _)| byte);
    let second = raw
        .char_indices()
        .nth(2)
        .map_or(raw.len(), |(byte, _)| byte);
    format.font_id.size = 0.01;
    format.color = egui::Color32::TRANSPARENT;
    format.background = egui::Color32::TRANSPARENT;
    format.underline = egui::Stroke::NONE;
    format.strikethrough = egui::Stroke::NONE;
    format.line_height = Some(format.line_height.unwrap_or(0.0).max(size.y));
    format.extra_letter_spacing = 0.0;
    job.append(&raw[..first], leading, format.clone());
    format.extra_letter_spacing = size.x;
    job.append(&raw[first..second], 0.0, format.clone());
    format.extra_letter_spacing = 0.0;
    job.append(&raw[second..], 0.0, format);
}

pub fn paint(
    ui: &egui::Ui,
    galley: &egui::Galley,
    origin: egui::Pos2,
    placement: &Placement,
) -> egui::Rect {
    let caret = galley.pos_from_cursor(egui::text::CCursor {
        index: placement.character.into(),
        prefer_next_row: true,
    });
    let rect = egui::Rect::from_min_size(
        origin
            + egui::vec2(
                caret.left(),
                caret.center().y - placement.formula.size.y / 2.0,
            ),
        placement.formula.size,
    );
    egui::Image::new(&placement.formula.uri)
        .fit_to_exact_size(placement.formula.size)
        .paint_at(ui, rect);
    crate::ui_probe::record(
        ui.ctx(),
        format!(
            "math-fragment:{:x}:{}",
            ui.scope_id().value(),
            placement.character
        ),
        rect,
    );
    rect
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_formula_retains_selection_text_and_room_for_neighbors() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let format =
                egui::TextFormat::simple(egui::FontId::proportional(18.0), egui::Color32::WHITE);
            for width in [120.0, 400.0] {
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = width;
                job.append("π before ", 0.0, format.clone());
                reserve(
                    &mut job,
                    "$x^2$",
                    format.clone(),
                    0.0,
                    egui::vec2(64.0, 38.0),
                );
                job.append(" after", 0.0, format.clone());
                let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                assert_eq!(galley.text(), "π before $x^2$ after");
                assert_eq!(galley.end().index.0, 20);
                let first = galley.pos_from_cursor(egui::text::CCursor {
                    index: 9.into(),
                    prefer_next_row: true,
                });
                let after = galley.pos_from_cursor(egui::text::CCursor::new(14));
                assert!((after.left() - first.left() - 64.0).abs() < 1.0,
                    "width={width}, first={first:?}, after={after:?}, rows={:?}", galley.rows.iter().map(|row| (row.rect(), row.char_count_including_newline().0)).collect::<Vec<_>>());
                assert!(galley.size().y >= 38.0);
                assert!(galley.size().x <= width + 1.0);
                assert_eq!(first.top(), after.top(), "width={width}, first={first:?}, after={after:?}");
                if width == 120.0 {
                    assert!(galley.rows.len() >= 2);
                } else {
                    let cursor = galley.layout_from_cursor(egui::text::CCursor::new(9));
                    let row = &galley.rows[cursor.row];
                    let previous = &row.glyphs[cursor.column.0 - 1];
                    assert!((first.left() - row.pos.x - previous.max_x()).abs() < 1.0,
                        "The formula must follow the preceding space without an extra formula-width gap");
                }
            }
        })
        .drop_without_applying_deltas();
    }
}
