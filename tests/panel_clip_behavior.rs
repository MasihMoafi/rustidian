#![cfg(feature = "desktop")]

use eframe::egui::{self, Color32, Frame, Panel, Pos2, RawInput, Rect, Vec2, epaint::Shape};

#[test]
fn minimum_panels_paint_the_whole_area_they_allocate_when_contents_need_more_space() {
    let fill = Color32::from_rgb(93, 115, 137);
    for side in ["left", "right", "top", "bottom"] {
        let ctx = egui::Context::default();
        let mut allocated = Rect::NOTHING;
        let output = ctx.run_ui(
            RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))),
                ..Default::default()
            },
            |ui| {
                let panel = match side {
                    "left" => Panel::left(side),
                    "right" => Panel::right(side),
                    "top" => Panel::top(side),
                    _ => Panel::bottom(side),
                };
                allocated = panel
                    .default_size(180.0)
                    .min_size(180.0)
                    .max_size(450.0)
                    .show_separator_line(false)
                    .frame(Frame::NONE.fill(fill))
                    .show(ui, |ui| ui.set_min_size(Vec2::splat(220.0)))
                    .response
                    .rect;
            },
        );
        let painted = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                Shape::Rect(rect) if rect.fill == fill => {
                    Some(rect.rect.intersect(shape.clip_rect))
                }
                _ => None,
            })
            .expect("The panel must paint its background");
        assert!(allocated.width() >= 220.0 && allocated.height() >= 220.0);
        let visible = allocated.intersect(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0)));
        assert!(
            painted.contains_rect(visible),
            "{side} panel allocated visible {visible:?} but painted only {painted:?}"
        );
        output.drop_without_applying_deltas();
    }
}
