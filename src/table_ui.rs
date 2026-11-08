use eframe::egui::{
    self, Key, TextEdit,
    text::{CCursor, CCursorRange},
    widgets::text_edit::TextEditState,
};
use rustidian::{
    tab::Tab,
    tables::{Action, CellDraft, Table},
};
use std::ops::Range;

type CellKey = (usize, usize, usize);
const EDGE_SIZE: f32 = 14.0;

struct Cell {
    text: String,
    presentation: crate::preview::TableCell,
}

struct TableView {
    model: Table,
    cells: Vec<Vec<Cell>>,
}

struct Draft {
    key: CellKey,
    cell: CellDraft,
}

struct LinkCell {
    key: CellKey,
    target: String,
    rect: egui::Rect,
    layer: egui::LayerId,
}

struct LinkPress {
    key: CellKey,
    origin: egui::Pos2,
    moved: bool,
    double: bool,
}

struct PendingLink {
    key: CellKey,
    target: String,
    position: egui::Pos2,
    deadline: f64,
}

/// A rectangular cell range, chosen by dragging, Shift+click or a row/column handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Selection {
    table: usize,
    anchor: (usize, usize),
    head: (usize, usize),
}

impl Selection {
    fn ranges(&self, rows: usize, columns: usize) -> (Range<usize>, Range<usize>) {
        let span =
            |a: usize, b: usize, limit: usize| a.min(b).min(limit)..(a.max(b) + 1).min(limit);
        (
            span(self.anchor.0, self.head.0, rows),
            span(self.anchor.1, self.head.1, columns),
        )
    }
}

fn cell_under(
    outer: egui::Rect,
    widths: &[f32],
    heights: &[f32],
    pos: egui::Pos2,
) -> (usize, usize) {
    let locate = |sizes: &[f32], offset: f32| {
        let mut edge = 0.0;
        sizes
            .iter()
            .position(|size| {
                edge += size;
                offset < edge
            })
            .unwrap_or(sizes.len().saturating_sub(1))
    };
    (
        locate(heights, pos.y - outer.top()),
        locate(widths, pos.x - outer.left()),
    )
}

