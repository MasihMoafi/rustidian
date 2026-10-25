use crate::{theme, ui_probe};
use eframe::egui::{
    self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};
use rustidian::{
    graph::{Graph, Group, Kind, Options, View},
    graph_layout::{Camera, Forces, Geometry},
    vault::Vault,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

pub enum Action {
    Open { path: PathBuf, kind: Kind },
    CloseLocal,
}

#[derive(Clone, PartialEq)]
struct Key {
    generation: u64,
    options: Options,
    forces: Forces,
    root: Option<PathBuf>,
}

struct Ready {
    key: Key,
    local: bool,
    graph: Arc<Graph>,
    result: Result<Scene, String>,
}

struct Scene {
    view: View,
    geometry: Geometry,
    degree: Vec<usize>,
    labels: Vec<usize>,
}

struct Display {
    arrows: bool,
    fade: f32,
    size: f32,
    thickness: f32,
    colors: ColorMode,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum ColorMode {
    Groups,
    #[default]
    LinkDirection,
}

impl ColorMode {
    fn label(self) -> &'static str {
        match self {
            Self::Groups => "Groups",
            Self::LinkDirection => "Link direction",
        }
    }
}

impl Default for Display {
    fn default() -> Self {
        Self {
            arrows: false,
            fade: 0.0,
            size: 1.0,
            thickness: 1.0,
            colors: ColorMode::default(),
        }
    }
}

struct Pane {
    options: Options,
    forces: Forces,
    display: Display,
    camera: Camera,
    anchor: Option<PathBuf>,
    settings: bool,
    sections: [bool; 4],
    requested: Option<Key>,
    changed: Instant,
    ready: Option<Arc<Ready>>,
    fit: bool,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            options: Default::default(),
            forces: Default::default(),
            display: Default::default(),
            camera: Default::default(),
            anchor: None,
            settings: true,
            sections: [false, false, true, false],
            requested: None,
            changed: Instant::now(),
            ready: None,
            fit: true,
        }
    }
}

#[derive(Default)]
pub struct Manager {
    pub local_open: bool,
    global: Pane,
    local: Pane,
    source: Option<Arc<Vault>>,
    graph: Option<Arc<Graph>>,
    worker: Option<Receiver<Ready>>,
    generation: u64,
}

impl Manager {
    pub fn set_active_note(&mut self, path: Option<&std::path::Path>) {
        self.global.anchor = path.map(std::path::Path::to_owned);
    }

