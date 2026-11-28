//! An editable vector canvas for Excalidraw documents.
use eframe::egui::{self, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rustidian::{excalidraw::Drawing, vault::Document};
use serde_json::{Value, json};

#[derive(Clone, Copy, Default, PartialEq)]
enum Tool {
    #[default]
    Select,
    Hand,
    Rect,
    Diamond,
    Ellipse,
    Arrow,
    Line,
    Freedraw,
    Text,
    Eraser,
}

struct Drag {
    start: Pos2,
    original: Vec<Value>,
    index: Option<usize>,
    points: Vec<Pos2>,
    pan: bool,
}

pub struct ExcalidrawViewState {
    tool: Tool,
    tool_locked: bool,
    color: usize,
    width: f32,
    pan: Vec2,
    zoom: f32,
    selected: Option<String>,
    drag: Option<Drag>,
    text: String,
    text_position: Option<Pos2>,
    undo: Vec<Vec<Value>>,
    redo: Vec<Vec<Value>>,
    last_text: Option<String>,
    pub show_markdown: bool,
}

impl Default for ExcalidrawViewState {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            tool_locked: false,
            color: 0,
            width: 2.0,
            pan: Vec2::ZERO,
            zoom: 1.0,
            selected: None,
            drag: None,
            text: String::new(),
            text_position: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_text: None,
            show_markdown: false,
        }
    }
}

fn number(element: &Value, key: &str) -> f32 {
    element[key].as_f64().unwrap_or(0.0) as f32
}
fn point(element: &Value) -> Pos2 {
    egui::pos2(number(element, "x"), number(element, "y"))
}
fn visible(element: &Value) -> bool {
    !element["isDeleted"].as_bool().unwrap_or(false)
}
fn bounds(element: &Value) -> Rect {
    let p = point(element);
    Rect::from_two_pos(
        p,
        p + egui::vec2(number(element, "width"), number(element, "height")),
    )
    .expand(6.0)
}
fn color(element: &Value, fallback: Color32) -> Color32 {
    let s = element["strokeColor"]
        .as_str()
        .unwrap_or("")
        .trim_start_matches('#');
    if s.len() == 6
        && let Ok(n) = u32::from_str_radix(s, 16)
    {
        return Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8);
    }
    fallback
}
fn canvas_pos(p: Pos2, rect: Rect, state: &ExcalidrawViewState) -> Pos2 {
    Pos2::ZERO + (p - rect.min - state.pan) / state.zoom
}
fn screen_pos(p: Pos2, rect: Rect, state: &ExcalidrawViewState) -> Pos2 {
    rect.min + state.pan + p.to_vec2() * state.zoom
}
fn save(document: &mut Document, drawing: &mut Drawing, elements: &[Value]) -> bool {
    drawing.data["elements"] = Value::Array(elements.to_vec());
    match drawing.serialize() {
        Ok(text) if text != document.text => {
            document.text = text;
            true
        }
        _ => false,
    }
}
fn remember(state: &mut ExcalidrawViewState, before: Vec<Value>) {
    state.undo.push(before);
    state.redo.clear();
}
fn new_element(kind: &str, p: Pos2, color: Color32, width: f32) -> Value {
    json!({"id": format!("{:016x}", fastrand::u64(..)), "type": kind, "x": p.x, "y": p.y,
        "width": 0, "height": 0, "angle": 0, "strokeColor": format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b()),
        "backgroundColor": "transparent", "fillStyle": "solid", "strokeWidth": width, "strokeStyle": "solid",
        "roughness": 1, "opacity": 100, "seed": fastrand::u32(..), "version": 1, "versionNonce": fastrand::u32(..),
        "isDeleted": false, "groupIds": [], "frameId": null, "boundElements": null, "updated": chrono::Utc::now().timestamp_millis(), "link": null, "locked": false })
}