/// A small handle on a row's left edge or a column's top edge; clicking it selects the line.
fn grip(
    ui: &mut egui::Ui,
    id: egui::Id,
    rect: egui::Rect,
    visible: bool,
    selected: bool,
    probe: String,
) -> Option<egui::Response> {
    let rect = rect.intersect(ui.clip_rect());
    if !rect.is_positive() {
        return None;
    }
    let response = ui
        .interact(rect, id, egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if visible || selected || response.hovered() {
        let color = if selected || response.hovered() {
            crate::theme::accent(ui)
        } else {
            ui.visuals().weak_text_color()
        };
        ui.painter().rect_filled(rect.shrink(1.0), 3, color);
        crate::ui_probe::record(ui.ctx(), probe, rect);
    }
    Some(response)
}

#[derive(Default)]
pub struct Tables {
    owner: Option<egui::Id>,
    tab: Option<u64>,
    views: Vec<TableView>,
    focus: Option<CellKey>,
    error: Option<String>,
    draft: Option<Draft>,
    deferred: Vec<egui::Event>,
    raw_deferred: Vec<egui::Event>,
    exit: Option<usize>,
    selection: Option<Selection>,
    drag: Option<CellKey>,
    link_press: Option<LinkPress>,
    pending_link: Option<PendingLink>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableAction {
    Previous,
    Next,
    Down,
    Follow,
}

fn take_action(
    events: &mut Vec<egui::Event>,
    deferred: &mut Vec<egui::Event>,
) -> Option<TableAction> {
    let (index, key, modifiers) = events.iter().enumerate().find_map(|(index, event)| {
        if let egui::Event::Key {
            key: key @ (Key::Tab | Key::Enter),
            pressed: true,
            modifiers,
            ..
        } = event
        {
            Some((index, *key, *modifiers))
        } else {
            None
        }
    })?;
    let action = if modifiers.matches_exact(egui::Modifiers::NONE) {
        if key == Key::Tab {
            TableAction::Next
        } else {
            TableAction::Down
        }
    } else if key == Key::Tab && modifiers.matches_exact(egui::Modifiers::SHIFT) {
        TableAction::Previous
    } else if key == Key::Enter && modifiers.matches_exact(egui::Modifiers::COMMAND) {
        TableAction::Follow
    } else {
        return None;
    };
    if action == TableAction::Follow {
        events.remove(index);
    } else {
        events.remove(index);
        defer_from(events, deferred, index);
    }
    Some(action)
}

pub(crate) fn defer_from(
    events: &mut Vec<egui::Event>,
    deferred: &mut Vec<egui::Event>,
    index: usize,
) {
    for event in events.split_off(index) {
        if matches!(
            event,
            egui::Event::Key { .. }
                | egui::Event::Text(_)
                | egui::Event::ModifiersChanged(_)
                | egui::Event::Paste(_)
                | egui::Event::Copy
                | egui::Event::Cut
                | egui::Event::Ime(_)
        ) {
            deferred.push(event);
        } else {
            events.push(event);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub ordinal: usize,
    pub bytes: Range<usize>,
    pub characters: Range<usize>,
    pub widths: Vec<f32>,
    pub heights: Vec<f32>,
    pub available: f32,
}

impl Placement {
    pub fn height(&self) -> f32 {
        self.heights.iter().sum::<f32>() + 1.0 + EDGE_SIZE
    }
}

fn edge_button(
    ui: &mut egui::Ui,
    id: egui::Id,
    table: usize,
    rect: egui::Rect,
    column: bool,
) -> bool {
    let rect = rect.intersect(ui.clip_rect());
    if !rect.is_positive() {
        return false;
    }
    // Leave the existing last-column resize target independent of insertion.
    let mut hit = rect;
    if column {
        hit.min.x += 4.0;
    }
    if !hit.is_positive() {
        return false;
    }
    let response = ui
        .interact(hit, id, egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(if column { "Add column" } else { "Add row" });
    if response.hovered() || response.has_focus() || response.is_pointer_button_down_on() {
        ui.painter().rect(
            rect,
            0,
            ui.visuals().window_fill,
            ui.visuals().widgets.noninteractive.bg_stroke,
            egui::StrokeKind::Inside,
        );
        crate::theme::icon_image(crate::theme::Icon::NewTab, ui.visuals().weak_text_color())
            .fit_to_exact_size(egui::Vec2::splat(10.0))
            .paint_at(
                ui,
                egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(10.0)),
            );
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect,
                0,
                egui::Stroke::new(1.0, crate::theme::accent(ui)),
                egui::StrokeKind::Inside,
            );
        }
        crate::ui_probe::record(
            ui.ctx(),
            format!(
                "edit-table-add-{}:{table}",
                if column { "column" } else { "row" }
            ),
            rect,
        );
    }
    response.clicked()
        || response.has_focus()
            && ui.input_mut(|input| {
                input.consume_key(egui::Modifiers::NONE, Key::Enter)
                    || input.consume_key(egui::Modifiers::NONE, Key::Space)
            })
}

fn cell_id(id: egui::Id, key: CellKey) -> egui::Id {
    id.with(("table-cell", key))
}

impl Tables {
    pub fn raw_input(
        &mut self,
        ctx: &egui::Context,
        raw: &mut egui::RawInput,
        live_tab: Option<u64>,
    ) {
        if raw.viewport_id != egui::ViewportId::ROOT || raw.viewport().visible() == Some(false) {
            return;
        }
        if live_tab.is_none() || live_tab != self.tab {
            self.raw_deferred.clear();
            return;
        }
        let Some(owner) = self.owner else {
            return;
        };
        self.raw_deferred.append(&mut raw.events);
        raw.events = std::mem::take(&mut self.raw_deferred);
        let edges: Vec<_> = (0..self.views.len())
            .flat_map(|table| [false, true].map(|column| owner.with(("table-add", table, column))))
            .filter_map(|id| ctx.read_response(id))
            .filter(|response| {
                response.enabled()
                    && ctx.memory(|memory| memory.allows_interaction(response.layer_id))
            })
            .collect();
        let mut mutation = false;
        let mut split = None;
        for (index, event) in raw.events.iter().enumerate() {
            match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } if edges.iter().any(|response| {
                    let local = ctx
                        .layer_transform_from_global(response.layer_id)
                        .map_or(*pos, |transform| transform * *pos);
                    response.interact_rect.contains(local)
                        && ctx.layer_id_at(*pos) == Some(response.layer_id)
                }) =>
                {
                    // Text before a click belongs to the old cell. Text after its
                    // release belongs to the new cell, which exists next frame.
                    if *pressed && mutation {
                        split = Some(index);
                        break;
                    }
                    if !pressed {
                        split = Some(index + 1);
                        break;
                    }
                }
                egui::Event::Key {
                    key: key @ (Key::Enter | Key::Space),
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.matches_exact(egui::Modifiers::NONE)
                    && edges.iter().any(|response| response.has_focus()) =>
                {
                    // Winit pairs a printable Space key with Text(" "). Keep
                    // that activation text with the button, not the new cell.
                    let end = index + 1;
                    let space_text = *key == Key::Space
                        && matches!(raw.events.get(end), Some(egui::Event::Text(text)) if text == " ");
                    split = Some(end + usize::from(space_text));
                    break;
                }
                egui::Event::Text(_)
                | egui::Event::Paste(_)
                | egui::Event::Cut
                | egui::Event::Ime(egui::ImeEvent::Commit(_))
                | egui::Event::Key {
                    key: Key::Backspace | Key::Delete | Key::Enter | Key::Tab,
                    pressed: true,
                    ..
                } => mutation = true,
                _ => {}
            }
        }
        if let Some(index) = split.filter(|index| *index < raw.events.len()) {
            self.raw_deferred = raw.events.split_off(index);
            ctx.request_repaint();
        }
    }

    pub fn prepare(
        &mut self,
        ui: &egui::Ui,
        document: (&std::path::Path, &str),
        models: &[Table],
        active_line: Option<&Range<usize>>,
        caret: Option<usize>,
        hidden: &[Range<usize>],
    ) -> Vec<Placement> {
        let (from, source) = document;
        if self.views.len() != models.len()
            || self.views.iter().zip(models).any(|(view, model)| {
                view.model.range != model.range || !view.model.is_current(source)
            })
        {
            self.views = models
                .iter()
                .enumerate()
                .map(|(table, model)| TableView {
                    model: model.clone(),
                    cells: model
                        .rows
                        .iter()
                        .enumerate()
                        .map(|(row, _)| {
                            (0..model.alignments.len())
                                .map(|column| Cell {
                                    text: self
                                        .draft
                                        .as_ref()
                                        .filter(|draft| {
                                            draft.key == (table, row, column)
                                                && draft.cell.is_current(source)
                                        })
                                        .map_or_else(
                                            || model.cell_text(source, row, column),
                                            |draft| draft.cell.text.clone(),
                                        ),
                                    presentation: crate::preview::TableCell::new(
                                        &model.cell_text(source, row, column),
                                    ),
                                })
                                .collect()
                        })
                        .collect(),
                })
                .collect();
        }
        // Tables come in text order: one forward walk gives every start in characters.
        let mut counted = (0, 0);
        let starts: Vec<usize> = self
            .views
            .iter()
            .map(|view| {
                let byte = view.model.range.start;
                if byte < counted.0 {
                    counted = (0, 0);
                }
                counted = (byte, counted.1 + source[counted.0..byte].chars().count());
                counted.1
            })
            .collect();
        self.views
            .iter()
            .enumerate()
            .filter_map(|(ordinal, view)| {
                if active_line.is_some_and(|line| {
                    line.start < view.model.range.end && line.end >= view.model.range.start
                }) {
                    // A caret entering the table edits its cell in place, as in Obsidian;
                    // only a selection that reaches into the table shows the Markdown source.
                    match caret.filter(|byte| view.model.range.contains(byte)) {
                        Some(byte) => {
                            let (row, column) = view.model.cell_at(byte);
                            self.focus.get_or_insert((ordinal, row, column));
                        }
                        None => return None,
                    }
                }
                let start = starts[ordinal];
                if hidden.iter().any(|range| range.contains(&start)) {
                    return None;
                }
                let mut widths = vec![80.0_f32; view.model.alignments.len()];
                for (row, cells) in view.cells.iter().enumerate() {
                    for (column, cell) in cells.iter().enumerate() {
                        widths[column] = widths[column].max(
                            (cell
                                .presentation
                                .layout(ui, row == 0, f32::INFINITY)
                                .size()
                                .x
                                + 17.0)
                                .min(320.0),
                        );
                    }
                }
                let widths = crate::table_layout::widths(
                    ui.ctx(),
                    from,
                    ordinal,
                    &widths,
                    (ui.available_width() - EDGE_SIZE).max(1.0),
                );
                let heights = view
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(row, cells)| {
                        cells
                            .iter()
                            .enumerate()
                            .map(|(column, cell)| {
                                cell.presentation
                                    .layout(ui, row == 0, widths[column] - 17.0)
                                    .size()
                                    .y
                                    + 9.0
                            })
                            .fold(32.4_f32, f32::max)
                    })
                    .collect();
                Some(Placement {
                    ordinal,
                    bytes: view.model.range.clone(),
                    characters: start..start + source[view.model.range.clone()].chars().count(),
                    widths,
                    heights,
                    available: (ui.available_width() - EDGE_SIZE).max(1.0),
                })
            })
            .collect()
    }

    pub fn begin(&mut self, ui: &egui::Ui, tab: &mut Tab, id: egui::Id, live: bool) {
        if self.owner != Some(id) {
            *self = Self {
                owner: Some(id),
                ..Default::default()
            };
        }
        if self.tab != Some(tab.id()) {
            self.link_press = None;
            self.pending_link = None;
        }
        self.tab = Some(tab.id());
        if !live {
            self.focus = None;
            self.selection = None;
            self.deferred.clear();
            self.draft = None;
            self.link_press = None;
            self.pending_link = None;
            return;
        }
        if let Some(key) = self.focus {
            ui.memory_mut(|memory| memory.request_focus(cell_id(id, key)));
        }
        if !self.deferred.is_empty() {
            ui.input_mut(|input| {
                self.deferred.append(&mut input.events);
                input.events = std::mem::take(&mut self.deferred);
            });
        }
        let focused_cell = self.focus.or_else(|| self.focused_cell(ui.ctx(), id));
        let focused = focused_cell.is_some();
        if !focused
            || !ui.is_enabled()
            || !ui.memory(|memory| memory.allows_interaction(ui.layer_id()))
        {
            return;
        }
        feed_history(ui.ctx(), id, &tab.document.text);
        let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
        while crate::table_layout::history(
            ui.ctx(),
            &tab.document.path,
            &tab.document.text,
            &mut state,
            true,
        ) {}
        state.clone().store(ui.ctx(), id);
        let request = ui.input_mut(|input| {
            let mut mutation = false;
            for (index, event) in input.events.iter().enumerate() {
                match event {
                    egui::Event::Text(_)
                    | egui::Event::Paste(_)
                    | egui::Event::Cut
                    | egui::Event::Ime(egui::ImeEvent::Commit(_)) => mutation = true,
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => {
                        let redo = *key == Key::Y
                            && modifiers.matches_logically(egui::Modifiers::COMMAND)
                            || *key == Key::Z
                                && modifiers.matches_logically(
                                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                                );
                        let undo =
                            *key == Key::Z && modifiers.matches_logically(egui::Modifiers::COMMAND);
                        if undo || redo {
                            if mutation {
                                defer_from(&mut input.events, &mut self.deferred, index);
                                return None;
                            }
                            input.events.remove(index);
                            defer_from(&mut input.events, &mut self.deferred, index);
                            return Some(redo);
                        }
                        if matches!(key, Key::Backspace | Key::Delete | Key::Enter | Key::Tab) {
                            mutation = true;
                        }
                    }
                    _ => {}
                }
            }
            None
        });
        if !self.deferred.is_empty() {
            ui.ctx().request_repaint();
        }
        let Some(redo) = request else {
            return;
        };
        let cursor = state.cursor.char_range().unwrap_or_default();
        let mut history = state.undoer();
        let mut current = (cursor, tab.document.text.clone());
        let restored = loop {
            let next = if redo {
                history.redo(&current)
            } else {
                history.undo(&current)
            }
            .cloned();
            match next {
                Some(next) if next.1 == tab.document.text => current = next,
                next => break next,
            }
        };
        if let Some((cursor, text)) = restored {
            self.draft = None;
            tab.document.text = text;
            state.cursor.set_char_range(Some(cursor));
            if let Some((table, row, column)) = focused_cell {
                let models = rustidian::editor::Markdown::parse(&tab.document.text).tables;
                if let Some(model) = models.get(table) {
                    if row >= model.rows.len() || column >= model.alignments.len() {
                        let destination = (
                            table,
                            row.min(model.rows.len() - 1),
                            column.min(model.alignments.len() - 1),
                        );
                        self.focus = Some(destination);
                        ui.memory_mut(|memory| memory.request_focus(cell_id(id, destination)));
                    }
                } else {
                    self.focus = None;
                    ui.memory_mut(|memory| memory.request_focus(id));
                }
            }
            tab.edited();
            ui.ctx().request_repaint();
        }
        state.set_undoer(history);
        state.store(ui.ctx(), id);
    }

    pub fn has_focus(&self, ctx: &egui::Context, id: egui::Id) -> bool {
        self.focused_cell(ctx, id).is_some()
    }

    fn focused_cell(&self, ctx: &egui::Context, id: egui::Id) -> Option<CellKey> {
        self.views.iter().enumerate().find_map(|(table, view)| {
            view.cells.iter().enumerate().find_map(|(row, cells)| {
                (0..cells.len()).find_map(|column| {
                    let key = (table, row, column);
                    ctx.memory(|memory| memory.has_focus(cell_id(id, key)))
                        .then_some(key)
                })
            })
        })
    }

    pub fn take_exit(&mut self) -> Option<usize> {
        self.exit.take()
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut Tab,
        id: egui::Id,
        placements: &[Placement],
        origins: &[egui::Pos2],
    ) -> Option<String> {
        let mut follow = None;
        let events = ui.input(|input| input.raw.events.clone());
        let link_positions: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    ..
                } => Some(*pos),
                _ => None,
            })
            .collect();
        let mut link_cells = Vec::new();
        let mut edit = None;
        let mut insertion = None;
        let mut separate_edit = false;
        let mut navigation = None;
        let mut menu_focus = None;
        let mut exit = None;
        let pending_focus = self.focus.take();
        let previous = self.focused_cell(ui.ctx(), id).or(pending_focus);
        if previous.is_some() {
            self.selection_keys(ui, tab);
        }
        let mut clear_selection = false;
        let mut table_areas = Vec::new();
        for (placement, origin) in placements.iter().zip(origins) {
            let table = placement.ordinal;
            let view = &mut self.views[table];
            let rect = egui::Rect::from_min_size(
                *origin,
                egui::vec2(
                    placement.widths.iter().sum::<f32>() + 1.0,
                    placement.height() - EDGE_SIZE,
                ),
            );
            let available = egui::Rect::from_min_size(
                *origin,
                egui::vec2(ui.available_width(), placement.height() + 2.0),
            );
            ui.expand_to_include_rect(available);
            ui.scope_builder(egui::UiBuilder::new().max_rect(available), |ui| {
                egui::ScrollArea::horizontal()
                    .id_salt(id.with(("edit-table-scroll", table)))
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let (allocation, _) = ui.allocate_exact_size(
                            rect.size() + egui::Vec2::splat(EDGE_SIZE),
                            egui::Sense::hover(),
                        );
                        let outer = egui::Rect::from_min_size(allocation.min, rect.size());
                        crate::ui_probe::record(
                            ui.ctx(),
                            format!("edit-table:{table}"),
                            outer.intersect(ui.clip_rect()),
                        );
                        let border = ui.visuals().widgets.noninteractive.bg_stroke;
                        ui.painter().rect_filled(outer, 0, ui.visuals().window_fill);
                        let row_count = view.cells.len();
                        let mut y = outer.top() + 0.5;
                        for (row, cells) in view.cells.iter_mut().enumerate() {
                            let mut x = outer.left() + 0.5;
                            for (column, cell) in cells.iter_mut().enumerate() {
                                let key = (table, row, column);
                                let field_id = cell_id(id, key);
                                let cell_rect = egui::Rect::from_min_size(
                                    egui::pos2(x, y),
                                    egui::vec2(placement.widths[column], placement.heights[row]),
                                );
                                x += placement.widths[column];
                                if !cell_rect.intersects(ui.clip_rect())
                                    && pending_focus != Some(key)
                                {
                                    continue;
                                }
                                ui.painter().rect_stroke(
                                    cell_rect,
                                    0,
                                    border,
                                    egui::StrokeKind::Middle,
                                );
                                let inner = cell_rect.shrink2(egui::vec2(8.0, 4.0));
                                if pending_focus == Some(key) {
                                    ui.memory_mut(|m| m.request_focus(field_id));
                                    let mut state =
                                        TextEditState::load(ui.ctx(), field_id).unwrap_or_default();
                                    state.cursor.set_char_range(Some(CCursorRange::two(
                                        CCursor::new(0),
                                        CCursor::new(cell.text.chars().count()),
                                    )));
                                    state.store(ui.ctx(), field_id);
                                    ui.scroll_to_rect(cell_rect, None);
                                }
                                let focused = ui.memory(|m| m.has_focus(field_id));
                                if focused
                                    && ui.input_mut(|input| {
                                        input.consume_key(egui::Modifiers::NONE, Key::Escape)
                                    })
                                {
                                    exit = Some(placement.characters.end);
                                }
                                let action = if focused {
                                    ui.input_mut(|input| {
                                        take_action(&mut input.events, &mut self.deferred)
                                    })
                                } else {
                                    None
                                };
                                let nav = match action {
                                    Some(TableAction::Previous) => Some(-1),
                                    Some(TableAction::Next) => Some(1),
                                    Some(TableAction::Down) => Some(2),
                                    _ => None,
                                };
                                let discrete = focused
                                    && ui.input(|input| {
                                        input.events.iter().any(|event| {
                                            matches!(
                                                event,
                                                egui::Event::Paste(_) | egui::Event::Cut
                                            )
                                        })
                                    });
                                let replacing = focused
                                    && TextEditState::load(ui.ctx(), field_id)
                                        .and_then(|state| state.cursor.char_range())
                                        .is_some_and(|cursor| !cursor.is_empty())
                                    && ui.input(|input| {
                                        input
                                            .events
                                            .iter()
                                            .any(|event| matches!(event, egui::Event::Text(_)))
                                    });
                                if discrete || replacing {
                                    checkpoint(ui.ctx(), id, &tab.document.text);
                                }
                                let rendered =
                                    cell.presentation.layout(ui, row == 0, inner.width());
                                let mut layouter =
                                    |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _width: f32| {
                                        if focused {
                                            ui.fonts_mut(|fonts| {
                                                fonts.layout_no_wrap(
                                                    buffer.as_str().into(),
                                                    egui::FontId::new(
                                                        crate::theme::document_size(ui, 18.0),
                                                        crate::theme::fonts::text("regular"),
                                                    ),
                                                    ui.visuals().text_color(),
                                                )
                                            })
                                        } else {
                                            rendered.clone()
                                        }
                                    };
                                let alignment = match view.model.alignments[column] {
                                    pulldown_cmark::Alignment::Center => egui::Align2::CENTER_TOP,
                                    pulldown_cmark::Alignment::Right => egui::Align2::RIGHT_TOP,
                                    _ => egui::Align2::LEFT_TOP,
                                };
                                let response = ui.put(
                                    inner,
                                    TextEdit::singleline(&mut cell.text)
                                        .id(field_id)
                                        .auto_pair(true)
                                        .font(egui::FontId::new(
                                            crate::theme::document_size(ui, 18.0),
                                            crate::theme::fonts::text("regular"),
                                        ))
                                        .frame(egui::Frame::NONE)
                                        .margin(0)
                                        .desired_width(inner.width())
                                        .align(alignment)
                                        .event_filter(egui::EventFilter {
                                            horizontal_arrows: true,
                                            vertical_arrows: true,
                                            tab: true,
                                            escape: true,
                                        })
                                        .layouter(&mut layouter),
                                );
                                if !focused {
                                    let mut math_ui =
                                        ui.new_child(egui::UiBuilder::new().max_rect(inner));
                                    math_ui.set_clip_rect(inner.intersect(ui.clip_rect()));
                                    cell.presentation.paint(
                                        &math_ui,
                                        &rendered,
                                        alignment
                                            .align_size_within_rect(rendered.size(), inner)
                                            .min,
                                    );
                                }
                                crate::ui_probe::record(
                                    ui.ctx(),
                                    format!("edit-table-cell:{table}:{row}:{column}"),
                                    cell_rect.intersect(ui.clip_rect()),
                                );
                                if focused {
                                    ui.painter().rect_stroke(
                                        cell_rect,
                                        0,
                                        egui::Stroke::new(1.0, crate::theme::accent(ui)),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                if response.changed() {
                                    separate_edit = discrete;
                                    edit = Some((
                                        table,
                                        Action::Cell {
                                            row,
                                            column,
                                            text: cell.text.clone(),
                                        },
                                    ));
                                }
                                if action == Some(TableAction::Follow)
                                    || response.clicked()
                                        && ui.input(|input| input.modifiers.command)
                                {
                                    let caret = TextEditState::load(ui.ctx(), field_id)
                                        .and_then(|state| state.cursor.char_range())
                                        .map_or(0, |range| range.primary.index.0);
                                    follow = rustidian::editor::link_at(&cell.text, caret);
                                }
                                if !link_positions.is_empty()
                                    && response.enabled()
                                    && ui.memory(|memory| {
                                        memory.allows_interaction(response.layer_id)
                                    })
                                    && link_positions.iter().any(|pos| {
                                        let local = ui
                                            .ctx()
                                            .layer_transform_from_global(response.layer_id)
                                            .map_or(*pos, |transform| transform * *pos);
                                        response.interact_rect.contains(local)
                                            && ui.ctx().layer_id_at(*pos) == Some(response.layer_id)
                                    })
                                    && let Some(target) = rustidian::editor::sole_link(&cell.text)
                                {
                                    link_cells.push(LinkCell {
                                        key,
                                        target,
                                        rect: response.interact_rect,
                                        layer: response.layer_id,
                                    });
                                }
                                if let Some(direction) = nav {
                                    navigation = Some((key, direction));
                                }
                                let (shift, pressed) = ui.input(|input| {
                                    (input.modifiers.shift, input.pointer.primary_pressed())
                                });
                                if pressed && response.contains_pointer() {
                                    let anchor = self
                                        .selection
                                        .filter(|selection| selection.table == table)
                                        .map(|selection| selection.anchor)
                                        .or(previous
                                            .filter(|cell| cell.0 == table)
                                            .map(|cell| (cell.1, cell.2)));
                                    match anchor.filter(|_| shift) {
                                        Some(anchor) => {
                                            self.selection = Some(Selection {
                                                table,
                                                anchor,
                                                head: (row, column),
                                            })
                                        }
                                        None => {
                                            self.selection = None;
                                            self.drag = Some(key);
                                        }
                                    }
                                }
                                let selected = self.selection.filter(|selection| {
                                    let (rows, columns) =
                                        selection.ranges(row_count, view.model.alignments.len());
                                    selection.table == table
                                        && rows.contains(&row)
                                        && columns.contains(&column)
                                });
                                response.context_menu(|ui| {
                                    crate::theme::menu_style(ui);
                                    let columns = view.model.alignments.len();
                                    let count = view.model.rows.len();
                                    if let Some(selection) = selected {
                                        let (rows, chosen) = selection.ranges(count, columns);
                                        if ui.button("Copy selection").clicked() {
                                            ui.ctx().copy_text(view.model.copy(
                                                &tab.document.text,
                                                rows.clone(),
                                                chosen.clone(),
                                            ));
                                            ui.close();
                                        }
                                        if ui.button("Clear selection").clicked() {
                                            edit = Some((
                                                table,
                                                Action::Clear {
                                                    rows,
                                                    columns: chosen,
                                                },
                                            ));
                                            menu_focus = Some(key);
                                            ui.close();
                                        }
                                        ui.separator();
                                    }
                                    let mut action =
                                        |ui: &mut egui::Ui,
                                         label: &str,
                                         enabled: bool,
                                         value: Action| {
                                            let response =
                                                ui.add_enabled(enabled, egui::Button::new(label));
                                            crate::ui_probe::record(
                                                ui.ctx(),
                                                format!("table-action:{label}"),
                                                response.rect,
                                            );
                                            if response.clicked() {
                                                edit = Some((table, value));
                                                menu_focus = Some(key);
                                                ui.close();
                                            }
                                        };
                                    action(ui, "Insert row above", row > 0, Action::InsertRow(row));
                                    action(
                                        ui,
                                        "Insert row below",
                                        true,
                                        Action::InsertRow(row + 1),
                                    );
                                    action(ui, "Delete row", row > 0, Action::RemoveRow(row));
                                    action(
                                        ui,
                                        "Move row up",
                                        row > 1,
                                        Action::MoveRow {
                                            from: row,
                                            to: row.saturating_sub(1),
                                        },
                                    );
                                    action(
                                        ui,
                                        "Move row down",
                                        row > 0 && row + 1 < count,
                                        Action::MoveRow {
                                            from: row,
                                            to: row + 1,
                                        },
                                    );
                                    ui.separator();
                                    action(
                                        ui,
                                        "Insert column left",
                                        true,
                                        Action::InsertColumn(column),
                                    );
                                    action(
                                        ui,
                                        "Insert column right",
                                        true,
                                        Action::InsertColumn(column + 1),
                                    );
                                    action(
                                        ui,
                                        "Delete column",
                                        columns > 1,
                                        Action::RemoveColumn(column),
                                    );
                                    action(
                                        ui,
                                        "Move column left",
                                        column > 0,
                                        Action::MoveColumn {
                                            from: column,
                                            to: column.saturating_sub(1),
                                        },
                                    );
                                    action(
                                        ui,
                                        "Move column right",
                                        column + 1 < columns,
                                        Action::MoveColumn {
                                            from: column,
                                            to: column + 1,
                                        },
                                    );
                                    ui.separator();
                                    action(
                                        ui,
                                        "Align left",
                                        true,
                                        Action::Align {
                                            column,
                                            alignment: pulldown_cmark::Alignment::Left,
                                        },
                                    );
                                    action(
                                        ui,
                                        "Align center",
                                        true,
                                        Action::Align {
                                            column,
                                            alignment: pulldown_cmark::Alignment::Center,
                                        },
                                    );
                                    action(
                                        ui,
                                        "Align right",
                                        true,
                                        Action::Align {
                                            column,
                                            alignment: pulldown_cmark::Alignment::Right,
                                        },
                                    );
                                    action(
                                        ui,
                                        "Sort ascending",
                                        true,
                                        Action::Sort {
                                            column,
                                            descending: false,
                                        },
                                    );
                                    action(
                                        ui,
                                        "Sort descending",
                                        true,
                                        Action::Sort {
                                            column,
                                            descending: true,
                                        },
                                    );
                                });
                            }
                            y += placement.heights[row];
                        }
                        let (rows, columns) = (view.cells.len(), view.model.alignments.len());
                        table_areas.push(outer.expand(8.0).intersect(ui.clip_rect()));
                        if let Some(origin) = self.drag.filter(|cell| cell.0 == table) {
                            let (down, pointer) = ui.input(|input| {
                                (input.pointer.primary_down(), input.pointer.interact_pos())
                            });
                            if !down {
                                self.drag = None;
                            } else if let Some(pointer) = pointer {
                                let head = cell_under(
                                    outer,
                                    &placement.widths,
                                    &placement.heights,
                                    pointer,
                                );
                                if head != (origin.1, origin.2) || self.selection.is_some() {
                                    self.selection = Some(Selection {
                                        table,
                                        anchor: (origin.1, origin.2),
                                        head,
                                    });
                                    ui.ctx().request_repaint();
                                }
                            }
                        }
                        let chosen = self.selection.filter(|selection| selection.table == table);
                        let ranges = chosen.map(|selection| selection.ranges(rows, columns));
                        if let Some((selected_rows, selected_columns)) = ranges.clone()
                            && !selected_rows.is_empty()
                            && !selected_columns.is_empty()
                        {
                            let left = outer.left()
                                + 0.5
                                + placement.widths[..selected_columns.start]
                                    .iter()
                                    .sum::<f32>();
                            let top = outer.top()
                                + 0.5
                                + placement.heights[..selected_rows.start].iter().sum::<f32>();
                            let area = egui::Rect::from_min_size(
                                egui::pos2(left, top),
                                egui::vec2(
                                    placement.widths[selected_columns].iter().sum(),
                                    placement.heights[selected_rows].iter().sum(),
                                ),
                            );
                            let accent = crate::theme::accent(ui);
                            ui.painter()
                                .rect_filled(area, 0, accent.gamma_multiply(0.15));
                            ui.painter().rect_stroke(
                                area,
                                0,
                                egui::Stroke::new(2.0, accent),
                                egui::StrokeKind::Inside,
                            );
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("edit-table-selection:{table}"),
                                area.intersect(ui.clip_rect()),
                            );
                        }
                        let hovered = ui
                            .input(|input| input.pointer.hover_pos())
                            .filter(|pointer| outer.expand(6.0).contains(*pointer))
                            .map(|pointer| {
                                cell_under(outer, &placement.widths, &placement.heights, pointer)
                            });
                        let mut picked: Option<(bool, usize, bool, egui::Response)> = None;
                        let mut edge = outer.left();
                        for column in 0..columns {
                            let width = placement.widths[column];
                            let rect = egui::Rect::from_center_size(
                                egui::pos2(edge + width / 2.0, outer.top() + 4.0),
                                egui::vec2(28.0, 7.0),
                            );
                            edge += width;
                            let whole = ranges
                                .as_ref()
                                .is_some_and(|(r, c)| r.len() == rows && c.contains(&column));
                            if let Some(response) = grip(
                                ui,
                                id.with(("table-column-grip", table, column)),
                                rect,
                                hovered.is_some_and(|cell| cell.1 == column),
                                whole,
                                format!("edit-table-column-grip:{table}:{column}"),
                            ) {
                                let response = response.on_hover_text("Select column");
                                if response.clicked() || response.secondary_clicked() && !whole {
                                    picked = Some((true, column, true, response));
                                } else if response.context_menu_opened()
                                    || response.secondary_clicked()
                                {
                                    picked = picked.or(Some((true, column, false, response)));
                                }
                            }
                        }
                        let mut edge = outer.top();
                        for row in 0..rows {
                            let height = placement.heights[row];
                            let rect = egui::Rect::from_center_size(
                                egui::pos2(outer.left() + 4.0, edge + height / 2.0),
                                egui::vec2(7.0, 22.0),
                            );
                            edge += height;
                            let whole = ranges
                                .as_ref()
                                .is_some_and(|(r, c)| c.len() == columns && r.contains(&row));
                            if let Some(response) = grip(
                                ui,
                                id.with(("table-row-grip", table, row)),
                                rect,
                                hovered.is_some_and(|cell| cell.0 == row),
                                whole,
                                format!("edit-table-row-grip:{table}:{row}"),
                            ) {
                                let response = response.on_hover_text("Select row");
                                if response.clicked() || response.secondary_clicked() && !whole {
                                    picked = Some((false, row, true, response));
                                } else if response.context_menu_opened()
                                    || response.secondary_clicked()
                                {
                                    picked = picked.or(Some((false, row, false, response)));
                                }
                            }
                        }
                        if let Some((column_grip, line, select, response)) = picked {
                            if select {
                                let shift = ui.input(|input| input.modifiers.shift);
                                let anchor = chosen
                                    .filter(|_| shift)
                                    .map(|selection| selection.anchor)
                                    .unwrap_or(if column_grip { (0, line) } else { (line, 0) });
                                self.selection = Some(Selection {
                                    table,
                                    anchor: if column_grip {
                                        (0, anchor.1)
                                    } else {
                                        (anchor.0, 0)
                                    },
                                    head: if column_grip {
                                        (rows - 1, line)
                                    } else {
                                        (line, columns - 1)
                                    },
                                });
                                self.drag = None;
                                menu_focus = Some(if column_grip {
                                    (table, 0, line)
                                } else {
                                    (table, line, 0)
                                });
                            }
                            let current =
                                self.selection.filter(|selection| selection.table == table);
                            response.context_menu(|ui| {
                                crate::theme::menu_style(ui);
                                let Some(selection) = current else {
                                    return;
                                };
                                let (chosen_rows, chosen_columns) = selection.ranges(rows, columns);
                                let mut pick =
                                    |ui: &mut egui::Ui,
                                     label: &str,
                                     enabled: bool,
                                     value: Option<Action>| {
                                        let response =
                                            ui.add_enabled(enabled, egui::Button::new(label));
                                        crate::ui_probe::record(
                                            ui.ctx(),
                                            format!("table-action:{label}"),
                                            response.rect,
                                        );
                                        if response.clicked() {
                                            match value {
                                                Some(value) => {
                                                    let keep =
                                                        matches!(value, Action::Clear { .. });
                                                    edit = Some((table, value));
                                                    clear_selection |= !keep;
                                                }
                                                None => ui.ctx().copy_text(view.model.copy(
                                                    &tab.document.text,
                                                    chosen_rows.clone(),
                                                    chosen_columns.clone(),
                                                )),
                                            }
                                            menu_focus = Some((
                                                table,
                                                chosen_rows.start,
                                                chosen_columns.start,
                                            ));
                                            ui.close();
                                        }
                                    };
                                pick(ui, "Copy", true, None);
                                pick(
                                    ui,
                                    "Clear contents",
                                    true,
                                    Some(Action::Clear {
                                        rows: chosen_rows.clone(),
                                        columns: chosen_columns.clone(),
                                    }),
                                );
                                ui.separator();
                                if column_grip {
                                    pick(
                                        ui,
                                        "Insert column left",
                                        true,
                                        Some(Action::InsertColumn(chosen_columns.start)),
                                    );
                                    pick(
                                        ui,
                                        "Insert column right",
                                        true,
                                        Some(Action::InsertColumn(chosen_columns.end)),
                                    );
                                    pick(
                                        ui,
                                        if chosen_columns.len() > 1 {
                                            "Delete columns"
                                        } else {
                                            "Delete column"
                                        },
                                        chosen_columns.len() < columns,
                                        Some(Action::RemoveColumns(chosen_columns.clone())),
                                    );
                                    ui.separator();
                                    for (label, alignment) in [
                                        ("Align left", pulldown_cmark::Alignment::Left),
                                        ("Align center", pulldown_cmark::Alignment::Center),
                                        ("Align right", pulldown_cmark::Alignment::Right),
                                    ] {
                                        pick(
                                            ui,
                                            label,
                                            chosen_columns.len() == 1,
                                            Some(Action::Align {
                                                column: chosen_columns.start,
                                                alignment,
                                            }),
                                        );
                                    }
                                    pick(
                                        ui,
                                        "Sort ascending",
                                        chosen_columns.len() == 1,
                                        Some(Action::Sort {
                                            column: chosen_columns.start,
                                            descending: false,
                                        }),
                                    );
                                    pick(
                                        ui,
                                        "Sort descending",
                                        chosen_columns.len() == 1,
                                        Some(Action::Sort {
                                            column: chosen_columns.start,
                                            descending: true,
                                        }),
                                    );
                                } else {
                                    pick(
                                        ui,
                                        "Insert row above",
                                        chosen_rows.start > 0,
                                        Some(Action::InsertRow(chosen_rows.start)),
                                    );
                                    pick(
                                        ui,
                                        "Insert row below",
                                        true,
                                        Some(Action::InsertRow(chosen_rows.end)),
                                    );
                                    pick(
                                        ui,
                                        if chosen_rows.len() > 1 {
                                            "Delete rows"
                                        } else {
                                            "Delete row"
                                        },
                                        chosen_rows.start > 0,
                                        Some(Action::RemoveRows(chosen_rows.clone())),
                                    );
                                }
                            });
                        }
                        crate::table_layout::handles(
                            ui,
                            &tab.document.path,
                            table,
                            outer,
                            &placement.widths,
                            placement.available,
                        );
                        for column in [false, true] {
                            let edge = if column {
                                egui::Rect::from_min_size(
                                    outer.right_top(),
                                    egui::vec2(EDGE_SIZE, outer.height()),
                                )
                            } else {
                                egui::Rect::from_min_size(
                                    outer.left_bottom(),
                                    egui::vec2(outer.width(), EDGE_SIZE),
                                )
                            };
                            let edge_id = id.with(("table-add", table, column));
                            if edge_button(ui, edge_id, table, edge, column) {
                                let rows = view.model.rows.len();
                                let columns = view.model.alignments.len();
                                insertion = Some((
                                    table,
                                    if column {
                                        Action::InsertColumn(columns)
                                    } else {
                                        Action::InsertRow(rows)
                                    },
                                ));
                                menu_focus = Some(if column {
                                    (table, 0, columns)
                                } else {
                                    (table, rows, 0)
                                });
                            }
                        }
                    });
            });
        }
        if follow.is_none() && edit.is_none() && insertion.is_none() {
            follow = self.link_click(ui, &link_cells, &events);
        } else {
            self.link_press = None;
            self.pending_link = None;
        }
        // A press outside every table ends the range; handles and menus keep it.
        let outside = ui.input(|input| {
            input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pointer| !table_areas.iter().any(|area| area.contains(pointer)))
        });
        if clear_selection || outside && !egui::Popup::is_any_open(ui.ctx()) {
            self.selection = None;
        }
        if let Some((table, action)) = edit {
            self.apply(ui, tab, id, table, action);
            if separate_edit {
                checkpoint(ui.ctx(), id, &tab.document.text);
            }
        }
        if let Some((table, action)) = insertion {
            // A queued cell edit must reach the document before its structure
            // changes; refresh offsets rather than discarding that input.
            if let Some(model) = rustidian::editor::Markdown::parse(&tab.document.text)
                .tables
                .get(table)
            {
                self.views[table].model = model.clone();
                self.apply(ui, tab, id, table, action);
            }
        }
        if let Some((table, row, column)) = menu_focus
            && let Some(model) = rustidian::editor::Markdown::parse(&tab.document.text)
                .tables
                .get(table)
        {
            self.focus = Some((
                table,
                row.min(model.rows.len() - 1),
                column.min(model.alignments.len() - 1),
            ));
        }
        if let Some(((table, row, column), direction)) = navigation {
            let columns = self.views[table].model.alignments.len();
            let rows = self.views[table].model.rows.len();
            let destination = match direction {
                -1 if column > 0 => Some((table, row, column - 1)),
                -1 if row > 0 => Some((table, row - 1, columns - 1)),
                1 if column + 1 < columns => Some((table, row, column + 1)),
                1 | 2 if row + 1 < rows => {
                    Some((table, row + 1, if direction == 2 { column } else { 0 }))
                }
                1 | 2 => {
                    // A changed cell invalidates the original offsets; parse once before the insertion.
                    let models = rustidian::editor::Markdown::parse(&tab.document.text).tables;
                    if let Some(model) = models.get(table) {
                        self.views[table].model = model.clone();
                    }
                    self.apply(ui, tab, id, table, Action::InsertRow(rows));
                    Some((table, rows, if direction == 2 { column } else { 0 }))
                }
                _ => {
                    exit = Some(
                        placements
                            .iter()
                            .find(|placement| placement.ordinal == table)
                            .map_or(0, |placement| placement.characters.start.saturating_sub(1)),
                    );
                    None
                }
            };
            checkpoint(ui.ctx(), id, &tab.document.text);
            self.focus = destination;
            ui.ctx().request_repaint();
        }
        if let Some(character) = exit {
            self.exit = Some(character);
            self.focus = None;
            ui.ctx().request_repaint();
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        follow
    }

    fn link_click(
        &mut self,
        ui: &egui::Ui,
        cells: &[LinkCell],
        events: &[egui::Event],
    ) -> Option<String> {
        if self.link_press.is_none() && self.pending_link.is_none() && cells.is_empty() {
            return None;
        }
        let options = ui.ctx().options(|options| options.input_options);
        let now = ui.input(|input| input.time);
        let hit = |pos: egui::Pos2| {
            cells.iter().find(|cell| {
                let local = ui
                    .ctx()
                    .layer_transform_from_global(cell.layer)
                    .map_or(pos, |transform| transform * pos);
                cell.rect.contains(local) && ui.ctx().layer_id_at(pos) == Some(cell.layer)
            })
        };
        for event in events {
            match event {
                egui::Event::PointerMoved(pos) => {
                    if let Some(press) = &mut self.link_press {
                        press.moved |= press.origin.distance(*pos) > options.max_click_dist;
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers,
                } => {
                    let pending = self.pending_link.take();
                    self.link_press = hit(*pos)
                        .filter(|_| modifiers.matches_exact(egui::Modifiers::NONE))
                        .map(|cell| LinkPress {
                            key: cell.key,
                            origin: *pos,
                            moved: false,
                            double: pending.is_some_and(|previous| {
                                previous.key == cell.key
                                    && now <= previous.deadline
                                    && previous.position.distance(*pos) <= options.max_click_dist
                            }),
                        });
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers,
                } => {
                    if let Some(press) = self.link_press.take()
                        && !press.double
                        && !press.moved
                        && press.origin.distance(*pos) <= options.max_click_dist
                        && modifiers.matches_exact(egui::Modifiers::NONE)
                        && let Some(cell) = hit(*pos).filter(|cell| cell.key == press.key)
                    {
                        // A single click opens after the double-click interval; a second
                        // press cancels navigation and leaves TextEdit to select the word.
                        self.pending_link = Some(PendingLink {
                            key: cell.key,
                            target: cell.target.clone(),
                            position: *pos,
                            deadline: now + options.max_double_click_delay,
                        });
                    }
                }
                egui::Event::PointerGone => self.link_press = None,
                egui::Event::PointerButton { .. }
                | egui::Event::Key { pressed: true, .. }
                | egui::Event::Text(_)
                | egui::Event::Paste(_)
                | egui::Event::Cut
                | egui::Event::Copy
                | egui::Event::Ime(_)
                | egui::Event::WindowFocused(false) => {
                    self.link_press = None;
                    self.pending_link = None;
                }
                _ => {}
            }
        }
        if let Some(pending) = &self.pending_link {
            if now >= pending.deadline {
                return self.pending_link.take().map(|link| link.target);
            }
            ui.ctx()
                .request_repaint_after_secs((pending.deadline - now) as f32);
        }
        None
    }

    /// While a cell range is selected, copy, cut, Delete/Backspace and Escape act on the range.
    fn selection_keys(&mut self, ui: &egui::Ui, tab: &mut Tab) {
        let Some(selection) = self.selection else {
            return;
        };
        let Some(view) = self.views.get(selection.table) else {
            self.selection = None;
            return;
        };
        let (rows, columns) = selection.ranges(view.cells.len(), view.model.alignments.len());
        let (mut copy, mut clear, mut cancel) = (false, false, false);
        let mut delete_key = false;
        ui.input_mut(|input| {
            input.events.retain(|event| match event {
                egui::Event::Copy => {
                    copy = true;
                    false
                }
                egui::Event::Cut => {
                    (copy, clear) = (true, true);
                    false
                }
                egui::Event::Key {
                    key: Key::Backspace | Key::Delete,
                    pressed: true,
                    ..
                } => {
                    (clear, delete_key) = (true, true);
                    false
                }
                egui::Event::Key {
                    key: Key::Escape,
                    pressed: true,
                    ..
                } => {
                    cancel = true;
                    false
                }
                egui::Event::Text(_) | egui::Event::Paste(_) => {
                    cancel = true;
                    true
                }
                _ => true,
            })
        });
        if copy {
            ui.ctx().copy_text(
                view.model
                    .copy(&tab.document.text, rows.clone(), columns.clone()),
            );
        }
        if clear
            && let Some(model) = rustidian::editor::Markdown::parse(&tab.document.text)
                .tables
                .get(selection.table)
        {
            self.views[selection.table].model = model.clone();
            self.draft = None;
            let id = self.owner.unwrap_or(egui::Id::NULL);
            // Delete or Backspace over whole body rows removes the rows, as Delete rows in
            // the menu does; the header row and partial ranges only lose their contents.
            let whole_rows = delete_key
                && rows.start > 0
                && columns.len() == self.views[selection.table].model.alignments.len();
            let action = if whole_rows {
                self.selection = None;
                Action::RemoveRows(rows)
            } else {
                Action::Clear { rows, columns }
            };
            self.apply(ui, tab, id, selection.table, action);
        }
        if cancel {
            self.selection = None;
        }
    }

    fn apply(&mut self, ui: &egui::Ui, tab: &mut Tab, id: egui::Id, table: usize, action: Action) {
        let structural = !matches!(action, Action::Cell { .. });
        if structural {
            checkpoint(ui.ctx(), id, &tab.document.text);
        }
        let before = tab.document.text.clone();
        let mut next_draft = None;
        let result = if let Action::Cell { row, column, text } = action {
            let key = (table, row, column);
            let tracked = self
                .draft
                .as_ref()
                .filter(|draft| draft.key == key)
                .map(|draft| &draft.cell);
            self.views[table]
                .model
                .edit_cell(&mut tab.document.text, row, column, text, tracked)
                .map(|cell| {
                    next_draft = Some(Draft { key, cell });
                })
        } else {
            self.views[table]
                .model
                .apply(&mut tab.document.text, action)
        };
        match result {
            Ok(()) => {
                self.draft = next_draft;
                self.error = None;
                if before != tab.document.text {
                    tab.edited();
                    if structural {
                        checkpoint(ui.ctx(), id, &tab.document.text);
                    } else {
                        feed_history(ui.ctx(), id, &tab.document.text);
                    }
                    ui.ctx().request_repaint();
                }
            }
            Err(error) => self.error = Some(error.into()),
        }
    }
}

