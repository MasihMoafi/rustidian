use eframe::egui::{self, Context, Id, Key, Modifiers};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub type Layouts = BTreeMap<PathBuf, BTreeMap<usize, Vec<f32>>>;

fn storage() -> Id {
    Id::new("table-column-widths")
}

pub fn restore(ctx: &Context, layouts: Layouts) {
    ctx.data_mut(|data| data.insert_temp(storage(), layouts));
}

pub fn snapshot(ctx: &Context) -> Layouts {
    ctx.data(|data| data.get_temp(storage()).unwrap_or_default())
}

pub fn take_changed(ctx: &Context) -> bool {
    ctx.data_mut(|data| {
        data.remove_temp::<bool>(storage().with("changed"))
            .unwrap_or(false)
    })
}

pub fn widths(ctx: &Context, from: &Path, table: usize, natural: &[f32]) -> Vec<f32> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Layouts>(storage())
            .get(from)
            .and_then(|tables| tables.get(&table))
            .filter(|widths| {
                widths.len() == natural.len()
                    && widths
                        .iter()
                        .all(|width| width.is_finite() && *width > 0.0 && *width <= 1400.0)
            })
            .cloned()
            .unwrap_or_else(|| natural.to_vec())
    })
}

pub fn handles(ui: &mut egui::Ui, from: &Path, table: usize, outer: egui::Rect, widths: &[f32]) {
    let mut edge = outer.left();
    for (column, width) in widths.iter().enumerate() {
        edge += width;
        let rect = egui::Rect::from_center_size(
            egui::pos2(edge, outer.center().y),
            egui::vec2(8.0, outer.height()),
        )
        .intersect(ui.clip_rect());
        if !rect.is_positive() {
            continue;
        }
        let id = storage().with((from, table, column));
        let response = ui
            .interact(rect, id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
            .on_hover_text("Drag to resize column · arrow keys adjust width");
        crate::ui_probe::record(ui.ctx(), format!("table-resize:{table}:{column}"), rect);
        if response.hovered() || response.dragged() || response.has_focus() {
            ui.painter().line_segment(
                [
                    egui::pos2(edge, rect.top()),
                    egui::pos2(edge, rect.bottom()),
                ],
                egui::Stroke::new(2.0, crate::theme::accent(ui)),
            );
        }
        let drag = id.with("origin");
        if response.drag_started() {
            let x = ui
                .input(|input| input.pointer.press_origin())
                .unwrap_or(rect.center())
                .x;
            ui.ctx()
                .data_mut(|data| data.insert_temp(drag, (x, *width)));
        }
        let mut next = None;
        if response.dragged()
            && let Some(pointer) = response.interact_pointer_pos()
            && let Some((start, width)) = ui.ctx().data(|data| data.get_temp::<(f32, f32)>(drag))
        {
            next = Some(width + pointer.x - start);
        }
        if response.has_focus() {
            if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
                next = Some(*width - 8.0);
            }
            if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowRight)) {
                next = Some(*width + 8.0);
            }
        }
        if let Some(next) = next {
            let mut resized = widths.to_vec();
            resized[column] = next.clamp(56.0, 1400.0);
            ui.ctx().data_mut(|data| {
                data.get_temp_mut_or_default::<Layouts>(storage())
                    .entry(from.to_owned())
                    .or_default()
                    .insert(table, resized);
            });
            ui.ctx().request_repaint();
        }
        if response.drag_stopped() || (next.is_some() && !response.dragged()) {
            ui.ctx()
                .data_mut(|data| data.insert_temp(storage().with("changed"), true));
            ui.ctx().request_repaint();
        }
    }
}
