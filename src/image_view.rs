use eframe::egui::{self, Align, Layout, ScrollArea, Vec2};
use rust_notes::tab::ImageScale;
use std::path::Path;

pub fn show(ui: &mut egui::Ui, absolute: &Path, id: egui::Id, scale: ImageScale) {
    let available = ui.available_size().max(Vec2::splat(1.0));
    let image = egui::Image::new(format!("file://{}", absolute.display()));
    let natural = match image.load_for_size(ui.ctx(), available) {
        Ok(poll) => match poll.size() {
            Some(size) => size,
            None => {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                });
                return;
            }
        },
        Err(error) => {
            ui.centered_and_justified(|ui| {
                ui.label(format!("Cannot open image: {error}"));
            });
            return;
        }
    };
    let factor = match scale {
        ImageScale::Fit => (available.x / natural.x)
            .min(available.y / natural.y)
            .min(1.0),
        ImageScale::Actual => 1.0,
    };
    let size = natural * factor;
    let canvas = available.max(size);
    ScrollArea::both()
        .id_salt(id)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_size(canvas);
            ui.set_width(canvas.x);
            ui.add_space(((canvas.y - size.y) * 0.5).max(0.0));
            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                let response = ui.add(image.fit_to_exact_size(size));
                crate::ui_probe::record(ui.ctx(), "attachment-image", response.rect);
            });
        });
}
