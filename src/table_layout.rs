use eframe::egui::{self, Context, Id, Key, Modifiers, widgets::text_edit::TextEditState};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub type Layouts = BTreeMap<PathBuf, BTreeMap<usize, Vec<f32>>>;
const MIN_WIDTH: f32 = 56.0;

#[derive(Clone)]
struct Change {
    path: PathBuf,
    table: usize,
    before: Vec<f32>,
    after: Vec<f32>,
}

#[derive(Clone)]
struct Drag {
    path: PathBuf,
    table: usize,
    column: usize,
    start: f32,
    before: Vec<f32>,
    cancelled: bool,
}

type Commands = BTreeMap<PathBuf, BTreeMap<u64, Change>>;

fn storage() -> Id {
    Id::unique("table-column-widths")
}

pub fn restore(ctx: &Context, layouts: Layouts) {
    ctx.data_mut(|data| {
        data.insert_temp(storage(), layouts);
        data.remove::<Drag>(storage().with("drag"));
        data.remove::<Vec<Change>>(storage().with("pending"));
        data.remove::<Commands>(storage().with("commands"));
    });
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

fn set(ctx: &Context, path: &Path, table: usize, widths: Vec<f32>, changed: bool) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Layouts>(storage())
            .entry(path.to_owned())
            .or_default()
            .insert(table, widths);
        if changed {
            data.insert_temp(storage().with("changed"), true);
        }
    });
    ctx.request_repaint();
}

fn budget(available: f32, columns: usize) -> f32 {
    // Very wide tables retain horizontal scrolling at the readable minimum.
    (available - 1.0).max(columns as f32 * MIN_WIDTH)
}

fn fit(widths: &[f32], available: f32) -> Vec<f32> {
    let mut widths: Vec<_> = widths
        .iter()
        .map(|width| {
            if width.is_finite() {
                width.max(MIN_WIDTH)
            } else {
                MIN_WIDTH
            }
        })
        .collect();
    let excess = widths.iter().sum::<f32>() - budget(available, widths.len());
    if excess > 0.01 {
        let capacity: f32 = widths.iter().map(|width| width - MIN_WIDTH).sum();
        if capacity > 0.0 {
            for width in &mut widths {
                *width = MIN_WIDTH + (*width - MIN_WIDTH) * (1.0 - excess / capacity).max(0.0);
            }
        }
    }
    widths
}

pub fn widths(
    ctx: &Context,
    from: &Path,
    table: usize,
    natural: &[f32],
    available: f32,
) -> Vec<f32> {
    let saved: Option<Vec<f32>> = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Layouts>(storage())
            .get(from)
            .and_then(|tables| tables.get(&table))
            .filter(|widths| widths.len() == natural.len())
            .cloned()
    });
    // Fit each rendering surface without changing the saved preference. A narrow
    // hover preview or split pane must not resize the main document's table.
    fit(saved.as_deref().unwrap_or(natural), available)
}

pub fn relocate(ctx: &Context, from: &Path, to: &Path) {
    ctx.data_mut(|data| {
        let layouts = data.get_temp_mut_or_default::<Layouts>(storage());
        let moving: Vec<_> = layouts
            .keys()
            .filter(|path| path.starts_with(from))
            .cloned()
            .collect();
        for path in moving {
            if let Some(widths) = layouts.remove(&path) {
                layouts.insert(rustidian::paths::moved_path(&path, from, to, true), widths);
            }
        }
        let commands = data.get_temp_mut_or_default::<Commands>(storage().with("commands"));
        let moving: Vec<_> = commands
            .keys()
            .filter(|path| path.starts_with(from))
            .cloned()
            .collect();
        for path in moving {
            if let Some(mut changes) = commands.remove(&path) {
                let target = rustidian::paths::moved_path(&path, from, to, true);
                for change in changes.values_mut() {
                    change.path = target.clone();
                }
                commands.insert(target, changes);
            }
        }
        for change in data.get_temp_mut_or_default::<Vec<Change>>(storage().with("pending")) {
            change.path = rustidian::paths::moved_path(&change.path, from, to, true);
        }
        if let Some(mut drag) = data.get_temp::<Drag>(storage().with("drag")) {
            drag.path = rustidian::paths::moved_path(&drag.path, from, to, true);
            data.insert_temp(storage().with("drag"), drag);
        }
    });
}

pub fn has_focus(ctx: &Context, from: &Path) -> bool {
    let focused = ctx.data(|data| data.get_temp::<Id>(storage().with(("focus", from))));
    focused.is_some_and(|id| ctx.memory(|memory| memory.has_focus(id)))
}

