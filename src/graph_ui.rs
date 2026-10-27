use crate::{theme, ui_probe};
use eframe::egui::{
    self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Stroke, Vec2,
};
use rustidian::{
    graph::{Graph, Group, Kind, Options, View},
    graph_layout::{Camera, Forces, Geometry},
    vault::Vault,
};
use serde::{Deserialize, Serialize};
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

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Display {
    arrows: bool,
    fade: f32,
    size: f32,
    thickness: f32,
    colors: ColorMode,
}

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Preferences {
    options: Options,
    forces: Forces,
    display: Display,
    camera: Camera,
    camera_root: Option<PathBuf>,
    fitted: bool,
    settings: bool,
    sections: [bool; 4],
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            options: Default::default(),
            forces: Default::default(),
            display: Default::default(),
            camera: Default::default(),
            camera_root: None,
            fitted: false,
            settings: true,
            sections: [false, false, true, false],
        }
    }
}

impl Preferences {
    fn normalize(&mut self) {
        self.options.depth = self.options.depth.clamp(1, 5);
        self.forces = self.forces.normalized();
        for (value, fallback, min, max) in [
            (&mut self.display.fade, 0.0, -3.0, 3.0),
            (&mut self.display.size, 1.0, 0.1, 5.0),
            (&mut self.display.thickness, 1.0, 0.1, 5.0),
        ] {
            *value = if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            };
        }
        if !self.camera.zoom.is_finite() || self.camera.pan.iter().any(|value| !value.is_finite()) {
            self.camera = Camera::default();
            self.fitted = false;
        }
        self.camera.zoom = self.camera.zoom.clamp(0.02, 12.0);
        for value in &mut self.camera.pan {
            *value = value.clamp(-1_000_000.0, 1_000_000.0);
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    global: Preferences,
    local: Preferences,
    local_open: bool,
}

enum DragTarget {
    Node {
        path: PathBuf,
        before: Option<[f32; 2]>,
        offset: Vec2,
    },
    Canvas {
        origin: Pos2,
        before: [f32; 2],
    },
}

struct Drag {
    target: DragTarget,
    button: egui::PointerButton,
    cancelled: bool,
}

struct Pane {
    preferences: Preferences,
    anchor: Option<PathBuf>,
    requested: Option<Key>,
    changed: Instant,
    ready: Option<Arc<Ready>>,
    fit: bool,
    positions: HashMap<PathBuf, [f32; 2]>,
    drag: Option<Drag>,
}

impl Default for Pane {
    fn default() -> Self {
        Self {
            preferences: Default::default(),
            anchor: None,
            requested: None,
            changed: Instant::now(),
            ready: None,
            fit: true,
            positions: HashMap::new(),
            drag: None,
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
    changed: bool,
}

impl Manager {
    pub fn snapshot(&self) -> State {
        State {
            global: self.global.preferences.clone(),
            local: self.local.preferences.clone(),
            local_open: self.local_open,
        }
    }

    pub fn restore(&mut self, mut state: State) {
        self.reset();
        state.global.normalize();
        state.local.normalize();
        self.global.fit = !state.global.fitted;
        self.local.fit = !state.local.fitted;
        self.global.preferences = state.global;
        self.local.preferences = state.local;
        self.local_open = state.local_open;
        self.changed = false;
    }

    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

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
                    .is_some_and(|old| old.key.root != ready.key.root)
                {
                    pane.fit = true;
                }
                pane.positions.clear();
                pane.drag = None;
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
        let previous = pane.preferences.clone();
        if local && pane.preferences.camera_root != root {
            pane.preferences.camera_root = root.clone();
            pane.fit = true;
            pane.positions.clear();
            pane.drag = None;
        }
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
                    let settings = ui.selectable_label(pane.preferences.settings, "Settings");
                    ui_probe::record(ui.ctx(), "settings", settings.rect);
                    if settings.clicked() {
                        pane.preferences.settings = !pane.preferences.settings;
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
        if pane.preferences.settings && !blocked {
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
            options: pane.preferences.options.clone(),
            forces: pane.preferences.forces,
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
        ui_probe::metric(ui.ctx(), "depth", pane.preferences.options.depth as f32);
        let painter = ui.painter().with_clip_rect(rect);
        if let Some(ready) = pane.ready.clone() {
            match &ready.result {
                Ok(scene) => {
                    let Scene { view, geometry, .. } = scene;
                    if pane.fit {
                        let reserved = if pane.preferences.settings {
                            panel_width + 20.0
                        } else {
                            0.0
                        };
                        pane.preferences.camera.fit(
                            geometry.bounds,
                            [(rect.width() - reserved).max(1.0), rect.height()],
                        );
                        pane.preferences.camera.pan[0] -= reserved * 0.5;
                        pane.preferences.fitted = true;
                        pane.positions.clear();
                        pane.drag = None;
                        pane.fit = false;
                    }
                    if !blocked && response.hovered() {
                        let wheel = ui.ctx().input(|input| input.smooth_scroll_delta().y);
                        if wheel != 0.0 {
                            let pointer =
                                response.hover_pos().unwrap_or(rect.center()) - rect.center();
                            pane.preferences
                                .camera
                                .zoom_at((wheel * 0.003).exp(), [pointer.x, pointer.y]);
                            ui.ctx()
                                .input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
                        }
                    }
                    interact(
                        ui,
                        &response,
                        &ready,
                        scene,
                        &mut pane,
                        !pending && !blocked,
                    );
                    let open = paint(
                        ui,
                        &painter,
                        &response,
                        &ready,
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
        ui_probe::metric(ui.ctx(), "zoom", pane.preferences.camera.zoom);
        ui_probe::metric(ui.ctx(), "pan-x", pane.preferences.camera.pan[0]);
        ui_probe::metric(ui.ctx(), "pan-y", pane.preferences.camera.pan[1]);
        record_preferences(ui, &pane.preferences);
        self.changed |= previous != pane.preferences;
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
        let response = theme::slider(ui, slider);
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
    ui.spacing_mut().slider_width = (ui.available_width() - 46.0).clamp(36.0, 170.0);
    ui.style_mut()
        .text_styles
        .insert(egui::TextStyle::Body, FontId::proportional(13.0));
    if section(ui, &mut pane.preferences.sections[0], "Filters") {
        let filter = ui.add(
            egui::TextEdit::singleline(&mut pane.preferences.options.filter)
                .hint_text("Search files…")
                .desired_width(f32::INFINITY),
        );
        ui_probe::record(ui.ctx(), "filter", filter.rect);
        if local {
            ui.label("Depth");
            slider(
                ui,
                egui::Slider::new(&mut pane.preferences.options.depth, 1..=5),
                "depth-slider",
            );
            toggle(
                ui,
                &mut pane.preferences.options.incoming,
                "Incoming links",
                "incoming",
            );
            toggle(
                ui,
                &mut pane.preferences.options.outgoing,
                "Outgoing links",
                "outgoing",
            );
            toggle(
                ui,
                &mut pane.preferences.options.neighbor_links,
                "Neighbor links",
                "neighbor",
            );
        }
        toggle(ui, &mut pane.preferences.options.tags, "Tags", "tags");
        toggle(
            ui,
            &mut pane.preferences.options.attachments,
            "Attachments",
            "attachments",
        );
        toggle(
            ui,
            &mut pane.preferences.options.existing_only,
            "Existing files only",
            "existing-only",
        );
        if !local {
            toggle(
                ui,
                &mut pane.preferences.options.orphans,
                "Orphans",
                "orphans",
            );
        }
        ui.add_space(4.0);
    }
    if section(ui, &mut pane.preferences.sections[1], "Groups") {
        let mut remove = None;
        let mut reorder = None;
        let group_count = pane.preferences.options.groups.len();
        let owner = ui.scope_id();
        for (index, group) in pane.preferences.options.groups.iter_mut().enumerate() {
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
            pane.preferences.options.groups.remove(index);
        } else if let Some((from, to)) = reorder {
            let group = pane.preferences.options.groups.remove(from);
            pane.preferences.options.groups.insert(to, group);
        }
        let add = ui.button("New group");
        ui_probe::record(ui.ctx(), "group-add", add.rect);
        if add.clicked() {
            pane.preferences.display.colors = ColorMode::Groups;
            pane.preferences.options.groups.push(Group {
                query: String::new(),
                color: [224, 82, 82, 255],
            });
        }
        ui.add_space(4.0);
    }
    if section(ui, &mut pane.preferences.sections[2], "Display") {
        ui.label("Color by");
        let color_mode = egui::ComboBox::from_id_salt("graph-color-mode")
            .selected_text(pane.preferences.display.colors.label())
            .show_ui(ui, |ui| {
                for mode in [ColorMode::Groups, ColorMode::LinkDirection] {
                    let response = ui.selectable_value(
                        &mut pane.preferences.display.colors,
                        mode,
                        mode.label(),
                    );
                    ui_probe::record(
                        ui.ctx(),
                        format!("color-mode:{}", mode.label()),
                        response.rect,
                    );
                }
            });
        ui_probe::record(ui.ctx(), "color-mode", color_mode.response.rect);
        if pane.preferences.display.colors == ColorMode::LinkDirection {
            for (label, relation) in [("Incoming", 1), ("Outgoing", 2), ("Both directions", 3)] {
                ui.colored_label(direction_color(ui, relation), label);
            }
            ui.small(if local {
                "Relative to the current note"
            } else {
                "Current note; hover another to inspect its connections"
            });
        }
        toggle(ui, &mut pane.preferences.display.arrows, "Arrows", "arrows");
        for (label, value, range, probe) in [
            (
                "Text fade threshold",
                &mut pane.preferences.display.fade,
                -3.0..=3.0,
                "text-fade",
            ),
            (
                "Node size",
                &mut pane.preferences.display.size,
                0.1..=5.0,
                "node-size",
            ),
            (
                "Link thickness",
                &mut pane.preferences.display.thickness,
                0.1..=5.0,
                "link-thickness",
            ),
        ] {
            ui.label(label);
            slider(ui, egui::Slider::new(value, range), probe);
        }
        ui.add_space(4.0);
    }
    if section(ui, &mut pane.preferences.sections[3], "Forces") {
        for (label, value, range, probe) in [
            (
                "Center force",
                &mut pane.preferences.forces.center,
                0.0..=1.0,
                "force-center",
            ),
            (
                "Repel force",
                &mut pane.preferences.forces.repel,
                0.0..=20.0,
                "force-repel",
            ),
            (
                "Link force",
                &mut pane.preferences.forces.link,
                0.0..=1.0,
                "force-link",
            ),
            (
                "Link distance",
                &mut pane.preferences.forces.distance,
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
        pane.preferences.options = Options::default();
        pane.preferences.forces = Forces::default();
        pane.preferences.display = Display::default();
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

fn record_preferences(ui: &egui::Ui, preferences: &Preferences) {
    if !ui_probe::enabled() {
        return;
    }
    ui_probe::metric(
        ui.ctx(),
        "settings-open",
        u8::from(preferences.settings) as f32,
    );
    for (name, open) in ["filters", "groups", "display", "forces"]
        .into_iter()
        .zip(preferences.sections)
    {
        ui_probe::metric(
            ui.ctx(),
            &format!("section-open:{name}"),
            u8::from(open) as f32,
        );
    }
    for (name, value) in [
        ("text-fade", preferences.display.fade),
        ("node-size", preferences.display.size),
        ("link-thickness", preferences.display.thickness),
        ("force-center", preferences.forces.center),
        ("force-repel", preferences.forces.repel),
        ("force-link", preferences.forces.link),
        ("force-distance", preferences.forces.distance),
        (
            "color-mode",
            u8::from(preferences.display.colors == ColorMode::LinkDirection) as f32,
        ),
    ] {
        ui_probe::metric(ui.ctx(), &format!("value:{name}"), value);
    }
    for (name, checked) in [
        ("tags", preferences.options.tags),
        ("attachments", preferences.options.attachments),
        ("existing-only", preferences.options.existing_only),
        ("orphans", preferences.options.orphans),
        ("incoming", preferences.options.incoming),
        ("outgoing", preferences.options.outgoing),
        ("neighbor", preferences.options.neighbor_links),
        ("arrows", preferences.display.arrows),
    ] {
        ui_probe::metric(
            ui.ctx(),
            &format!("checked:{name}"),
            u8::from(checked) as f32,
        );
    }
}

fn screen_position(
    pane: &Pane,
    graph: &Graph,
    geometry: &Geometry,
    node: usize,
    rect: Rect,
) -> Pos2 {
    let world = pane
        .positions
        .get(&graph.nodes[node].path)
        .copied()
        .unwrap_or(geometry.points[node]);
    let point = pane.preferences.camera.project(world);
    rect.center() + egui::vec2(point[0], point[1])
}

fn node_radius(pane: &Pane, scene: &Scene, node: usize) -> f32 {
    (((6.0 + 0.5 * (scene.degree[node] as f32).sqrt()).min(9.0)
        * pane.preferences.camera.zoom.sqrt().clamp(0.35, 1.2))
    .min(10.0)
        * pane.preferences.display.size)
        .max(2.0)
}

fn node_at(pane: &Pane, ready: &Ready, scene: &Scene, rect: Rect, pointer: Pos2) -> Option<usize> {
    scene
        .view
        .nodes
        .iter()
        .copied()
        .filter(|&node| {
            screen_position(pane, &ready.graph, &scene.geometry, node, rect).distance_sq(pointer)
                <= (node_radius(pane, scene, node).max(7.0) + 4.0).powi(2)
        })
        .min_by(|&a, &b| {
            screen_position(pane, &ready.graph, &scene.geometry, a, rect)
                .distance_sq(pointer)
                .total_cmp(
                    &screen_position(pane, &ready.graph, &scene.geometry, b, rect)
                        .distance_sq(pointer),
                )
        })
}

fn interact(
    ui: &egui::Ui,
    response: &egui::Response,
    ready: &Ready,
    scene: &Scene,
    pane: &mut Pane,
    interactive: bool,
) {
    if !interactive {
        pane.drag = None;
        return;
    }
    let start_button = [egui::PointerButton::Middle, egui::PointerButton::Primary]
        .into_iter()
        .find(|&button| response.drag_started_by(button));
    if pane.drag.is_none()
        && let Some(button) = start_button
        && let Some(origin) = ui.input(|input| input.pointer.press_origin())
    {
        let node = (button == egui::PointerButton::Primary)
            .then(|| node_at(pane, ready, scene, response.rect, origin))
            .flatten();
        let target = if let Some(node) = node {
            let path = ready.graph.nodes[node].path.clone();
            DragTarget::Node {
                before: pane.positions.get(&path).copied(),
                path,
                offset: screen_position(pane, &ready.graph, &scene.geometry, node, response.rect)
                    - origin,
            }
        } else {
            DragTarget::Canvas {
                origin,
                before: pane.preferences.camera.pan,
            }
        };
        pane.drag = Some(Drag {
            target,
            button,
            cancelled: false,
        });
    }
    let Some(drag) = &mut pane.drag else {
        return;
    };
    if !drag.cancelled
        && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
    {
        match &drag.target {
            DragTarget::Node { path, before, .. } => {
                if let Some(before) = before {
                    pane.positions.insert(path.clone(), *before);
                } else {
                    pane.positions.remove(path);
                }
            }
            DragTarget::Canvas { before, .. } => pane.preferences.camera.pan = *before,
        }
        drag.cancelled = true;
    }
    let (down, pointer) = ui.input(|input| {
        let down = input.pointer.button_down(drag.button);
        // A release and a later hover can share one frame. Commit the release
        // position, not that later hover, and include the drag threshold distance.
        let pointer = if down {
            input.pointer.interact_pos()
        } else {
            input.events.iter().rev().find_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: false,
                    ..
                } if *button == drag.button => Some(*pos),
                _ => None,
            })
        };
        (down, pointer)
    });
    if !drag.cancelled
        && let Some(pointer) = pointer
    {
        match &drag.target {
            DragTarget::Node { path, offset, .. } => {
                let screen = pointer + *offset - response.rect.center();
                let camera = pane.preferences.camera;
                pane.positions.insert(
                    path.clone(),
                    [
                        (screen.x - camera.pan[0]) / camera.zoom,
                        (screen.y - camera.pan[1]) / camera.zoom,
                    ],
                );
            }
            DragTarget::Canvas { origin, before } => {
                let delta = pointer - *origin;
                pane.preferences.camera.pan = [before[0] + delta.x, before[1] + delta.y];
            }
        }
    }
    if !down {
        pane.drag = None;
    }
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
        labels,
        ..
    } = scene;
    let screen = |node| screen_position(pane, &ready.graph, geometry, node, rect);
    let radius = |node| node_radius(pane, scene, node);
    let pointer = response.hover_pos();
    let hovered = pointer.and_then(|pointer| node_at(pane, ready, scene, rect, pointer));
    let graph = &ready.graph;
    let root = ready.key.root.as_deref().and_then(|path| graph.node(path));
    let current = pane
        .anchor
        .as_deref()
        .and_then(|path| graph.node(path))
        .filter(|index| view.nodes.contains(index));
    let anchor = (pane.preferences.display.colors == ColorMode::LinkDirection)
        .then_some(root.or(hovered).or(current))
        .flatten();
    let mut directions = HashMap::<usize, u8>::new();
    for edge in &view.edges {
        if pane.preferences.display.colors == ColorMode::LinkDirection {
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
        let stroke = Stroke::new(pane.preferences.display.thickness.max(0.25), color);
        if pane.preferences.display.arrows && (end - start).length() > radius * 2.0 {
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
        } else if pane.preferences.display.colors == ColorMode::Groups
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
    let fade =
        ((pane.preferences.camera.zoom * 2.0_f32.powf(-pane.preferences.display.fade) - 0.3) / 0.7)
            .clamp(0.0, 1.0);
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
        let font = FontId::proportional(16.0);
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
    if pane
        .drag
        .as_ref()
        .is_some_and(|drag| drag.button == egui::PointerButton::Middle && !drag.cancelled)
    {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
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
