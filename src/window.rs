use crate::theme::{self, Icon};
use eframe::egui::{
    self, Align, Color32, FontId, Layout, Margin, PointerButton, Sense, ViewportCommand,
};

/// Keep window controls in the native app so hover feedback matches every other control.
pub fn title_bar(ui: &mut egui::Ui, title: &str) {
    resize_edges(ui);
    let maximized = ui.input(|input| input.viewport().maximized.unwrap_or(false));
    egui::Panel::top("window-title-bar")
        .exact_size(34.0)
        .resizable(false)
        .frame(
            egui::Frame::NONE
                .fill(ui.visuals().panel_fill)
                .inner_margin(Margin::symmetric(6, 3)),
        )
        .show(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if control(ui, Icon::Close, "Close window", true).clicked() {
                    // The existing close-request handler keeps unsaved changes safe.
                    ui.ctx().send_viewport_cmd(ViewportCommand::Close);
                }
                if control(
                    ui,
                    if maximized {
                        Icon::Restore
                    } else {
                        Icon::Maximize
                    },
                    if maximized {
                        "Restore window"
                    } else {
                        "Maximize window"
                    },
                    false,
                )
                .clicked()
                {
                    ui.ctx()
                        .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }
                if control(ui, Icon::Minimize, "Minimize window", false).clicked() {
                    ui.ctx().send_viewport_cmd(ViewportCommand::Minimized(true));
                }
                let rect = ui.available_rect_before_wrap();
                let response =
                    ui.interact(rect, ui.id().with("title-drag"), Sense::click_and_drag());
                crate::ui_probe::record(ui.ctx(), "window-title-drag", rect);
                let galley = egui::WidgetText::from(egui::RichText::new(title).size(12.0).weak())
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Truncate),
                        (rect.width() - 12.0).max(1.0),
                        FontId::proportional(12.0),
                    );
                ui.painter().galley(
                    rect.left_center() + egui::vec2(6.0, -galley.size().y / 2.0),
                    galley,
                    ui.visuals().weak_text_color(),
                );
                if response.double_clicked() {
                    ui.ctx()
                        .send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                } else if response.is_pointer_button_down_on()
                    && ui.input(|input| input.pointer.primary_pressed())
                {
                    ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
                }
            });
        });
}

fn control(ui: &mut egui::Ui, icon: Icon, label: &str, close: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 28.0), Sense::click());
    if close && response.hovered() {
        ui.painter()
            .rect_filled(rect, 6.0, Color32::from_rgb(185, 53, 67));
    } else {
        theme::control_feedback(ui, &response);
    }
    let tint = if close && response.hovered() {
        Color32::WHITE
    } else if response.hovered() {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    theme::icon_image(icon, tint).paint_at(
        ui,
        egui::Rect::from_center_size(rect.center(), egui::vec2(14.0, 14.0)),
    );
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, label));
    crate::ui_probe::record(ui.ctx(), format!("window:{label}"), rect);
    response.on_hover_text(label)
}

fn resize_edges(ui: &egui::Ui) {
    use egui::{CursorIcon, ResizeDirection};
    let (position, pressed, locked) = ui.input(|input| {
        (
            input.pointer.interact_pos(),
            input.pointer.button_pressed(PointerButton::Primary),
            input.viewport().maximized.unwrap_or(false)
                || input.viewport().fullscreen.unwrap_or(false),
        )
    });
    if locked {
        return;
    }
    let Some(position) = position else {
        return;
    };
    let rect = ui.max_rect();
    let left = position.x < rect.left() + 4.0;
    let right = position.x > rect.right() - 4.0;
    let top = position.y < rect.top() + 4.0;
    let bottom = position.y > rect.bottom() - 4.0;
    let (direction, cursor) = match (left, right, top, bottom) {
        (true, _, true, _) => (ResizeDirection::NorthWest, CursorIcon::ResizeNwSe),
        (_, true, true, _) => (ResizeDirection::NorthEast, CursorIcon::ResizeNeSw),
        (true, _, _, true) => (ResizeDirection::SouthWest, CursorIcon::ResizeNeSw),
        (_, true, _, true) => (ResizeDirection::SouthEast, CursorIcon::ResizeNwSe),
        (true, _, _, _) => (ResizeDirection::West, CursorIcon::ResizeHorizontal),
        (_, true, _, _) => (ResizeDirection::East, CursorIcon::ResizeHorizontal),
        (_, _, true, _) => (ResizeDirection::North, CursorIcon::ResizeVertical),
        (_, _, _, true) => (ResizeDirection::South, CursorIcon::ResizeVertical),
        _ => return,
    };
    ui.ctx().set_cursor_icon(cursor);
    if pressed {
        ui.ctx()
            .send_viewport_cmd(ViewportCommand::BeginResize(direction));
    }
}
