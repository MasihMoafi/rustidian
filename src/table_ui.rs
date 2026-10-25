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

#[derive(Default)]
pub struct Tables {
    owner: Option<egui::Id>,
    views: Vec<TableView>,
    focus: Option<CellKey>,
    error: Option<String>,
    draft: Option<Draft>,
    deferred: Vec<egui::Event>,
    exit: Option<usize>,
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
        let tail = events.split_off(index + 1);
        events.pop();
        for event in tail {
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
    Some(action)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub ordinal: usize,
    pub bytes: Range<usize>,
    pub characters: Range<usize>,
    pub widths: Vec<f32>,
    pub heights: Vec<f32>,
}

impl Placement {
    pub fn height(&self) -> f32 {
        self.heights.iter().sum::<f32>() + 1.0
    }
}

fn cell_id(id: egui::Id, key: CellKey) -> egui::Id {
    id.with(("table-cell", key))
}

impl Tables {
    pub fn prepare(
        &mut self,
        ui: &egui::Ui,
        source: &str,
        models: &[Table],
        active_line: Option<&Range<usize>>,
        caret: Option<usize>,
        hidden: &[Range<usize>],
    ) -> Vec<Placement> {
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
                let start = source[..view.model.range.start].chars().count();
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
        if !live {
            self.focus = None;
            self.deferred.clear();
            self.draft = None;
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
        let focused = self.focus.is_some()
            || self.views.iter().enumerate().any(|(table, view)| {
                view.cells.iter().enumerate().any(|(row, cells)| {
                    (0..cells.len())
                        .any(|column| ui.memory(|m| m.has_focus(cell_id(id, (table, row, column)))))
                })
            });
        if !focused {
            return;
        }
        let redo = ui.input_mut(|input| {
            input.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::Z)
                || input.consume_key(egui::Modifiers::COMMAND, Key::Y)
        });
        let undo =
            !redo && ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, Key::Z));
        if !(undo || redo) {
            return;
        }
        let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
        let cursor = state.cursor.char_range().unwrap_or_default();
        let mut history = state.undoer();
        let current = (cursor, tab.document.text.clone());
        let restored = if redo {
            history.redo(&current)
        } else {
            history.undo(&current)
        }
        .cloned();
        if let Some((cursor, text)) = restored {
            self.draft = None;
            tab.document.text = text;
            state.cursor.set_char_range(Some(cursor));
            tab.edited();
            ui.ctx().request_repaint();
        }
        state.set_undoer(history);
        state.store(ui.ctx(), id);
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
        let mut edit = None;
        let mut navigation = None;
        let mut menu_focus = None;
        let mut exit = None;
        let pending_focus = self.focus.take();
        for (placement, origin) in placements.iter().zip(origins) {
            let table = placement.ordinal;
            let view = &mut self.views[table];
            let rect = egui::Rect::from_min_size(
                *origin,
                egui::vec2(
                    placement.widths.iter().sum::<f32>() + 1.0,
                    placement.height(),
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
                        let (outer, _) = ui.allocate_exact_size(rect.size(), egui::Sense::hover());
                        crate::ui_probe::record(
                            ui.ctx(),
                            format!("edit-table:{table}"),
                            outer.intersect(ui.clip_rect()),
                        );
                        let border = ui.visuals().widgets.noninteractive.bg_stroke;
                        ui.painter().rect_filled(outer, 0, ui.visuals().window_fill);
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
                                let rendered =
                                    cell.presentation.layout(ui, row == 0, inner.width());
                                let mut layouter =
                                    |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, _width: f32| {
                                        if focused {
                                            ui.fonts_mut(|fonts| {
                                                fonts.layout_no_wrap(
                                                    buffer.as_str().into(),
                                                    egui::FontId::proportional(
                                                        crate::theme::document_size(ui, 18.0),
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
                                        .font(egui::FontId::proportional(
                                            crate::theme::document_size(ui, 18.0),
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
                                if let Some(direction) = nav {
                                    navigation = Some((key, direction));
                                }
                                response.context_menu(|ui| {
                                    let columns = view.model.alignments.len();
                                    let count = view.model.rows.len();
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
                    });
            });
        }
        if let Some((table, action)) = edit {
            self.apply(ui, tab, id, table, action);
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
                    }
                    ui.ctx().request_repaint();
                }
            }
            Err(error) => self.error = Some(error.into()),
        }
    }
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