// Layout commands use real marker edges in the document's shared text history.
// No note text, dirty flag or saved revision changes for a layout action.
pub fn history(
    ctx: &Context,
    from: &Path,
    text: &str,
    state: &mut TextEditState,
    shortcuts: bool,
) -> bool {
    if let Some(mut drag) = ctx.data(|data| data.get_temp::<Drag>(storage().with("drag"))) {
        if !drag.cancelled
            && (drag.path != from
                || ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)))
        {
            set(ctx, &drag.path, drag.table, drag.before.clone(), true);
            drag.cancelled = true;
            ctx.data_mut(|data| data.insert_temp(storage().with("drag"), drag.clone()));
        }
        if drag.cancelled && !ctx.input(|input| input.pointer.primary_down()) {
            ctx.data_mut(|data| data.remove::<Drag>(storage().with("drag")));
        }
    }
    let pending = ctx.data_mut(|data| {
        let events = data.get_temp_mut_or_default::<Vec<Change>>(storage().with("pending"));
        let mut current = Vec::new();
        events.retain(|change| {
            if change.path == from {
                current.push(change.clone());
                false
            } else {
                true
            }
        });
        current
    });
    let cursor = state.cursor.char_range().unwrap_or_default();
    if !pending.is_empty() {
        let mut history = state.undoer();
        for change in pending {
            let marker = ctx.data_mut(|data| {
                let sequence = data.get_temp_mut_or_default::<u64>(storage().with("sequence"));
                *sequence += 1;
                *sequence
            });
            history.add_marker(&(cursor, text.to_owned()), marker);
            ctx.data_mut(|data| {
                let commands = data
                    .get_temp_mut_or_default::<Commands>(storage().with("commands"))
                    .entry(from.to_owned())
                    .or_default();
                commands.insert(marker, change);
                while commands.len() > 100 {
                    commands.pop_first();
                }
            });
        }
        state.set_undoer(history);
    }
    if !shortcuts {
        return false;
    }
    // Do not overtake queued text. The editor must apply an earlier mutation
    // before it decides which command this Undo/Redo belongs to.
    let request = ctx.input(|input| {
        for (index, event) in input.events.iter().enumerate() {
            match event {
                egui::Event::Text(_)
                | egui::Event::Paste(_)
                | egui::Event::Cut
                | egui::Event::Ime(egui::ImeEvent::Commit(_)) => return None,
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let redo = *key == Key::Y && modifiers.matches_logically(Modifiers::COMMAND)
                        || *key == Key::Z
                            && modifiers.matches_logically(Modifiers::COMMAND | Modifiers::SHIFT);
                    let undo = *key == Key::Z && modifiers.matches_logically(Modifiers::COMMAND);
                    if undo || redo {
                        return Some((index, redo));
                    }
                    if matches!(key, Key::Backspace | Key::Delete | Key::Enter | Key::Tab) {
                        return None;
                    }
                }
                _ => {}
            }
        }
        None
    });
    let Some((event_index, redo)) = request else {
        return false;
    };
    let mut history = state.undoer();
    let mut current = (cursor, text.to_owned());
    loop {
        let marker = history.step_marker(&current, redo);
        let next = if redo {
            history.redo(&current)
        } else {
            history.undo(&current)
        }
        .cloned();
        let Some(next) = next else {
            return false;
        };
        if let Some(marker) = marker {
            let change = ctx.data(|data| {
                data.get_temp::<Commands>(storage().with("commands"))
                    .and_then(|commands| commands.get(from)?.get(&marker).cloned())
            });
            if let Some(change) = change {
                set(
                    ctx,
                    from,
                    change.table,
                    if redo { change.after } else { change.before },
                    true,
                );
                state.cursor.set_char_range(Some(next.0));
                state.set_undoer(history);
                ctx.input_mut(|input| {
                    input.events.remove(event_index);
                });
                return true;
            }
            return false;
        }
        if next.1 != text {
            return false;
        }
        current = next;
    }
}

pub fn handles(
    ui: &mut egui::Ui,
    from: &Path,
    table: usize,
    outer: egui::Rect,
    widths: &[f32],
    available: f32,
) {
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
            .on_hover_text("Drag to resize column · arrow keys adjust width · Ctrl+Z undoes");
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
        if response.clicked() || response.drag_started() {
            response.request_focus();
            ui.ctx()
                .data_mut(|data| data.insert_temp(storage().with(("focus", from)), id));
        }
        if response.drag_started() {
            let start = ui
                .input(|input| input.pointer.press_origin())
                .unwrap_or(rect.center())
                .x;
            ui.ctx().data_mut(|data| {
                data.insert_temp(
                    storage().with("drag"),
                    Drag {
                        path: from.to_owned(),
                        table,
                        column,
                        start,
                        before: widths.to_vec(),
                        cancelled: false,
                    },
                )
            });
        }
        let drag = ui
            .ctx()
            .data(|data| data.get_temp::<Drag>(storage().with("drag")))
            .filter(|drag| drag.path == from && drag.table == table && drag.column == column);
        let mut next = None;
        if response.dragged()
            && let Some(pointer) = response.interact_pointer_pos()
            && let Some(drag) = &drag
            && !drag.cancelled
        {
            next = Some(drag.before[column] + pointer.x - drag.start);
        }
        if response.has_focus() {
            ui.memory_mut(|memory| {
                memory.set_focus_lock_filter(
                    id,
                    egui::EventFilter {
                        horizontal_arrows: true,
                        escape: true,
                        ..Default::default()
                    },
                )
            });
            if drag.is_none()
                && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
            {
                response.surrender_focus();
            }
            if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowLeft)) {
                next = Some(*width - 8.0);
            }
            if ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::ArrowRight)) {
                next = Some(*width + 8.0);
            }
        }
        let mut resized = widths.to_vec();
        if let Some(next) = next {
            let others = widths.iter().sum::<f32>() - width;
            resized[column] = next.clamp(
                MIN_WIDTH,
                (budget(available, widths.len()) - others).max(MIN_WIDTH),
            );
            set(ui.ctx(), from, table, resized.clone(), false);
        }
        let before = if response.drag_stopped() {
            drag.filter(|drag| !drag.cancelled).map(|drag| drag.before)
        } else if next.is_some() && !response.dragged() {
            Some(widths.to_vec())
        } else {
            None
        };
        if let Some(before) = before
            && before != resized
        {
            ui.ctx().data_mut(|data| {
                data.get_temp_mut_or_default::<Vec<Change>>(storage().with("pending"))
                    .push(Change {
                        path: from.to_owned(),
                        table,
                        before,
                        after: resized,
                    });
                data.insert_temp(storage().with("changed"), true);
            });
            ui.ctx().request_repaint();
        }
        if response.drag_stopped() {
            ui.ctx()
                .data_mut(|data| data.remove::<Drag>(storage().with("drag")));
        }
    }
}