fn paint(ui: &egui::Ui, rect: Rect, state: &ExcalidrawViewState, element: &Value) {
    if !visible(element) {
        return;
    }
    let painter = ui.painter();
    let origin = point(element);
    let a = screen_pos(origin, rect, state);
    let b = screen_pos(
        origin + egui::vec2(number(element, "width"), number(element, "height")),
        rect,
        state,
    );
    let r = Rect::from_two_pos(a, b);
    let stroke = Stroke::new(
        number(element, "strokeWidth").max(1.0) * state.zoom,
        color(element, ui.visuals().text_color()),
    );
    match element["type"].as_str().unwrap_or("") {
        "rectangle" => {
            painter.rect_stroke(r, 0, stroke, egui::StrokeKind::Middle);
        }
        "diamond" => {
            painter.line(
                vec![
                    egui::pos2(r.center().x, r.top()),
                    egui::pos2(r.right(), r.center().y),
                    egui::pos2(r.center().x, r.bottom()),
                    egui::pos2(r.left(), r.center().y),
                    egui::pos2(r.center().x, r.top()),
                ],
                stroke,
            );
        }
        "ellipse" => {
            let points = (0..=48)
                .map(|i| {
                    let t = i as f32 * std::f32::consts::TAU / 48.0;
                    r.center() + egui::vec2(t.cos() * r.width() / 2.0, t.sin() * r.height() / 2.0)
                })
                .collect();
            painter.line(points, stroke);
        }
        "line" | "arrow" | "freedraw" => {
            let points: Vec<Pos2> = element["points"]
                .as_array()
                .map(|points| {
                    points
                        .iter()
                        .filter_map(|p| {
                            Some(screen_pos(
                                origin + egui::vec2(p[0].as_f64()? as f32, p[1].as_f64()? as f32),
                                rect,
                                state,
                            ))
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![a, b]);
            if points.len() >= 2 {
                painter.line(points.clone(), stroke);
                if element["type"] == "arrow" {
                    let end = points[points.len() - 1];
                    let direction = (end - points[points.len() - 2]).normalized();
                    let side = egui::vec2(-direction.y, direction.x);
                    let size = 12.0 * state.zoom;
                    painter.line(
                        vec![
                            end - direction * size + side * size * 0.5,
                            end,
                            end - direction * size - side * size * 0.5,
                        ],
                        stroke,
                    );
                }
            }
        }
        "text" => {
            painter.text(
                a,
                egui::Align2::LEFT_TOP,
                element["text"].as_str().unwrap_or(""),
                egui::FontId::proportional(
                    element["fontSize"].as_f64().unwrap_or(20.0) as f32 * state.zoom,
                ),
                stroke.color,
            );
        }
        _ => {}
    }
    if state.selected.as_deref() == element["id"].as_str() {
        painter.rect_stroke(
            r.expand(4.0),
            0,
            Stroke::new(1.0, ui.visuals().selection.stroke.color),
            egui::StrokeKind::Middle,
        );
    }
}

fn tool_button(ui: &mut egui::Ui, tool: Tool, active: bool, hint: &str, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), Sense::click());
    let painter = ui.painter();
    if active || response.hovered() {
        painter.rect_filled(
            rect,
            5,
            if active {
                Color32::from_rgb(224, 223, 255)
            } else {
                ui.visuals().widgets.hovered.bg_fill
            },
        );
    }
    let color = if active {
        Color32::from_rgb(71, 66, 166)
    } else {
        ui.visuals().text_color()
    };
    let stroke = Stroke::new(1.6, color);
    let p = |x: f32, y: f32| rect.min + egui::vec2(x, y);
    match tool {
        Tool::Select => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(8., 5.),
                    p(8., 22.),
                    p(12., 18.),
                    p(15., 24.),
                    p(18., 22.),
                    p(15., 16.),
                    p(21., 16.),
                ],
                stroke,
            ));
        }
        Tool::Hand => {
            painter.add(egui::Shape::line(
                vec![
                    p(8., 17.),
                    p(7., 13.),
                    p(9., 12.),
                    p(11., 16.),
                    p(11., 7.),
                    p(13., 6.),
                    p(14., 13.),
                    p(14., 5.),
                    p(16., 5.),
                    p(17., 13.),
                    p(17., 7.),
                    p(19., 7.),
                    p(20., 14.),
                    p(20., 10.),
                    p(22., 10.),
                    p(22., 18.),
                    p(20., 23.),
                    p(12., 23.),
                    p(8., 17.),
                ],
                stroke,
            ));
        }
        Tool::Rect => {
            painter.rect_stroke(
                Rect::from_min_max(p(6., 7.), p(23., 22.)),
                2,
                stroke,
                egui::StrokeKind::Middle,
            );
        }
        Tool::Diamond => {
            painter.add(egui::Shape::closed_line(
                vec![p(15., 5.), p(24., 15.), p(15., 24.), p(6., 15.)],
                stroke,
            ));
        }
        Tool::Ellipse => {
            painter.circle_stroke(p(15., 15.), 8.5, stroke);
        }
        Tool::Arrow => {
            painter.line_segment([p(7., 22.), p(22., 7.)], stroke);
            painter.line_segment([p(15., 7.), p(22., 7.)], stroke);
            painter.line_segment([p(22., 7.), p(22., 14.)], stroke);
        }
        Tool::Line => {
            painter.line_segment([p(7., 22.), p(22., 7.)], stroke);
        }
        Tool::Freedraw => {
            painter.add(egui::Shape::closed_line(
                vec![p(7., 22.), p(9., 17.), p(20., 6.), p(23., 9.), p(12., 20.)],
                stroke,
            ));
            painter.line_segment([p(9., 17.), p(12., 20.)], stroke);
        }
        Tool::Text => {
            painter.line_segment([p(7., 23.), p(14., 6.)], stroke);
            painter.line_segment([p(14., 6.), p(21., 23.)], stroke);
            painter.line_segment([p(10., 17.), p(18., 17.)], stroke);
        }
        Tool::Eraser => {
            painter.add(egui::Shape::closed_line(
                vec![
                    p(6., 17.),
                    p(16., 6.),
                    p(23., 12.),
                    p(13., 23.),
                    p(10., 23.),
                ],
                stroke,
            ));
            painter.line_segment([p(10., 13.), p(17., 19.)], stroke);
            painter.line_segment([p(13., 23.), p(23., 23.)], stroke);
        }
    }
    if !hint.is_empty() {
        painter.text(
            rect.right_bottom() - egui::vec2(3., 1.),
            egui::Align2::RIGHT_BOTTOM,
            hint,
            egui::FontId::proportional(8.),
            ui.visuals().weak_text_color(),
        );
    }
    response.on_hover_text(tip).clicked()
}

