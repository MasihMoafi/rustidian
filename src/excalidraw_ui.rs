//! An editable vector canvas for Excalidraw documents.
use crate::theme::{self, space};
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
    resize: Option<usize>,
    selection_bounds: Option<Rect>,
}

pub struct ExcalidrawViewState {
    tool: Tool,
    tool_locked: bool,
    color: usize,
    width: f32,
    pan: Vec2,
    zoom: f32,
    selected: Vec<String>,
    drag: Option<Drag>,
    text: String,
    text_position: Option<Pos2>,
    text_id: Option<String>,
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
            selected: Vec::new(),
            drag: None,
            text: String::new(),
            text_position: None,
            text_id: None,
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
    if let Some(points) = element["points"].as_array().filter(|p| !p.is_empty()) {
        let mut r = Rect::NOTHING;
        for value in points {
            if let (Some(x), Some(y)) = (value[0].as_f64(), value[1].as_f64()) {
                r.extend_with(p + egui::vec2(x as f32, y as f32));
            }
        }
        if r.is_finite() {
            return r;
        }
    }
    Rect::from_two_pos(
        p,
        p + egui::vec2(number(element, "width"), number(element, "height")),
    )
}

fn selected(element: &Value, ids: &[String]) -> bool {
    visible(element)
        && element["id"]
            .as_str()
            .is_some_and(|id| ids.iter().any(|s| s == id))
}

fn selection_bounds(elements: &[Value], ids: &[String]) -> Option<Rect> {
    elements
        .iter()
        .filter(|e| selected(e, ids))
        .map(bounds)
        .reduce(|a, b| a.union(b))
}

fn marquee_select(elements: &[Value], area: Rect) -> Vec<String> {
    elements
        .iter()
        .filter(|e| visible(e) && area.intersects(bounds(e)))
        .filter_map(|e| e["id"].as_str().map(str::to_owned))
        .collect()
}

