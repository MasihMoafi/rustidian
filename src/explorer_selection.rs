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
fn update<T>(ctx: &egui::Context, change: impl FnOnce(&mut Selection) -> T) -> T {
    ctx.data_mut(|data| change(data.get_temp_mut_or_default(id())))
}

pub fn begin(ctx: &egui::Context, active: Option<&Path>) {
    let active = active.filter(|path| !path.as_os_str().is_empty());
    update(ctx, |state| {
        if state.active.as_deref() != active {
            state.paths.clear();
            state.paths.extend(active.map(Path::to_owned));
            state.anchor = active.map(Path::to_owned);
            state.active = active.map(Path::to_owned);
        }
        state.seen.clear();
    });
}

pub fn record(ctx: &egui::Context, paths: impl IntoIterator<Item = PathBuf>) -> usize {
    update(ctx, |state| {
        state.seen.extend(paths);
        state.seen.len()
    })
}

pub fn truncate(ctx: &egui::Context, count: usize) {
    update(ctx, |state| state.seen.truncate(count));
}

pub fn contains(ctx: &egui::Context, path: &Path) -> bool {
    update(ctx, |state| state.paths.contains(path))
}

pub fn prepare_drag(ctx: &egui::Context, path: &Path) -> Vec<PathBuf> {
    update(ctx, |state| {
        if !state.paths.contains(path) {
            state.paths.clear();
            state.paths.insert(path.to_owned());
            state.anchor = Some(path.to_owned());
        }
    });
    roots(ctx)
}

pub fn moved(ctx: &egui::Context, source: &Path, destination: &Path) {
    let relocate = |path: &Path| rustidian::paths::moved_path(path, source, destination, true);
    update(ctx, |state| {
        state.paths = std::mem::take(&mut state.paths)
            .into_iter()
            .map(|path| relocate(&path))
            .collect();
        state.anchor = state.anchor.as_deref().map(relocate);
        state.active = state.active.as_deref().map(relocate);
        state.order.clear();
        state.seen.clear();
    });
}

pub fn roots(ctx: &egui::Context) -> Vec<PathBuf> {
    update(ctx, |state| {
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
    })
}

/// Return true when this click changes a selection without opening the note.
pub fn row(ui: &egui::Ui, response: &egui::Response, path: &Path) -> bool {
    let modifiers = ui.input(|input| input.modifiers);
    let clicked = response.clicked();
    let secondary = response.secondary_clicked();
    if clicked {
        response.request_focus();
    }
    let selected = update(ui.ctx(), |state| {
        if secondary {
            if !state.paths.contains(path) {
                state.paths.clear();
                state.paths.insert(path.to_owned());
                state.anchor = Some(path.to_owned());
            }
        } else if clicked {
            state.focus = Some(response.id);
            if modifiers.shift {
                let anchor = state.anchor.as_deref().unwrap_or(path);
                if let (Some(a), Some(b)) = (
                    state.order.iter().position(|p| p == anchor),
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
        state.paths.contains(path)
    });
    if selected {
        crate::ui_probe::record(
            ui.ctx(),
            format!("explorer-selected-path:{}", path.display()),
            response.rect,
        );
    }
    clicked && (modifiers.command || modifiers.shift)
}

pub fn menu(ui: &mut egui::Ui, action: &mut Option<crate::file_tree::FileAction>) -> bool {
    use crate::file_tree::FileAction;
    let count = update(ui.ctx(), |state| state.paths.len());
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
    let focus = update(ui.ctx(), |state| state.focus);
    let focused = focus.is_some_and(|id| ui.memory(|memory| memory.has_focus(id)));
    let all = focused && ui.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::A));
    let trash = focused && ui.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Delete));
    let count = update(ui.ctx(), |state| {
        state.order = std::mem::take(&mut state.seen);
        if all {
            state.paths = state.order.iter().cloned().collect();
        }
        state.paths.len()
    });
    crate::ui_probe::record(
        ui.ctx(),
        format!("explorer-selected:{count}"),
        ui.min_rect(),
    );
    (trash && count > 0).then(|| crate::file_tree::FileAction::TrashMany(roots(ui.ctx())))
}

pub fn select(ctx: &egui::Context, path: &Path) {
    update(ctx, |state| {
        state.paths.clear();
        state.paths.insert(path.to_owned());
        state.anchor = Some(path.to_owned());
        state.focus = None;
        state.order.clear();
        state.seen.clear();
    });
}

pub fn reset(ctx: &egui::Context) {
    update(ctx, |state| *state = Selection::default());
}