    pub fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.source = None;
        self.graph = None;
    }

    pub fn reset(&mut self) {
        self.invalidate();
        self.local_open = false;
        self.global = Pane::default();
        self.local = Pane::default();
    }

    fn poll(&mut self) {
        let result = self
            .worker
            .as_ref()
            .and_then(|worker| worker.try_recv().ok());
        if let Some(ready) = result {
            self.worker = None;
            if ready.key.generation == self.generation {
                self.graph = Some(ready.graph.clone());
                let pane = if ready.local {
                    &mut self.local
                } else {
                    &mut self.global
                };
                if pane
                    .ready
                    .as_ref()
                    .is_none_or(|old| old.key.root != ready.key.root)
                {
                    pane.fit = true;
                }
                pane.ready = Some(Arc::new(ready));
            }
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        vault: &Vault,
        root: Option<PathBuf>,
        local: bool,
        blocked: bool,
    ) -> Option<Action> {
        let ctx = ui.ctx().clone();
        ui_probe::scope(
            &ctx,
            if local {
                "local-graph:"
            } else {
                "global-graph:"
            },
            || self.show_inner(ui, vault, root, local, blocked),
        )
    }

    fn show_inner(
        &mut self,
        ui: &mut egui::Ui,
        vault: &Vault,
        root: Option<PathBuf>,
        local: bool,
        blocked: bool,
    ) -> Option<Action> {
        self.poll();
        theme::menu_style(ui);
        if self.source.is_none() {
            self.source = Some(Arc::new(vault.clone()));
        }
        let mut pane = if local {
            std::mem::take(&mut self.local)
        } else {
            std::mem::take(&mut self.global)
        };
        let mut action = None;
        ui.add_enabled_ui(!blocked, |ui| {
            ui.horizontal(|ui| {
                let title = root.as_deref().map_or_else(
                    || "Graph view".to_owned(),
                    |path| format!("Graph of {}", rustidian::vault::file_title(path)),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if local {
                        let close = theme::icon_button(ui, theme::Icon::Close, "Close local graph");
                        ui_probe::record(ui.ctx(), "close", close.rect);
                        if close.clicked() {
                            action = Some(Action::CloseLocal);
                        }
                    }
                    let settings = ui.selectable_label(pane.settings, "Settings");
                    ui_probe::record(ui.ctx(), "settings", settings.rect);
                    if settings.clicked() {
                        pane.settings = !pane.settings;
                    }
                    let fit = ui.button("Fit");
                    ui_probe::record(ui.ctx(), "fit", fit.rect);
                    if fit.clicked() {
                        pane.fit = true;
                    }
                    ui.add(
                        egui::Label::new(RichText::new(title).size(13.0))
                            .truncate()
                            .halign(Align::Min),
                    );
                });
            })
        });
        ui.separator();
        let (rect, response) = ui.allocate_exact_size(
            ui.available_size().max(Vec2::splat(1.0)),
            Sense::click_and_drag(),
        );
        ui_probe::record(ui.ctx(), "canvas", rect);
        let panel_width = 240.0_f32.min((rect.width() - 20.0).max(100.0));
        if pane.settings && !blocked {
            let panel_height = (rect.height() - 44.0).max(40.0);
            egui::Area::new(ui.scope_id().with(("graph-settings", local)))
                .fixed_pos(egui::pos2(
                    rect.right() - panel_width - 10.0,
                    rect.top() + 10.0,
                ))
                .order(egui::Order::Foreground)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style())
                        .fill(ui.visuals().panel_fill)
                        .corner_radius(8)
                        .inner_margin(egui::Margin::symmetric(12, 6))
                        .show(ui, |ui| {
                            ui.set_width((panel_width - 24.0).max(76.0));
                            ui.set_max_height(panel_height);
                            egui::ScrollArea::vertical()
                                .id_salt("graph-settings-scroll")
                                .max_height(panel_height)
                                .show(ui, |ui| settings(ui, &mut pane, local));
                        });
                });
        }
        let key = Key {
            generation: self.generation,
            options: pane.options.clone(),
            forces: pane.forces,
            root,
        };
        if pane.requested.as_ref() != Some(&key) {
            pane.changed = Instant::now();
            pane.requested = Some(key.clone());
        }
        let pending = pane.ready.as_ref().is_none_or(|ready| ready.key != key);
        if pending && self.worker.is_none() {
            if pane.ready.is_none() || pane.changed.elapsed() >= Duration::from_millis(120) {
                let source = self.source.as_ref().unwrap().clone();
                let graph = self.graph.clone();
                let request = key.clone();
                let ctx = ui.ctx().clone();
                let (sender, receiver) = mpsc::channel();
                self.worker = Some(receiver);
                std::thread::spawn(move || {
                    let graph = graph.unwrap_or_else(|| Arc::new(Graph::from_vault(&source)));
                    let result = graph
                        .visible(&source, &request.options, request.root.as_deref())
                        .map(|view| {
                            let center = request.root.as_deref().and_then(|path| graph.node(path));
                            let geometry = Geometry::compute(&graph, &view, request.forces, center);
                            let mut degree = vec![0; graph.nodes.len()];
                            for edge in &view.edges {
                                degree[edge.from] += 1;
                                degree[edge.to] += 1;
                            }
                            let mut labels = view.nodes.clone();
                            labels.sort_by_key(|&index| std::cmp::Reverse(degree[index]));
                            Scene {
                                view,
                                geometry,
                                degree,
                                labels,
                            }
                        });
                    let _ = sender.send(Ready {
                        key: request,
                        local,
                        graph,
                        result,
                    });
                    ctx.request_repaint();
                });
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(120));
            }
        }
        ui_probe::metric(ui.ctx(), "busy", if pending { 1.0 } else { 0.0 });
        ui_probe::metric(ui.ctx(), "depth", pane.options.depth as f32);
        let painter = ui.painter().with_clip_rect(rect);
        if let Some(ready) = &pane.ready {
            match &ready.result {
                Ok(scene) => {
                    let Scene { view, geometry, .. } = scene;
                    if pane.fit {
                        let reserved = if pane.settings {
                            panel_width + 20.0
                        } else {
                            0.0
                        };
                        pane.camera.fit(
                            geometry.bounds,
                            [(rect.width() - reserved).max(1.0), rect.height()],
                        );
                        pane.camera.pan[0] -= reserved * 0.5;
                        pane.fit = false;
                    }
                    if !blocked && response.hovered() {
                        let wheel = ui.ctx().input(|input| input.smooth_scroll_delta().y);
                        if wheel != 0.0 {
                            let pointer =
                                response.hover_pos().unwrap_or(rect.center()) - rect.center();
                            pane.camera
                                .zoom_at((wheel * 0.003).exp(), [pointer.x, pointer.y]);
                            ui.ctx()
                                .input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
                        }
                    }
                    if !blocked && response.dragged() {
                        let delta = ui.ctx().input(|input| input.pointer.delta());
                        pane.camera.pan[0] += delta.x;
                        pane.camera.pan[1] += delta.y;
                    }
                    let open = paint(
                        ui,
                        &painter,
                        &response,
                        ready,
                        scene,
                        &pane,
                        !pending && !blocked,
                    );
                    if open.is_some() {
                        action = open;
                    }
                    ui_probe::metric(ui.ctx(), "node-count", view.nodes.len() as f32);
                    ui_probe::metric(ui.ctx(), "edge-count", view.edges.len() as f32);
                    ui_probe::metric(ui.ctx(), "layout-iterations", geometry.iterations as f32);
                    if view.nodes.is_empty() {
                        painter.text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "No matching notes",
                            FontId::proportional(15.0),
                            ui.visuals().weak_text_color(),
                        );
                    }
                }
                Err(error) => {
                    let text = egui::WidgetText::from(
                        RichText::new(error).color(Color32::from_rgb(220, 135, 110)),
                    );
                    let galley = text.into_galley(
                        ui,
                        Some(egui::TextWrapMode::Wrap),
                        (rect.width() - 40.0).max(40.0),
                        FontId::proportional(13.0),
                    );
                    painter.galley(
                        rect.center() - galley.size() * 0.5,
                        galley,
                        ui.visuals().text_color(),
                    );
                }
            }
        }
        if pending {
            painter.text(
                rect.left_bottom() + egui::vec2(12.0, -12.0),
                egui::Align2::LEFT_BOTTOM,
                "Updating graph…",
                FontId::proportional(12.0),
                ui.visuals().weak_text_color(),
            );
        }
        ui_probe::metric(ui.ctx(), "zoom", pane.camera.zoom);
        ui_probe::metric(ui.ctx(), "pan-x", pane.camera.pan[0]);
        ui_probe::metric(ui.ctx(), "pan-y", pane.camera.pan[1]);
        if local {
            self.local = pane;
        } else {
            self.global = pane;
        }
        action
    }
}

