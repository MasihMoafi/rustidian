use eframe::egui::{self, Key, Modifiers};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

#[derive(Clone, Default)]
struct Selection {
    paths: BTreeSet<PathBuf>,
    anchor: Option<PathBuf>,
    active: Option<PathBuf>,
    order: Vec<PathBuf>,
    seen: Vec<PathBuf>,
    focus: Option<egui::Id>,
}

fn id() -> egui::Id {
    egui::Id::unique("explorer-selection")
}
fn get(ctx: &egui::Context) -> Selection {
    ctx.data(|data| data.get_temp(id())).unwrap_or_default()
}
fn put(ctx: &egui::Context, state: Selection) {
    ctx.data_mut(|data| data.insert_temp(id(), state));
}

pub fn begin(ctx: &egui::Context, active: Option<&Path>) {
    let active = active.filter(|path| !path.as_os_str().is_empty());
    let mut state = get(ctx);
    if state.active.as_deref() != active {
        state.paths.clear();
        state.paths.extend(active.map(Path::to_owned));
        state.anchor = active.map(Path::to_owned);
        state.active = active.map(Path::to_owned);
    }
    state.seen.clear();
    put(ctx, state);
}

pub fn record(ctx: &egui::Context, paths: impl IntoIterator<Item = PathBuf>) -> usize {
    let mut state = get(ctx);
    state.seen.extend(paths);
    let count = state.seen.len();
    put(ctx, state);
    count
}

pub fn truncate(ctx: &egui::Context, count: usize) {
    let mut state = get(ctx);
    state.seen.truncate(count);
    put(ctx, state);
}

pub fn contains(ctx: &egui::Context, path: &Path) -> bool {
    get(ctx).paths.contains(path)
}

pub fn roots(ctx: &egui::Context) -> Vec<PathBuf> {
    let state = get(ctx);
    state
        .paths
        .iter()
        .filter(|path| {
            !state
                .paths
                .iter()
                .any(|parent| parent != *path && path.starts_with(parent))
        })
        .cloned()
        .collect()
}

/// Return true when this click changes a selection without opening the note.
pub fn row(ui: &egui::Ui, response: &egui::Response, path: &Path) -> bool {
    let mut state = get(ui.ctx());
    let modifiers = ui.input(|input| input.modifiers);
    if response.secondary_clicked() {
        if !state.paths.contains(path) {
            state.paths.clear();
            state.paths.insert(path.to_owned());
            state.anchor = Some(path.to_owned());
        }
    } else if response.clicked() {
        response.request_focus();
        state.focus = Some(response.id);
        if modifiers.shift {
            let anchor = state.anchor.clone().unwrap_or_else(|| path.to_owned());
            if let (Some(a), Some(b)) = (
                state.order.iter().position(|p| *p == anchor),
                state.order.iter().position(|p| p == path),
            ) {
                if !modifiers.command {
                    state.paths.clear();
                }
                state
                    .paths
                    .extend(state.order[a.min(b)..=a.max(b)].iter().cloned());
            } else {
                state.paths.insert(path.to_owned());
            }
        } else if modifiers.command {
            if !state.paths.remove(path) {
                state.paths.insert(path.to_owned());
            }
            state.anchor = Some(path.to_owned());
        } else {
            state.paths.clear();
            state.paths.insert(path.to_owned());
            state.anchor = Some(path.to_owned());
        }
    }
    if state.paths.contains(path) {
        crate::ui_probe::record(
            ui.ctx(),
            format!("explorer-selected-path:{}", path.display()),
            response.rect,
        );
    }
    put(ui.ctx(), state);
    response.clicked() && (modifiers.command || modifiers.shift)
}

pub fn menu(ui: &mut egui::Ui, action: &mut Option<crate::file_tree::FileAction>) -> bool {
    use crate::file_tree::FileAction;
    let count = get(ui.ctx()).paths.len();
    if count < 2 {
        return false;
    }
    ui.label(format!("{count} selected"));
    let move_items = ui.button("Move selected items…");
    crate::ui_probe::record(ui.ctx(), "explorer-move-selected", move_items.rect);
    if move_items.clicked() {
        *action = Some(FileAction::MoveMany(roots(ui.ctx())));
        ui.close();
    }
    let trash = ui.button("Delete selected items…");
    crate::ui_probe::record(ui.ctx(), "explorer-trash-selected", trash.rect);
    if trash.clicked() {
        *action = Some(FileAction::TrashMany(roots(ui.ctx())));
        ui.close();
    }
    true
}

pub fn finish(ui: &egui::Ui) -> Option<crate::file_tree::FileAction> {
    let mut state = get(ui.ctx());
    state.order = std::mem::take(&mut state.seen);
    let focused = state
        .focus
        .is_some_and(|id| ui.memory(|memory| memory.has_focus(id)));
    if focused && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::A)) {
        state.paths = state.order.iter().cloned().collect();
    }
    let trash = focused
        && !state.paths.is_empty()
        && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Delete));
    crate::ui_probe::record(
        ui.ctx(),
        format!("explorer-selected:{}", state.paths.len()),
        ui.min_rect(),
    );
    put(ui.ctx(), state);
    trash.then(|| crate::file_tree::FileAction::TrashMany(roots(ui.ctx())))
}

pub fn reset(ctx: &egui::Context) {
    put(ctx, Selection::default());
}