fn zoom_around_point(pan: Vec2, old_zoom: f32, new_zoom: f32, anchor: Vec2) -> Vec2 {
    anchor - (anchor - pan) * (new_zoom / old_zoom)
}

fn lock_button(ui: &mut egui::Ui, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(30.0, 30.0), Sense::click());
    let painter = ui.painter();
    if active || response.hovered() {
        painter.rect_filled(rect, 5, ui.visuals().widgets.hovered.bg_fill);
    }
    let p = |x: f32, y: f32| rect.min + egui::vec2(x, y);
    let stroke = Stroke::new(1.6, ui.visuals().text_color());
    painter.rect_stroke(
        Rect::from_min_max(p(8., 14.), p(22., 24.)),
        2,
        stroke,
        egui::StrokeKind::Middle,
    );
    painter.add(egui::Shape::line(
        vec![
            p(11., 14.),
            p(11., 9.),
            p(13., 6.),
            p(17., 6.),
            p(19., 9.),
            p(19., if active { 14. } else { 10. }),
        ],
        stroke,
    ));
    response.on_hover_text("Keep tool active").clicked()
}

/// Returns true when the document's serialized drawing changed.
pub fn show(ui: &mut egui::Ui, document: &mut Document, state: &mut ExcalidrawViewState) -> bool {
    if state
        .last_text
        .as_deref()
        .is_some_and(|text| text != document.text)
    {
        state.undo.clear();
        state.redo.clear();
        state.drag = None;
        state.selected = None;
        state.text_position = None;
    }
    state.last_text = Some(document.text.clone());
    let mut drawing = match Drawing::parse(&document.text) {
        Ok(drawing) => drawing,
        Err(error) => {
            ui.horizontal(|ui| {
                ui.label(format!("Cannot open drawing: {error}"));
                if ui.button("Markdown").clicked() {
                    state.show_markdown = true;
                }
            });
            return false;
        }
    };
    let Some(mut elements) = drawing.data["elements"].as_array().cloned() else {
        ui.label("Drawing has no elements array. Open Markdown to inspect it.");
        if ui.button("Markdown").clicked() {
            state.show_markdown = true;
        }
        return false;
    };
    let colors = [
        ui.visuals().text_color(),
        Color32::from_rgb(224, 49, 49),
        Color32::from_rgb(47, 158, 68),
        Color32::from_rgb(25, 113, 194),
        Color32::from_rgb(240, 140, 0),
        Color32::from_rgb(190, 75, 170),
    ];
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        ui.add_space((ui.available_width() - 740.0).max(0.0) / 2.0);
        if lock_button(ui, state.tool_locked) {
            state.tool_locked = !state.tool_locked;
        }
        for (tool, hint, tip) in [
            (Tool::Hand, "", "Hand — H"),
            (Tool::Select, "1", "Select — V or 1"),
            (Tool::Rect, "2", "Rectangle — R or 2"),
            (Tool::Diamond, "3", "Diamond — D or 3"),
            (Tool::Ellipse, "4", "Ellipse — O or 4"),
            (Tool::Arrow, "5", "Arrow — A or 5"),
            (Tool::Line, "6", "Line — L or 6"),
            (Tool::Freedraw, "7", "Draw — P or 7"),
            (Tool::Text, "8", "Text — T or 8"),
            (Tool::Eraser, "0", "Eraser — E or 0"),
        ] {
            if tool_button(ui, tool, state.tool == tool, hint, tip) {
                state.tool = tool;
            }
        }
        if ui.button("Undo").clicked()
            && let Some(previous) = state.undo.pop()
        {
            state.redo.push(elements.clone());
            elements = previous;
            changed = true;
        }
        if ui.button("Redo").clicked()
            && let Some(next) = state.redo.pop()
        {
            state.undo.push(elements.clone());
            elements = next;
            changed = true;
        }
        if ui.button("−").clicked() {
            state.zoom = (state.zoom / 1.2).max(0.1);
        }
        if ui
            .button(format!("{:.0}%", state.zoom * 100.0))
            .on_hover_text("Reset zoom")
            .clicked()
        {
            state.zoom = 1.0;
            state.pan = Vec2::ZERO;
        }
        if ui.button("+").clicked() {
            state.zoom = (state.zoom * 1.2).min(10.0);
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.add_space((ui.available_width() - 400.0).max(0.0) / 2.0);
        for (i, color) in colors.iter().enumerate() {
            if ui
                .add(
                    egui::Button::new(egui::RichText::new("●").color(*color))
                        .selected(state.color == i),
                )
                .on_hover_text("Stroke color")
                .clicked()
            {
                state.color = i;
            }
        }
        for width in [1.0, 2.0, 4.0] {
            if ui
                .selectable_label(state.width == width, format!("{width:.0}"))
                .on_hover_text("Stroke width")
                .clicked()
            {
                state.width = width;
            }
        }
        if ui.button("Markdown").clicked() {
            state.show_markdown = true;
        }
    });
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    let step = 24.0 * state.zoom;
    if step >= 6.0 {
        let mut x = rect.left() + state.pan.x.rem_euclid(step);
        while x < rect.right() {
            let mut y = rect.top() + state.pan.y.rem_euclid(step);
            while y < rect.bottom() {
                painter.circle_filled(
                    egui::pos2(x, y),
                    1.0,
                    ui.visuals().weak_text_color().gamma_multiply(0.35),
                );
                y += step;
            }
            x += step;
        }
    }
    if response.hovered() && ui.input(|i| i.modifiers.ctrl) {
        let scroll = ui.input(|i| i.smooth_scroll_delta().y);
        if scroll != 0.0 {
            let anchor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
            let old = state.zoom;
            state.zoom = (old * (scroll * 0.002).exp()).clamp(0.1, 10.0);
            state.pan = zoom_around_point(state.pan, old, state.zoom, anchor - rect.min);
            ui.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
        }
    }
    if ui.is_enabled() && !ui.ctx().text_edit_focused() {
        for (key, tool) in [
            (Key::Num1, Tool::Select),
            (Key::V, Tool::Select),
            (Key::H, Tool::Hand),
            (Key::Num2, Tool::Rect),
            (Key::R, Tool::Rect),
            (Key::Num3, Tool::Diamond),
            (Key::D, Tool::Diamond),
            (Key::Num4, Tool::Ellipse),
            (Key::O, Tool::Ellipse),
            (Key::Num5, Tool::Arrow),
            (Key::A, Tool::Arrow),
            (Key::Num6, Tool::Line),
            (Key::L, Tool::Line),
            (Key::Num7, Tool::Freedraw),
            (Key::P, Tool::Freedraw),
            (Key::Num8, Tool::Text),
            (Key::T, Tool::Text),
            (Key::Num0, Tool::Eraser),
            (Key::E, Tool::Eraser),
            (Key::Escape, Tool::Select),
        ] {
            if ui.input(|i| i.key_pressed(key) && !i.modifiers.command) {
                state.tool = tool;
            }
        }
        let redo = ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::Z)
        }) || ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Y));
        if redo && let Some(next) = state.redo.pop() {
            state.undo.push(elements.clone());
            elements = next;
            changed = true;
        } else if !redo
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Z))
            && let Some(previous) = state.undo.pop()
        {
            state.redo.push(elements.clone());
            elements = previous;
            changed = true;
        }
        if ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
            && let Some(id) = state.selected.take()
        {
            let before = elements.clone();
            elements.retain(|e| e["id"].as_str() != Some(&id));
            if elements != before {
                remember(state, before);
                changed = true;
            }
        }
    }
    let pointer = response.interact_pointer_pos();
    if response.drag_started()
        && let Some(p) = pointer
    {
        let start = canvas_pos(
            ui.input(|i| i.pointer.press_origin()).unwrap_or(p),
            rect,
            state,
        );
        let pan = state.tool == Tool::Hand
            || ui.input(|i| i.pointer.middle_down() || i.key_down(Key::Space));
        let index = elements
            .iter()
            .rposition(|e| visible(e) && bounds(e).contains(start));
        state.drag = Some(Drag {
            start,
            original: elements.clone(),
            index,
            points: vec![start],
            pan,
        });
        if !pan && state.tool == Tool::Select {
            state.selected = index.and_then(|i| elements[i]["id"].as_str().map(str::to_owned));
        }
    }
    if let Some(p) = pointer
        && let Some(drag) = &state.drag
    {
        let current = canvas_pos(p, rect, state);
        if drag.pan {
            state.pan += response.drag_delta();
        } else if response.dragged() || response.drag_stopped() {
            let start = drag.start;
            let original = drag.original.clone();
            let index = drag.index;
            if state.tool == Tool::Select
                && let Some(index) = index
            {
                elements = original;
                elements[index]["x"] = json!(number(&elements[index], "x") + current.x - start.x);
                elements[index]["y"] = json!(number(&elements[index], "y") + current.y - start.y);
            } else if matches!(
                state.tool,
                Tool::Rect
                    | Tool::Diamond
                    | Tool::Ellipse
                    | Tool::Arrow
                    | Tool::Line
                    | Tool::Freedraw
            ) {
                if state.tool == Tool::Freedraw
                    && let Some(drag) = &mut state.drag
                {
                    drag.points.push(current);
                }
                elements = original;
                let kind = match state.tool {
                    Tool::Rect => "rectangle",
                    Tool::Diamond => "diamond",
                    Tool::Ellipse => "ellipse",
                    Tool::Arrow => "arrow",
                    Tool::Line => "line",
                    _ => "freedraw",
                };
                let linear = matches!(state.tool, Tool::Arrow | Tool::Line | Tool::Freedraw);
                let origin = if linear {
                    start
                } else {
                    egui::pos2(start.x.min(current.x), start.y.min(current.y))
                };
                let mut e = new_element(kind, origin, colors[state.color], state.width);
                e["width"] = json!(if linear {
                    current.x - start.x
                } else {
                    (current.x - start.x).abs()
                });
                e["height"] = json!(if linear {
                    current.y - start.y
                } else {
                    (current.y - start.y).abs()
                });
                if linear {
                    let points = if state.tool == Tool::Freedraw {
                        state
                            .drag
                            .as_ref()
                            .map(|d| d.points.clone())
                            .unwrap_or_default()
                    } else {
                        vec![start, current]
                    };
                    e["points"] = json!(
                        points
                            .iter()
                            .map(|p| vec![p.x - start.x, p.y - start.y])
                            .collect::<Vec<_>>()
                    );
                }
                elements.push(e);
            }
        }
    }
    if response.drag_stopped()
        && let Some(drag) = state.drag.take()
        && elements != drag.original
    {
        remember(state, drag.original);
        changed = true;
        if !state.tool_locked && state.tool != Tool::Select {
            state.tool = Tool::Select;
        }
    }
    if response.clicked()
        && let Some(p) = pointer
    {
        let p = canvas_pos(p, rect, state);
        let index = elements
            .iter()
            .rposition(|e| visible(e) && bounds(e).contains(p));
        match state.tool {
            Tool::Select => {
                state.selected = index.and_then(|i| elements[i]["id"].as_str().map(str::to_owned))
            }
            Tool::Eraser => {
                if let Some(index) = index {
                    let before = elements.clone();
                    elements.remove(index);
                    remember(state, before);
                    changed = true;
                }
            }
            Tool::Text => {
                state.text.clear();
                state.text_position = Some(p);
            }
            _ => {}
        }
    }
    for element in &elements {
        paint(ui, rect, state, element);
    }
    if let Some(p) = state.text_position {
        let pos = screen_pos(p, rect, state);
        let response = ui.put(
            Rect::from_min_size(pos, egui::vec2(200.0, 28.0)),
            egui::TextEdit::singleline(&mut state.text).id_salt("drawing-text"),
        );
        if state.text.is_empty() {
            response.request_focus();
        }
        if response.lost_focus() && !ui.input(|i| i.key_pressed(Key::Escape)) {
            if !state.text.is_empty() {
                let before = elements.clone();
                let mut e = new_element("text", p, colors[state.color], state.width);
                e["text"] = json!(state.text);
                e["originalText"] = json!(state.text);
                e["fontSize"] = json!(20);
                e["fontFamily"] = json!(1);
                e["width"] = json!(state.text.chars().count() as f32 * 11.0);
                e["height"] = json!(25);
                elements.push(e);
                remember(state, before);
                changed = true;
            }
            state.text_position = None;
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            state.text_position = None;
        }
    }
    let saved = changed && save(document, &mut drawing, &elements);
    if saved {
        state.last_text = Some(document.text.clone());
    }
    saved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_preserves_canvas_position_under_anchor() {
        for (pan, old, new, anchor) in [
            (Vec2::ZERO, 1.0, 2.0, egui::vec2(150.0, 75.0)),
            (egui::vec2(-30.0, 40.0), 0.1, 10.0, egui::vec2(1.0, 2.0)),
            (egui::vec2(90.0, -60.0), 4.0, 0.5, Vec2::ZERO),
        ] {
            let before = (anchor - pan) / old;
            let after = (anchor - zoom_around_point(pan, old, new, anchor)) / new;
            assert!((before - after).length() < 0.001);
        }
    }
}