fn toggle(ui: &mut egui::Ui, value: &mut bool, label: &str, probe: &str) {
    let response = ui.checkbox(value, label);
    ui_probe::record(ui.ctx(), probe, response.rect);
}

fn section(ui: &mut egui::Ui, open: &mut bool, label: &str) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), Sense::click());
    ui_probe::record(
        ui.ctx(),
        format!("section:{}", label.to_lowercase()),
        response.rect,
    );
    if response.clicked() {
        *open = !*open;
    }
    let color = if response.hovered() {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    theme::icon_image(theme::Icon::Disclosure, color)
        .rotate(
            if *open {
                std::f32::consts::FRAC_PI_2
            } else {
                0.0
            },
            Vec2::splat(0.5),
        )
        .paint_at(
            ui,
            Rect::from_center_size(rect.left_center() + egui::vec2(5.0, 0.0), Vec2::splat(12.0)),
        );
    ui.painter().text(
        rect.left_center() + egui::vec2(20.0, 0.0),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        ui.visuals().text_color(),
    );
    *open
}

fn slider(ui: &mut egui::Ui, slider: egui::Slider<'_>, probe: &str) {
    ui.push_id(probe, |ui| {
        let response = ui.add(slider);
        let rail_key = ui.scope_id().with("rail");
        // Slider returns the number-field ID while it is edited. Retain the real
        // rail ID so both input paths keep their native behavior.
        if let Some(native) = ui.ctx().read_response(response.id)
            && !native.sense.senses_click()
        {
            ui.data_mut(|data| data.insert_temp(rail_key, native.id));
        }
        if let Some(rail) = ui
            .data(|data| data.get_temp::<egui::Id>(rail_key))
            .and_then(|id| ui.ctx().read_response(id))
        {
            // Sense clicks as well as drags: a complete press/release in one
            // frame must update the rail, even when no drag was started.
            let rail = rail.interact(Sense::click());
            if rail.clicked() || rail.is_pointer_button_down_on() {
                rail.request_focus();
            }
        }
        ui_probe::record(ui.ctx(), probe, response.rect);
    });
}