fn hit(element: &Value, p: Pos2, zoom: f32) -> bool {
    if matches!(
        element["type"].as_str(),
        Some("line" | "arrow" | "freedraw")
    ) {
        let origin = point(element);
        let points: Vec<Pos2> = element["points"]
            .as_array()
            .map(|points| {
                points
                    .iter()
                    .filter_map(|v| {
                        Some(origin + egui::vec2(v[0].as_f64()? as f32, v[1].as_f64()? as f32))
                    })
                    .collect()
            })
            .unwrap_or_else(|| {
                vec![
                    origin,
                    origin + egui::vec2(number(element, "width"), number(element, "height")),
                ]
            });
        return points.windows(2).any(|pair| {
            let d = pair[1] - pair[0];
            let t = if d.length_sq() > 0.0 {
                ((p - pair[0]).dot(d) / d.length_sq()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            p.distance(pair[0] + d * t) <= 8.0 / zoom
        });
    }
    bounds(element).expand(4.0 / zoom).contains(p)
}

fn resize_element(element: &mut Value, from: Rect, to: Rect) {
    let scale = egui::vec2(
        if from.width().abs() > 0.001 {
            to.width() / from.width()
        } else {
            1.0
        },
        if from.height().abs() > 0.001 {
            to.height() / from.height()
        } else {
            1.0
        },
    );
    let p = point(element);
    let next = to.min + (p - from.min) * scale;
    element["x"] = json!(next.x);
    element["y"] = json!(next.y);
    element["width"] = json!(number(element, "width") * scale.x);
    element["height"] = json!(number(element, "height") * scale.y);
    if let Some(points) = element["points"].as_array_mut() {
        for p in points {
            if let (Some(x), Some(y)) = (p[0].as_f64(), p[1].as_f64()) {
                *p = json!([x as f32 * scale.x, y as f32 * scale.y]);
            }
        }
    }
}

fn zoom_to_fit(elements: &[Value], size: Vec2) -> Option<(f32, Vec2)> {
    let bounds = elements
        .iter()
        .filter(|e| visible(e))
        .map(bounds)
        .reduce(|a, b| a.union(b))?;
    let available = (size - egui::vec2(80.0, 80.0)).max(egui::vec2(1.0, 1.0));
    let zoom = (available.x / bounds.width().max(1.0))
        .min(available.y / bounds.height().max(1.0))
        .clamp(0.1, 10.0);
    Some((zoom, size / 2.0 - bounds.center().to_vec2() * zoom))
}

fn handles(r: Rect) -> [Pos2; 8] {
    [
        r.left_top(),
        egui::pos2(r.center().x, r.top()),
        r.right_top(),
        egui::pos2(r.right(), r.center().y),
        r.right_bottom(),
        egui::pos2(r.center().x, r.bottom()),
        r.left_bottom(),
        egui::pos2(r.left(), r.center().y),
    ]
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
}

fn tool_button(ui: &mut egui::Ui, tool: Tool, active: bool, hint: &str, tip: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(space::CONTROL), Sense::click());
    let painter = ui.painter();
    if active || response.hovered() {
        painter.rect_filled(
            rect,
            theme::radius::ROW,
            if active {
                ui.visuals().widgets.active.weak_bg_fill
            } else {
                ui.visuals().widgets.hovered.bg_fill
            },
        );
    }
    let color = if active {
        theme::interactive(ui)
    } else {
        ui.visuals().text_color()
    };
    let stroke = Stroke::new(1.5, color);
    let p = |x: f32, y: f32| rect.center() + egui::vec2(x - 15.0, y - 15.0) * (space::ICON / 20.0);
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
            painter.circle_stroke(p(15., 15.), 8.5 * space::ICON / 20.0, stroke);
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
            rect.right_bottom() - egui::vec2(space::XS - space::HAIRLINE, space::XS / 2.0),
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

fn separator(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(
        egui::vec2(space::HAIRLINE, space::ICON + space::XS / 2.0),
        Sense::hover(),
    );
    ui.painter().line_segment(
        [r.center_top(), r.center_bottom()],
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
}

fn zoom_button(ui: &mut egui::Ui, plus: bool) -> egui::Response {
    let (r, response) = ui.allocate_exact_size(Vec2::splat(space::CONTROL_SM), Sense::click());
    theme::control_feedback(ui, &response);
    let stroke = Stroke::new(1.5, ui.visuals().text_color());
    let half = space::ICON / 2.0;
    ui.painter().line_segment(
        [
            r.center() - egui::vec2(half, 0.0),
            r.center() + egui::vec2(half, 0.0),
        ],
        stroke,
    );
    if plus {
        ui.painter().line_segment(
            [
                r.center() - egui::vec2(0.0, half),
                r.center() + egui::vec2(0.0, half),
            ],
            stroke,
        );
    }
    response.on_hover_text(if plus { "Zoom in" } else { "Zoom out" })
}

fn lock_button(ui: &mut egui::Ui, active: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(space::CONTROL), Sense::click());
    let painter = ui.painter();
    if active || response.hovered() {
        painter.rect_filled(
            rect,
            theme::radius::ROW,
            ui.visuals().widgets.hovered.bg_fill,
        );
    }
    let p = |x: f32, y: f32| rect.center() + egui::vec2(x - 15.0, y - 15.0) * (space::ICON / 20.0);
    let stroke = Stroke::new(1.5, ui.visuals().text_color());
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

fn history_button(ui: &mut egui::Ui, redo: bool) -> bool {
    let (r, response) = ui.allocate_exact_size(Vec2::splat(space::CONTROL_SM), Sense::click());
    if response.hovered() {
        ui.painter()
            .rect_filled(r, theme::radius::ROW, ui.visuals().widgets.hovered.bg_fill);
    }
    let p = |x: f32, y: f32| r.min + egui::vec2(if redo { 28.0 - x } else { x }, y);
    let stroke = Stroke::new(1.5, ui.visuals().text_color());
    ui.painter().add(egui::Shape::line(
        vec![p(7., 11.), p(11., 7.), p(7., 11.), p(11., 15.)],
        stroke,
    ));
    ui.painter().add(egui::Shape::line(
        vec![
            p(7., 11.),
            p(17., 11.),
            p(20., 13.),
            p(21., 16.),
            p(20., 19.),
            p(17., 21.),
            p(12., 21.),
        ],
        stroke,
    ));
    response
        .on_hover_text(if redo { "Redo" } else { "Undo" })
        .clicked()
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
        state.selected.clear();
        state.text_position = None;
        state.text_id = None;
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
    let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    egui::Area::new(ui.scope_id().with("drawing-tools"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_TOP)
        .fixed_pos(rect.center_top() + egui::vec2(0.0, space::M))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(space::margin(space::XS, space::XS))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = space::XS / 2.0;
                    ui.horizontal_centered(|ui| {
                        if lock_button(ui, state.tool_locked) {
                            state.tool_locked = !state.tool_locked;
                        }
                        separator(ui);
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
                    });
                });
        });
    egui::Area::new(ui.scope_id().with("drawing-style"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_TOP)
        .fixed_pos(
            rect.center_top()
                + egui::vec2(
                    0.0,
                    space::M + space::CONTROL + space::XS * 3.0 + space::XS / 2.0,
                ),
        )
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(space::margin(space::XS, space::XS))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = space::XS / 2.0;
                    ui.horizontal_centered(|ui| {
                        ui.set_min_height(space::CONTROL);
                        for (i, color) in colors.iter().enumerate() {
                            let (r, response) = ui.allocate_exact_size(
                                Vec2::splat(space::CONTROL_SM),
                                Sense::click(),
                            );
                            theme::control_feedback(ui, &response);
                            if state.color == i {
                                ui.painter().circle_stroke(
                                    r.center(),
                                    space::ICON / 2.0 + space::XS / 2.0,
                                    Stroke::new(space::HAIRLINE, theme::interactive(ui)),
                                );
                            }
                            ui.painter()
                                .circle_filled(r.center(), space::ICON_SM / 2.0, *color);
                            if response.on_hover_text("Stroke color").clicked() {
                                state.color = i;
                            }
                        }
                        separator(ui);
                        for (width, thickness) in [(1.0, 1.2), (2.0, 2.2), (4.0, 3.4)] {
                            let (r, response) = ui.allocate_exact_size(
                                Vec2::splat(space::CONTROL_SM),
                                Sense::click(),
                            );
                            if state.width == width {
                                ui.painter().rect_filled(
                                    r,
                                    theme::radius::ROW,
                                    ui.visuals().widgets.active.weak_bg_fill,
                                );
                            }
                            theme::control_feedback(ui, &response);
                            ui.painter().line_segment(
                                [
                                    r.center() - egui::vec2(space::ICON / 2.0, 0.0),
                                    r.center() + egui::vec2(space::ICON / 2.0, 0.0),
                                ],
                                Stroke::new(thickness, ui.visuals().text_color()),
                            );
                            if response.on_hover_text("Stroke width").clicked() {
                                state.width = width;
                            }
                        }
                    });
                });
        });
    egui::Area::new(ui.scope_id().with("drawing-markdown"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_TOP)
        .fixed_pos(rect.right_top() + egui::vec2(-space::M, space::M))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .inner_margin(space::margin(space::XS, 0.0))
                .show(ui, |ui| {
                    if ui
                        .add_sized(
                            [space::CONTROL * 3.0, space::CONTROL],
                            egui::Button::new("Markdown").frame_when_inactive(false),
                        )
                        .clicked()
                    {
                        state.show_markdown = true;
                    }
                });
        });
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
    if response.hovered() {
        let (scroll, pinch, ctrl, shift) = ui.input(|i| {
            (
                i.smooth_scroll_delta(),
                i.zoom_delta(),
                i.modifiers.ctrl,
                i.modifiers.shift,
            )
        });
        if ctrl || pinch != 1.0 {
            let anchor = ui.input(|i| i.pointer.hover_pos()).unwrap_or(rect.center());
            let old = state.zoom;
            state.zoom = (old
                * if pinch != 1.0 {
                    pinch
                } else {
                    (scroll.y * 0.002).exp()
                })
            .clamp(0.1, 10.0);
            state.pan = zoom_around_point(state.pan, old, state.zoom, anchor - rect.min);
        } else if shift {
            state.pan += egui::vec2(scroll.x + scroll.y, 0.0);
        } else {
            state.pan += scroll;
        }
        if scroll != Vec2::ZERO {
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
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            state.selected.clear();
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::A)) {
            state.selected = elements
                .iter()
                .filter(|e| visible(e))
                .filter_map(|e| e["id"].as_str().map(str::to_owned))
                .collect();
        }
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::D)) {
            let before = elements.clone();
            let mut copies: Vec<Value> = elements
                .iter()
                .filter(|e| selected(e, &state.selected))
                .cloned()
                .collect();
            state.selected.clear();
            for e in &mut copies {
                let id = format!("{:016x}", fastrand::u64(..));
                e["id"] = json!(id);
                e["x"] = json!(number(e, "x") + 12.0);
                e["y"] = json!(number(e, "y") + 12.0);
                state.selected.push(id);
            }
            if !copies.is_empty() {
                elements.extend(copies);
                remember(state, before);
                changed = true;
            }
        }
        let mut delta = Vec2::ZERO;
        let step = if ui.input(|i| i.modifiers.shift) {
            10.0
        } else {
            1.0
        };
        for (key, d) in [
            (Key::ArrowLeft, egui::vec2(-step, 0.0)),
            (Key::ArrowRight, egui::vec2(step, 0.0)),
            (Key::ArrowUp, egui::vec2(0.0, -step)),
            (Key::ArrowDown, egui::vec2(0.0, step)),
        ] {
            if ui.input(|i| i.key_pressed(key) && !i.modifiers.command) {
                delta += d;
            }
        }
        if delta != Vec2::ZERO && !state.selected.is_empty() {
            let before = elements.clone();
            for e in elements.iter_mut().filter(|e| selected(e, &state.selected)) {
                e["x"] = json!(number(e, "x") + delta.x);
                e["y"] = json!(number(e, "y") + delta.y);
            }
            if elements != before {
                remember(state, before);
                changed = true;
            }
        }
        if ui.input(|i| i.key_pressed(Key::Delete) || i.key_pressed(Key::Backspace))
            && !state.selected.is_empty()
        {
            let before = elements.clone();
            elements.retain(|e| !selected(e, &state.selected));
            state.selected.clear();
            if elements != before {
                remember(state, before);
                changed = true;
            }
        }
        let reset_zoom = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Num0));
        let zoom_in = ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND, Key::Plus)
                || i.consume_key(egui::Modifiers::COMMAND, Key::Equals)
        });
        let zoom_out = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Minus));
        if reset_zoom {
            state.zoom = 1.0;
            state.pan = rect.size() / 2.0;
        } else if zoom_in || zoom_out {
            let factor = if zoom_out { 1.0 / 1.2 } else { 1.2 };
            let old = state.zoom;
            state.zoom = (old * factor).clamp(0.1, 10.0);
            state.pan = zoom_around_point(state.pan, old, state.zoom, rect.size() / 2.0);
        } else if ui.input(|i| i.modifiers.shift && i.key_pressed(Key::Num1))
            && let Some((zoom, pan)) = zoom_to_fit(&elements, rect.size())
        {
            state.zoom = zoom;
            state.pan = pan;
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
            .rposition(|e| visible(e) && hit(e, start, state.zoom));
        let selection = selection_bounds(&elements, &state.selected);
        let resize = if !pan && state.tool == Tool::Select {
            selection.and_then(|r| {
                handles(r)
                    .iter()
                    .position(|h| h.distance(start) <= 7.0 / state.zoom)
            })
        } else {
            None
        };
        if !pan && state.tool == Tool::Select && resize.is_none() {
            if let Some(index) = index {
                if let Some(id) = elements[index]["id"].as_str()
                    && !state.selected.iter().any(|s| s == id)
                {
                    if !ui.input(|i| i.modifiers.shift) {
                        state.selected.clear();
                    }
                    state.selected.push(id.to_owned());
                }
            } else if !selection.is_some_and(|r| r.contains(start)) {
                state.selected.clear();
            }
        }
        state.drag = Some(Drag {
            start,
            original: elements.clone(),
            index,
            points: vec![start],
            pan,
            resize,
            selection_bounds: selection_bounds(&elements, &state.selected),
        });
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
                && let Some(handle) = drag.resize
                && let Some(from) = drag.selection_bounds
            {
                let delta = current - start;
                let mut to = from;
                if matches!(handle, 0 | 6 | 7) {
                    to.min.x = (from.min.x + delta.x).min(from.max.x - 0.001);
                }
                if matches!(handle, 2..=4) {
                    to.max.x = (from.max.x + delta.x).max(from.min.x + 0.001);
                }
                if matches!(handle, 0..=2) {
                    to.min.y = (from.min.y + delta.y).min(from.max.y - 0.001);
                }
                if matches!(handle, 4..=6) {
                    to.max.y = (from.max.y + delta.y).max(from.min.y + 0.001);
                }
                elements = original;
                for e in elements.iter_mut().filter(|e| selected(e, &state.selected)) {
                    resize_element(e, from, to);
                }
            } else if state.tool == Tool::Select
                && (index.is_some() || drag.selection_bounds.is_some_and(|r| r.contains(start)))
            {
                elements = original;
                for e in elements.iter_mut().filter(|e| selected(e, &state.selected)) {
                    e["x"] = json!(number(e, "x") + current.x - start.x);
                    e["y"] = json!(number(e, "y") + current.y - start.y);
                }
            } else if state.tool == Tool::Select {
                state.selected = marquee_select(&elements, Rect::from_two_pos(start, current));
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
            .rposition(|e| visible(e) && hit(e, p, state.zoom));
        match state.tool {
            Tool::Select => {
                if let Some(id) = index.and_then(|i| elements[i]["id"].as_str()) {
                    if ui.input(|i| i.modifiers.shift) {
                        if let Some(i) = state.selected.iter().position(|s| s == id) {
                            state.selected.remove(i);
                        } else {
                            state.selected.push(id.to_owned());
                        }
                    } else {
                        state.selected = vec![id.to_owned()];
                    }
                    if response.double_clicked()
                        && let Some(index) = index
                        && elements[index]["type"] == "text"
                    {
                        state.text = elements[index]["text"].as_str().unwrap_or("").to_owned();
                        state.text_position = Some(point(&elements[index]));
                        state.text_id = Some(id.to_owned());
                    }
                } else {
                    state.selected.clear();
                }
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
                state.text_id = None;
            }
            _ => {}
        }
    }
    for element in &elements {
        if state.text_id.as_deref() != element["id"].as_str() || state.text_id.is_none() {
            paint(ui, rect, state, element);
        }
    }
    if let Some(r) = selection_bounds(&elements, &state.selected) {
        let screen = Rect::from_two_pos(
            screen_pos(r.min, rect, state),
            screen_pos(r.max, rect, state),
        );
        let accent = Color32::from_rgb(105, 101, 219);
        painter.rect_stroke(
            screen,
            0,
            Stroke::new(1.0, accent),
            egui::StrokeKind::Middle,
        );
        for p in handles(screen) {
            painter.rect_filled(
                Rect::from_center_size(p, egui::vec2(8.0, 8.0)),
                0,
                Color32::WHITE,
            );
            painter.rect_stroke(
                Rect::from_center_size(p, egui::vec2(8.0, 8.0)),
                0,
                Stroke::new(1.0, accent),
                egui::StrokeKind::Middle,
            );
        }
    }
    if let Some(drag) = &state.drag
        && !drag.pan
        && drag.index.is_none()
        && drag.selection_bounds.is_none()
        && state.tool == Tool::Select
        && let Some(p) = pointer
    {
        let r = Rect::from_two_pos(screen_pos(drag.start, rect, state), p);
        painter.rect_filled(r, 0, Color32::from_rgba_unmultiplied(105, 101, 219, 35));
        painter.rect_stroke(
            r,
            0,
            Stroke::new(1.0, Color32::from_rgb(105, 101, 219)),
            egui::StrokeKind::Middle,
        );
    }
    if response.hovered() {
        let cursor = if state.drag.as_ref().is_some_and(|d| d.pan) {
            egui::CursorIcon::Grabbing
        } else if state.tool == Tool::Hand || ui.input(|i| i.key_down(Key::Space)) {
            egui::CursorIcon::Grab
        } else if state.tool == Tool::Select {
            let p = ui
                .input(|i| i.pointer.hover_pos())
                .map(|p| canvas_pos(p, rect, state));
            let handle = p.and_then(|p| {
                selection_bounds(&elements, &state.selected).and_then(|r| {
                    handles(r)
                        .iter()
                        .position(|h| h.distance(p) <= 7.0 / state.zoom)
                })
            });
            match handle {
                Some(0 | 4) => egui::CursorIcon::ResizeNwSe,
                Some(2 | 6) => egui::CursorIcon::ResizeNeSw,
                Some(1 | 5) => egui::CursorIcon::ResizeVertical,
                Some(3 | 7) => egui::CursorIcon::ResizeHorizontal,
                _ if p.is_some_and(|p| {
                    selection_bounds(&elements, &state.selected).is_some_and(|r| r.contains(p))
                }) =>
                {
                    egui::CursorIcon::Move
                }
                _ => egui::CursorIcon::Default,
            }
        } else {
            egui::CursorIcon::Crosshair
        };
        ui.ctx().set_cursor_icon(cursor);
    }
    if let Some(p) = state.text_position {
        let pos = screen_pos(p, rect, state);
        let response = ui.put(
            Rect::from_min_size(pos, egui::vec2(200.0, 28.0)),
            egui::TextEdit::singleline(&mut state.text).id_salt("drawing-text"),
        );
        if !response.has_focus() && !response.lost_focus() {
            response.request_focus();
        }
        if response.lost_focus() && !ui.input(|i| i.key_pressed(Key::Escape)) {
            if !state.text.is_empty() || state.text_id.is_some() {
                let before = elements.clone();
                let existing = state
                    .text_id
                    .as_deref()
                    .and_then(|id| elements.iter().position(|e| e["id"].as_str() == Some(id)));
                let mut e = existing
                    .map(|index| elements[index].clone())
                    .unwrap_or_else(|| new_element("text", p, colors[state.color], state.width));
                e["text"] = json!(state.text);
                e["originalText"] = json!(state.text);
                if existing.is_none() {
                    e["fontSize"] = json!(20);
                    e["fontFamily"] = json!(1);
                }
                e["width"] = json!(state.text.chars().count() as f32 * 11.0);
                e["height"] = json!(25);
                if let Some(index) = existing {
                    elements[index] = e;
                } else {
                    elements.push(e);
                }
                remember(state, before);
                changed = true;
            }
            state.text_position = None;
            state.text_id = None;
        }
        if ui.input(|i| i.key_pressed(Key::Escape)) {
            state.text_position = None;
            state.text_id = None;
            state.tool = Tool::Select;
        }
    }
    egui::Area::new(ui.scope_id().with("drawing-controls"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(rect.left_bottom() + egui::vec2(space::M, -space::M))
        .show(ui.ctx(), |ui| {
            ui.spacing_mut().item_spacing.x = space::S;
            ui.spacing_mut().interact_size.y = space::CONTROL_SM;
            ui.horizontal_centered(|ui| {
                egui::Frame::popup(ui.style())
                    .inner_margin(crate::theme::space::margin(
                        crate::theme::space::XS,
                        crate::theme::space::XS,
                    ))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.spacing_mut().interact_size.y = space::CONTROL_SM;
                        ui.horizontal_centered(|ui| {
                            if zoom_button(ui, false).clicked() {
                                let old = state.zoom;
                                state.zoom = (old / 1.2).max(0.1);
                                state.pan = zoom_around_point(
                                    state.pan,
                                    old,
                                    state.zoom,
                                    rect.size() / 2.0,
                                );
                            }
                            if ui
                                .add_sized(
                                    [space::CONTROL_SM + space::XL, space::CONTROL_SM],
                                    egui::Button::new(format!("{:.0}%", state.zoom * 100.0))
                                        .frame_when_inactive(false),
                                )
                                .on_hover_text("Reset zoom")
                                .clicked()
                            {
                                state.zoom = 1.0;
                                state.pan = rect.size() / 2.0;
                            }
                            if zoom_button(ui, true).clicked() {
                                let old = state.zoom;
                                state.zoom = (old * 1.2).min(10.0);
                                state.pan = zoom_around_point(
                                    state.pan,
                                    old,
                                    state.zoom,
                                    rect.size() / 2.0,
                                );
                            }
                        });
                    });
                egui::Frame::popup(ui.style())
                    .inner_margin(crate::theme::space::margin(
                        crate::theme::space::XS,
                        crate::theme::space::XS,
                    ))
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.spacing_mut().interact_size.y = space::CONTROL_SM;
                        ui.horizontal_centered(|ui| {
                            if history_button(ui, false)
                                && let Some(previous) = state.undo.pop()
                            {
                                state.redo.push(elements.clone());
                                elements = previous;
                                changed = true;
                            }
                            if history_button(ui, true)
                                && let Some(next) = state.redo.pop()
                            {
                                state.undo.push(elements.clone());
                                elements = next;
                                changed = true;
                            }
                        });
                    });
            });
        });
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

    #[test]
    fn fit_centers_visible_elements_and_handles_empty_drawings() {
        assert!(zoom_to_fit(&[], egui::vec2(400.0, 300.0)).is_none());
        let elements = vec![
            json!({"x": 10, "y": 20, "width": 100, "height": 50}),
            json!({"x": 9000, "y": 9000, "width": 1, "height": 1, "isDeleted": true}),
        ];
        let (zoom, pan) = zoom_to_fit(&elements, egui::vec2(400.0, 300.0)).unwrap();
        assert!((zoom - 3.2).abs() < 0.001);
        assert!((pan + egui::vec2(60.0, 45.0) * zoom - egui::vec2(200.0, 150.0)).length() < 0.001);
    }

    #[test]
    fn marquee_finds_intersections_and_ignores_deleted_elements() {
        let elements = vec![
            json!({"id":"a", "x":0, "y":0, "width":10, "height":10}),
            json!({"id":"b", "x":30, "y":30, "width":5, "height":5}),
            json!({"id":"c", "x":0, "y":0, "width":5, "height":5, "isDeleted":true}),
        ];
        assert_eq!(
            marquee_select(
                &elements,
                Rect::from_min_max(egui::pos2(9.0, 9.0), egui::pos2(12.0, 12.0))
            ),
            vec!["a"]
        );
        assert!(
            marquee_select(
                &elements,
                Rect::from_min_max(egui::pos2(15.0, 15.0), egui::pos2(20.0, 20.0))
            )
            .is_empty()
        );
    }

    #[test]
    fn resizing_scales_relative_points_and_zero_dimensions_remain_finite() {
        let mut element = json!({"x":10, "y":20, "width":10, "height":0, "points":[[0,0],[10,0]]});
        resize_element(
            &mut element,
            Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(20.0, 20.0)),
            Rect::from_min_max(egui::pos2(5.0, 10.0), egui::pos2(25.0, 10.0)),
        );
        assert_eq!(number(&element, "x"), 5.0);
        assert_eq!(number(&element, "width"), 20.0);
        assert_eq!(element["points"][1][0].as_f64(), Some(20.0));
        assert_eq!(number(&element, "height"), 0.0);
    }

    #[test]
    fn line_hit_testing_uses_segments() {
        let line = json!({"type":"line", "x":0, "y":0, "width":100, "height":100, "points":[[0,0],[100,100]]});
        assert!(hit(&line, egui::pos2(50.0, 53.0), 1.0));
        assert!(!hit(&line, egui::pos2(10.0, 90.0), 1.0));
        assert!(!hit(&line, egui::pos2(50.0, 57.0), 2.0));
    }
}