fn feed_history(ctx: &egui::Context, id: egui::Id, text: &str) {
    let state = TextEditState::load(ctx, id).unwrap_or_default();
    state.feed_undo_state(ctx.input(|input| input.time), text);
    state.store(ctx, id);
}

pub(crate) fn checkpoint(ctx: &egui::Context, id: egui::Id, text: &str) {
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    let cursor = state.cursor.char_range().unwrap_or_default();
    let mut undoer = state.undoer();
    undoer.add_undo(&(cursor, text.to_owned()));
    state.set_undoer(undoer);
    state.store(ctx, id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(key: Key, modifiers: egui::Modifiers) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    #[test]
    fn table_navigation_preserves_the_text_after_each_action_for_its_destination() {
        let mut events = vec![
            egui::Event::Text("first".into()),
            key(Key::Tab, egui::Modifiers::NONE),
            egui::Event::Text("second".into()),
            key(Key::Enter, egui::Modifiers::NONE),
            egui::Event::Text("third".into()),
            key(Key::S, egui::Modifiers::COMMAND),
        ];
        let mut deferred = Vec::new();
        assert_eq!(
            take_action(&mut events, &mut deferred),
            Some(TableAction::Next)
        );
        assert_eq!(events, vec![egui::Event::Text("first".into())]);
        assert_eq!(deferred.len(), 4);
        events = std::mem::take(&mut deferred);
        assert_eq!(
            take_action(&mut events, &mut deferred),
            Some(TableAction::Down)
        );
        assert_eq!(events, vec![egui::Event::Text("second".into())]);
        assert_eq!(
            deferred,
            vec![
                egui::Event::Text("third".into()),
                key(Key::S, egui::Modifiers::COMMAND)
            ]
        );
    }

    #[test]
    fn table_actions_use_exact_modifiers_and_never_skip_an_earlier_action() {
        for (key_value, modifiers, expected) in [
            (
                Key::Tab,
                egui::Modifiers::SHIFT,
                Some(TableAction::Previous),
            ),
            (
                Key::Enter,
                egui::Modifiers::COMMAND,
                Some(TableAction::Follow),
            ),
            (Key::Tab, egui::Modifiers::ALT, None),
            (Key::Enter, egui::Modifiers::SHIFT, None),
            (
                Key::Enter,
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                None,
            ),
        ] {
            let mut events = vec![
                key(key_value, modifiers),
                key(Key::Tab, egui::Modifiers::NONE),
            ];
            let original = events.clone();
            let mut deferred = Vec::new();
            assert_eq!(take_action(&mut events, &mut deferred), expected);
            if expected.is_none() {
                assert_eq!(events, original);
                assert!(deferred.is_empty());
            }
            if expected == Some(TableAction::Follow) {
                assert_eq!(events, vec![key(Key::Tab, egui::Modifiers::NONE)]);
                assert!(deferred.is_empty());
            }
        }
    }
}