fn settings(ui: &mut egui::Ui, pane: &mut Pane, local: bool) {
    ui.spacing_mut().item_spacing = egui::vec2(6.0, 8.0);
    ui.spacing_mut().slider_width = (ui.available_width() - 50.0).clamp(36.0, 110.0);
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    if section(ui, &mut pane.sections[0], "Filters") {
        let filter = ui.add(
            egui::TextEdit::singleline(&mut pane.options.filter)
                .hint_text("Search files…")
                .desired_width(f32::INFINITY),
        );
        ui_probe::record(ui.ctx(), "filter", filter.rect);
        if local {
            ui.label("Depth");
            slider(
                ui,
                egui::Slider::new(&mut pane.options.depth, 1..=5),
                "depth-slider",
            );
            toggle(ui, &mut pane.options.incoming, "Incoming links", "incoming");
            toggle(ui, &mut pane.options.outgoing, "Outgoing links", "outgoing");
            toggle(
                ui,
                &mut pane.options.neighbor_links,
                "Neighbor links",
                "neighbor",
            );
        }
        toggle(ui, &mut pane.options.tags, "Tags", "tags");
        toggle(
            ui,
            &mut pane.options.attachments,
            "Attachments",
            "attachments",
        );
        toggle(
            ui,
            &mut pane.options.existing_only,
            "Existing files only",
            "existing-only",
        );
        if !local {
            toggle(ui, &mut pane.options.orphans, "Orphans", "orphans");
        }
        ui.add_space(4.0);
    }
    if section(ui, &mut pane.sections[1], "Groups") {
        let mut remove = None;
        let mut reorder = None;
        let group_count = pane.options.groups.len();
        let owner = ui.scope_id();
        for (index, group) in pane.options.groups.iter_mut().enumerate() {
            let row = ui.push_id(index, |ui| {
                ui.horizontal(|ui| {
                    let query = ui.add(
                        egui::TextEdit::singleline(&mut group.query)
                            .desired_width((ui.available_width() - 90.0).max(30.0))
                            .hint_text("Search query"),
                    );
                    ui_probe::record(ui.ctx(), format!("group:{index}:query"), query.rect);
                    let mut rgb = [group.color[0], group.color[1], group.color[2]];
                    let color = ui.color_edit_button_srgb(&mut rgb);
                    ui_probe::record(ui.ctx(), format!("group:{index}:color"), color.rect);
                    group.color = [rgb[0], rgb[1], rgb[2], 255];
                    let delete = theme::icon_button(ui, theme::Icon::Close, "Delete group");
                    ui_probe::record(ui.ctx(), format!("group:{index}:delete"), delete.rect);
                    if delete.clicked() {
                        remove = Some(index);
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    let drag =
                        ui.dnd_drag_source(ui.scope_id().with("drag"), (owner, index), |ui| {
                            ui.small("Drag")
                        });
                    ui_probe::record(ui.ctx(), format!("group:{index}:drag"), drag.response.rect);
                    if index > 0 && ui.small_button("Move up").clicked() {
                        reorder = Some((index, index - 1));
                    }
                    if index + 1 < group_count && ui.small_button("Move down").clicked() {
                        reorder = Some((index, index + 1));
                    }
                });
            });
            if let Some(payload) = row.response.dnd_release_payload::<(egui::Id, usize)>()
                && payload.0 == owner
                && payload.1 < group_count
                && payload.1 != index
            {
                reorder = Some((payload.1, index));
            }
        }
        if let Some(index) = remove {
            pane.options.groups.remove(index);
        } else if let Some((from, to)) = reorder {
            let group = pane.options.groups.remove(from);
            pane.options.groups.insert(to, group);
        }
        let add = ui.button("New group");
        ui_probe::record(ui.ctx(), "group-add", add.rect);
        if add.clicked() {
            pane.display.colors = ColorMode::Groups;
            pane.options.groups.push(Group {
                query: String::new(),
                color: [224, 82, 82, 255],
            });
        }
        ui.add_space(4.0);
    }
    if section(ui, &mut pane.sections[2], "Display") {
        ui.label("Color by");
        let color_mode = egui::ComboBox::from_id_salt("graph-color-mode")
            .selected_text(pane.display.colors.label())
            .show_ui(ui, |ui| {
                for mode in [ColorMode::Groups, ColorMode::LinkDirection] {
                    let response =
                        ui.selectable_value(&mut pane.display.colors, mode, mode.label());
                    ui_probe::record(
                        ui.ctx(),
                        format!("color-mode:{}", mode.label()),
                        response.rect,
                    );
                }
            });
        ui_probe::record(ui.ctx(), "color-mode", color_mode.response.rect);
        if pane.display.colors == ColorMode::LinkDirection {
            for (label, relation) in [("Incoming", 1), ("Outgoing", 2), ("Both directions", 3)] {
                ui.colored_label(direction_color(ui, relation), label);
            }
            ui.small(if local {
                "Relative to the current note"
            } else {
                "Current note; hover another to inspect its connections"
            });
        }
        toggle(ui, &mut pane.display.arrows, "Arrows", "arrows");
        for (label, value, range, probe) in [
            (
                "Text fade threshold",
                &mut pane.display.fade,
                -3.0..=3.0,
                "text-fade",
            ),
            ("Node size", &mut pane.display.size, 0.1..=5.0, "node-size"),
            (
                "Link thickness",
                &mut pane.display.thickness,
                0.1..=5.0,
                "link-thickness",
            ),
        ] {
            ui.label(label);
            slider(ui, egui::Slider::new(value, range), probe);
        }
        ui.add_space(4.0);
    }
    if section(ui, &mut pane.sections[3], "Forces") {
        for (label, value, range, probe) in [
            (
                "Center force",
                &mut pane.forces.center,
                0.0..=1.0,
                "force-center",
            ),
            (
                "Repel force",
                &mut pane.forces.repel,
                0.0..=20.0,
                "force-repel",
            ),
            ("Link force", &mut pane.forces.link, 0.0..=1.0, "force-link"),
            (
                "Link distance",
                &mut pane.forces.distance,
                30.0..=500.0,
                "force-distance",
            ),
        ] {
            ui.label(label);
            slider(ui, egui::Slider::new(value, range), probe);
        }
    }
    ui.separator();
    let reset = ui.button("Restore default settings");
    ui_probe::record(ui.ctx(), "reset", reset.rect);
    if reset.clicked() {
        pane.options = Options::default();
        pane.forces = Forces::default();
        pane.display = Display::default();
        pane.fit = true;
    }
}

fn direction_color(ui: &egui::Ui, relation: u8) -> Color32 {
    let [red, green, blue] = match (ui.visuals().dark_mode, relation) {
        (true, 1) => [112, 173, 237],
        (true, 2) => [112, 207, 157],
        (true, _) => [191, 154, 232],
        (false, 1) => [38, 96, 171],
        (false, 2) => [32, 119, 77],
        (false, _) => [116, 67, 164],
    };
    Color32::from_rgb(red, green, blue)
}

fn paint(
    ui: &egui::Ui,
    painter: &egui::Painter,
    response: &egui::Response,
    ready: &Ready,
    scene: &Scene,
    pane: &Pane,
    interactive: bool,
) -> Option<Action> {
    let rect = response.rect;
    let Scene {
        view,
        geometry,
        degree,
        labels,
    } = scene;
    let screen = |node: usize| {
        let p = pane.camera.project(geometry.points[node]);
        rect.center() + egui::vec2(p[0], p[1])
    };
    let radius = |node: usize| {
        ((8.0 + 2.0 * (degree[node] as f32).sqrt()).min(32.0)
            * pane.display.size
            * pane.camera.zoom.sqrt().clamp(0.35, 2.5))
        .max(2.0)
    };
    let pointer = response.hover_pos();
    let hovered = pointer.and_then(|pointer| {
        view.nodes
            .iter()
            .copied()
            .filter(|&node| {
                screen(node).distance_sq(pointer) <= (radius(node).max(7.0) + 4.0).powi(2)
            })
            .min_by(|&a, &b| {
                screen(a)
                    .distance_sq(pointer)
                    .total_cmp(&screen(b).distance_sq(pointer))
            })
    });
    let graph = &ready.graph;
    let root = ready.key.root.as_deref().and_then(|path| graph.node(path));
    let current = pane
        .anchor
        .as_deref()
        .and_then(|path| graph.node(path))
        .filter(|index| view.nodes.contains(index));
    let anchor = (pane.display.colors == ColorMode::LinkDirection)
        .then_some(root.or(hovered).or(current))
        .flatten();
    let mut directions = HashMap::<usize, u8>::new();
    for edge in &view.edges {
        if pane.display.colors == ColorMode::LinkDirection {
            if anchor == Some(edge.to) {
                *directions.entry(edge.from).or_default() |= 1;
            }
            if anchor == Some(edge.from) {
                *directions.entry(edge.to).or_default() |= 2;
            }
            if anchor.is_none() {
                // A graph opened from an empty/filtered note still has meaningful
                // colors: incoming-only, outgoing-only, and both link roles.
                *directions.entry(edge.to).or_default() |= 1;
                *directions.entry(edge.from).or_default() |= 2;
            }
        }
        let start = screen(edge.from);
        let end = screen(edge.to);
        let radius = radius(edge.from).max(radius(edge.to));
        if !Rect::from_two_pos(start, end)
            .expand(radius)
            .intersects(rect)
        {
            continue;
        }
        let active = hovered == Some(edge.from) || hovered == Some(edge.to);
        let color = if active {
            theme::accent(ui)
        } else {
            ui.visuals().weak_text_color().gamma_multiply(0.42)
        };
        let stroke = Stroke::new(pane.display.thickness.max(0.25), color);
        if pane.display.arrows && (end - start).length() > radius * 2.0 {
            let direction = (end - start).normalized();
            let begin = start + direction * radius;
            let target = end - direction * (radius + 2.0);
            painter.arrow(begin, target - begin, stroke);
        } else {
            painter.line_segment([start, end], stroke);
        }
    }
    for &index in &view.nodes {
        let node = &graph.nodes[index];
        let point = screen(index);
        if !rect.expand(50.0).contains(point) {
            continue;
        }
        let size = radius(index);
        let color = if anchor == Some(index) {
            theme::accent(ui)
        } else if let Some(&relation) = directions.get(&index) {
            direction_color(ui, relation)
        } else if pane.display.colors == ColorMode::Groups
            && let Some(&group) = view.groups.get(&index)
        {
            let rgba = ready.key.options.groups[group].color;
            Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
        } else if Some(index) == root || hovered == Some(index) {
            theme::accent(ui)
        } else {
            match node.kind {
                Kind::Unresolved => ui.visuals().weak_text_color().gamma_multiply(0.45),
                Kind::Tag => theme::accent(ui).gamma_multiply(0.8),
                Kind::Attachment => Color32::from_rgb(185, 152, 91),
                Kind::Note => ui.visuals().weak_text_color(),
            }
        };
        painter.circle_filled(point, size, color);
        if ui_probe::enabled() {
            ui_probe::record(
                ui.ctx(),
                format!("node:{}", node.path.display()),
                Rect::from_center_size(point, Vec2::splat(size.max(7.0) * 2.0)).intersect(rect),
            );
        }
    }
    // Grid cells only find candidates. Exact intersections keep the same labels
    // when a pan moves their rectangles across cell boundaries.
    let mut occupied = HashMap::<(i32, i32), Vec<Rect>>::new();
    let fade = ((pane.camera.zoom * 2.0_f32.powf(-pane.display.fade) - 0.3) / 0.7).clamp(0.0, 1.0);
    for index in hovered.into_iter().chain(
        labels
            .iter()
            .copied()
            .filter(|index| Some(*index) != hovered),
    ) {
        let node = &graph.nodes[index];
        let point = screen(index);
        if !rect.expand(300.0).contains(point) || (fade == 0.0 && hovered != Some(index)) {
            continue;
        }
        let active = hovered == Some(index);
        let color = ui
            .visuals()
            .text_color()
            .gamma_multiply(if active { 1.0 } else { fade });
        let font = FontId::proportional((16.0 + (degree[index] as f32).sqrt() * 0.5).min(22.0));
        let galley = egui::WidgetText::from(RichText::new(&node.label).color(color)).into_galley(
            ui,
            Some(egui::TextWrapMode::Truncate),
            (rect.width() * 0.45).clamp(30.0, 240.0),
            font,
        );
        let position = Pos2::new(
            point.x - galley.size().x * 0.5,
            point.y + radius(index) + 5.0,
        );
        let bounds = Rect::from_min_size(position, galley.size()).expand(3.0);
        let cells = || {
            let x = (bounds.left() / 24.0).floor() as i32..=(bounds.right() / 24.0).floor() as i32;
            let y = (bounds.top() / 24.0).floor() as i32..=(bounds.bottom() / 24.0).floor() as i32;
            x.flat_map(move |x| y.clone().map(move |y| (x, y)))
        };
        if active
            || !cells().any(|cell| {
                occupied
                    .get(&cell)
                    .is_some_and(|others| others.iter().any(|other| other.intersects(bounds)))
            })
        {
            for cell in cells() {
                occupied.entry(cell).or_default().push(bounds);
            }
            painter.galley(position, galley, color);
        }
    }
    if let Some(index) = hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        response.clone().on_hover_ui(|ui| {
            ui.label(RichText::new(&graph.nodes[index].label).size(13.0));
        });
    }
    if interactive
        && response.clicked()
        && let Some(index) = hovered
    {
        let node = &graph.nodes[index];
        return Some(Action::Open {
            path: node.path.clone(),
            kind: node.kind,
        });
    }
    None
}
