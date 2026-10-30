use crate::explorer_selection as selection;
use crate::theme;
use eframe::egui::{self, Color32, FontId, RichText, Stroke};
use rustidian::vault::Vault;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub enum FileAction {
    NewNote(PathBuf),
    NewFolder(PathBuf),
    Open(PathBuf),
    OpenInNewTab(PathBuf),
    Copy(PathBuf),
    Move(PathBuf),
    MoveMany(Vec<PathBuf>),
    TrashMany(Vec<PathBuf>),
    MoveManyInto {
        sources: Vec<PathBuf>,
        folder: PathBuf,
    },
    CopyPath(PathBuf),
    Rename(PathBuf),
    Trash(PathBuf),
    Bookmark(PathBuf),
    /// A dragged note, attachment or folder let go over a folder ("" is the vault root).
    MoveInto {
        source: PathBuf,
        folder: PathBuf,
    },
}

pub(crate) fn creation_menu(ui: &mut egui::Ui, folder: &Path, action: &mut Option<FileAction>) {
    let note = ui.button("New note");
    crate::ui_probe::record(
        ui.ctx(),
        format!("folder-new-note:{}", folder.display()),
        note.rect,
    );
    if note.clicked() {
        *action = Some(FileAction::NewNote(folder.to_path_buf()));
        ui.close();
    }
    let child = ui.button("New folder");
    crate::ui_probe::record(
        ui.ctx(),
        format!("folder-new-folder:{}", folder.display()),
        child.rect,
    );
    if child.clicked() {
        *action = Some(FileAction::NewFolder(folder.to_path_buf()));
        ui.close();
    }
}

// One focus owner survives virtualization and note replacement in every sidebar list.
pub fn note_list_id() -> egui::Id {
    egui::Id::unique("sidebar-note-navigation")
}

/// A request to scroll the note lists to a note: the active note changed, or the
/// user moved through a list with the keyboard. It lapses after a second and the
/// user's own scrolling cancels it, so it never pulls the tree back later.
#[derive(Clone)]
struct Reveal {
    path: PathBuf,
    until: f64,
}

fn reveal_id() -> egui::Id {
    note_list_id().with("reveal")
}

pub fn reveal(ctx: &egui::Context, path: &Path) {
    let until = ctx.input(|input| input.time) + 1.0;
    ctx.data_mut(|data| {
        data.insert_temp(
            reveal_id(),
            Reveal {
                path: path.to_owned(),
                until,
            },
        )
    });
}

pub fn cancel_reveal(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove::<Reveal>(reveal_id()));
}

pub fn pending_reveal(ctx: &egui::Context) -> Option<PathBuf> {
    let reveal = ctx.data(|data| data.get_temp::<Reveal>(reveal_id()))?;
    if ctx.input(|input| input.time) > reveal.until {
        cancel_reveal(ctx);
        return None;
    }
    Some(reveal.path)
}

pub fn reveal_note_row(ui: &egui::Ui, path: &Path, rect: egui::Rect) {
    if pending_reveal(ui.ctx()).as_deref() != Some(path) {
        return;
    }
    if rect.top() >= ui.clip_rect().top() && rect.bottom() <= ui.clip_rect().bottom() {
        cancel_reveal(ui.ctx());
    } else {
        ui.scroll_to_rect(rect, Some(egui::Align::Center));
        ui.ctx().request_repaint();
    }
}

pub fn note_list_key(ui: &egui::Ui) -> Option<egui::Key> {
    let id = note_list_id();
    if !ui.memory(|memory| memory.has_focus(id)) || egui::Popup::is_any_open(ui.ctx()) {
        return None;
    }
    ui.memory_mut(|memory| {
        memory.set_focus_lock_filter(
            id,
            egui::EventFilter {
                vertical_arrows: true,
                ..Default::default()
            },
        )
    });
    crate::ui_probe::record(ui.ctx(), "sidebar-list-focused", ui.max_rect());
    [
        egui::Key::ArrowUp,
        egui::Key::ArrowDown,
        egui::Key::Home,
        egui::Key::End,
        egui::Key::Enter,
    ]
    .into_iter()
    .find(|key| ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, *key)))
}

/// The explorer row being dragged, carried by egui's drag-and-drop payload.
struct Dragged {
    paths: Vec<PathBuf>,
    path: PathBuf,
    title: String,
}

/// The folder a drag rests on and since when; a collapsed one opens after a pause.
#[derive(Clone)]
struct DropHover {
    folder: PathBuf,
    since: f64,
}

/// How long a drag rests on a collapsed folder before it opens (Obsidian: 750 ms).
const EXPAND_AFTER: f64 = 0.75;

fn drop_hover_id() -> egui::Id {
    note_list_id().with("drop-hover")
}

/// A row starts dragging once the pressed pointer moves a few pixels.
fn start_drag(response: &egui::Response, path: &Path, title: &str) {
    if response
        .interact(egui::Sense::drag())
        .drag_started_by(egui::PointerButton::Primary)
    {
        let paths = if selection::contains(&response.ctx, path) {
            selection::roots(&response.ctx)
        } else {
            vec![path.to_owned()]
        };
        let title = if paths.len() > 1 {
            format!("{} items", paths.len())
        } else {
            title.to_owned()
        };
        egui::DragAndDrop::set_payload(
            &response.ctx,
            Dragged {
                paths,
                path: path.to_owned(),
                title,
            },
        );
    }
}

/// The drop target's highlight: the row frame in the selection colour, behind its rows.
fn paint_drop_target(ui: &egui::Ui, slot: egui::layers::ShapeIdx, rect: egui::Rect, folder: &Path) {
    let mut frame = rect.shrink2(egui::vec2(0.0, 1.0));
    frame.max.x -= 6.0;
    ui.painter().set(
        slot,
        egui::Shape::rect_filled(frame, 7, theme::interactive(ui).gamma_multiply(0.16)),
    );
    let name = if folder.as_os_str().is_empty() {
        "/".to_owned()
    } else {
        folder.display().to_string()
    };
    crate::ui_probe::record(ui.ctx(), format!("drop-target:{name}"), frame);
}

/// The dragged row follows the pointer in its selected look, framed like a popup.
fn paint_ghost(ui: &egui::Ui, dragged: &Dragged, pointer: egui::Pos2) {
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        drop_hover_id().with("ghost"),
    ));
    let width = painter
        .layout_no_wrap(
            dragged.title.clone(),
            FontId::new(13.0, egui::FontFamily::Name("semibold".into())),
            ui.visuals().text_color(),
        )
        .size()
        .x;
    let rect = egui::Rect::from_min_size(
        pointer + egui::vec2(12.0, 12.0),
        egui::vec2((width + 20.0).min(300.0), 29.0),
    );
    let visuals = ui.visuals();
    painter.add(visuals.popup_shadow.as_shape(rect, 7));
    painter.rect_filled(rect, 7, visuals.window_fill());
    paint_row(ui, &painter, rect, &dragged.title, true, false);
    painter.rect_stroke(rect, 7, visuals.window_stroke(), egui::StrokeKind::Inside);
    crate::ui_probe::record(ui.ctx(), format!("drag:{}", dragged.path.display()), rect);
}

/// While a row is dragged, a pointer near the tree's top or bottom edge scrolls it.
pub fn drag_scroll(ui: &egui::Ui) {
    if !egui::DragAndDrop::has_payload_of_type::<Dragged>(ui.ctx()) {
        return;
    }
    let clip = ui.clip_rect();
    let Some(pointer) = ui
        .input(|input| input.pointer.latest_pos())
        .filter(|pointer| (clip.left()..=clip.right()).contains(&pointer.x))
    else {
        return;
    };
    let direction = if (clip.top()..clip.top() + 20.0).contains(&pointer.y) {
        1.0
    } else if (clip.bottom() - 20.0..=clip.bottom()).contains(&pointer.y) {
        -1.0
    } else {
        return;
    };
    ui.scroll_with_delta(egui::vec2(0.0, direction * 8.0));
    ui.ctx().request_repaint();
}

/// Ends each frame of an explorer drag. The deepest folder under the pointer takes the
/// drop, else the vault root anywhere over `list`; a folder never takes itself or its
/// own subfolders. The root's highlight goes into `root_slot`, the dragged row follows
/// the pointer, and a release returns the move. Escape (egui's drag and drop) or a
/// release anywhere else cancels, as does a release over the item's own folder.
pub fn finish_drag(
    ui: &egui::Ui,
    folder: Option<PathBuf>,
    list: egui::Rect,
    root_slot: egui::layers::ShapeIdx,
) -> Option<FileAction> {
    let ctx = ui.ctx();
    let Some(dragged) = egui::DragAndDrop::payload::<Dragged>(ctx) else {
        ctx.data_mut(|data| data.remove::<DropHover>(drop_hover_id()));
        return None;
    };
    let target = folder
        .or_else(|| ui.rect_contains_pointer(list).then(PathBuf::new))
        .filter(|folder| !dragged.paths.iter().any(|path| folder.starts_with(path)));
    let now = ctx.input(|input| input.time);
    match &target {
        Some(folder) => {
            let since = ctx.data_mut(|data| {
                let hover = data.get_temp_mut_or_insert_with(drop_hover_id(), || DropHover {
                    folder: folder.clone(),
                    since: now,
                });
                if hover.folder != *folder {
                    *hover = DropHover {
                        folder: folder.clone(),
                        since: now,
                    };
                }
                hover.since
            });
            if folder.as_os_str().is_empty() {
                paint_drop_target(ui, root_slot, list, folder);
            } else if now - since < EXPAND_AFTER {
                ctx.request_repaint_after_secs((EXPAND_AFTER - (now - since)) as f32);
            }
        }
        None => ctx.data_mut(|data| data.remove::<DropHover>(drop_hover_id())),
    }
    if let Some(pointer) = ctx.input(|input| input.pointer.latest_pos()) {
        paint_ghost(ui, &dragged, pointer);
    }
    if !ctx.input(|input| input.pointer.any_released()) {
        return None;
    }
    egui::DragAndDrop::clear_payload(ctx);
    ctx.data_mut(|data| data.remove::<DropHover>(drop_hover_id()));
    let folder = target?;
    if dragged.path.parent() == Some(folder.as_path()) {
        return None;
    }
    if dragged.paths.len() > 1 {
        Some(FileAction::MoveManyInto {
            sources: dragged.paths.clone(),
            folder,
        })
    } else {
        Some(FileAction::MoveInto {
            source: dragged.path.clone(),
            folder,
        })
    }
}

#[derive(Default)]
pub struct Tree {
    folders: BTreeMap<String, Tree>,
    files: Vec<PathBuf>,
}

pub fn file_row(ui: &mut egui::Ui, path: &Path, title: &str, selected: bool) -> egui::Response {
    let (_, outer) = ui.allocate_space(egui::vec2(ui.available_width(), 31.0));
    let mut rect = outer.shrink2(egui::vec2(0.0, 1.0));
    rect.max.x -= 6.0;
    let response = ui.interact(rect, ui.make_persistent_id(path), egui::Sense::click());
    paint_row(ui, ui.painter(), rect, title, selected, response.hovered());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), title));
    response
}

/// An explorer row's frame and title; the dragged row's ghost reuses it.
fn paint_row(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    title: &str,
    selected: bool,
    hovered: bool,
) {
    if selected || hovered {
        let fill = if hovered {
            theme::hover_fill(ui)
        } else if selected {
            theme::interactive(ui).gamma_multiply(0.16)
        } else {
            theme::hover_fill(ui)
        };
        painter.rect_filled(rect, 7, fill);
    }
    let color = if selected || hovered {
        ui.visuals().text_color()
    } else {
        ui.visuals().weak_text_color()
    };
    let title_text = RichText::new(title).size(13.0).color(color);
    let text = egui::WidgetText::from(if selected {
        title_text.font(FontId::new(13.0, egui::FontFamily::Name("semibold".into())))
    } else {
        title_text
    });
    let galley = text.into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (rect.width() - 18.0).max(1.0),
        FontId::proportional(13.0),
    );
    let position = egui::pos2(rect.left() + 9.0, rect.center().y - galley.size().y * 0.5);
    painter.galley(position, galley, color);
    if selected {
        painter.line_segment(
            [
                rect.left_top() + egui::vec2(1.5, 3.0),
                rect.left_bottom() + egui::vec2(1.5, -3.0),
            ],
            Stroke::new(3.0, theme::interactive(ui)),
        );
    }
}

impl Tree {
    pub fn from_vault(vault: &Vault) -> Self {
        let mut tree = Self::default();
        for folder in &vault.folders {
            let mut branch = &mut tree;
            for part in folder.components() {
                branch = branch
                    .folders
                    .entry(part.as_os_str().to_string_lossy().into_owned())
                    .or_default();
            }
        }
        for path in vault
            .notes
            .iter()
            .map(|note| &note.path)
            .chain(vault.attachments.iter())
        {
            let mut branch = &mut tree;
            if let Some(parent) = path.parent() {
                for part in parent.components() {
                    branch = branch
                        .folders
                        .entry(part.as_os_str().to_string_lossy().into_owned())
                        .or_default();
                }
            }
            branch.files.push(path.clone());
        }
        tree
    }

    #[allow(clippy::too_many_arguments)]
    pub fn show(
        &self,
        ui: &mut egui::Ui,
        prefix: &str,
        active: Option<&Path>,
        reverse: bool,
        collapse_epoch: u64,
        reveal_folder: Option<&Path>,
        visible_files: &mut Option<Vec<PathBuf>>,
        drop_folder: &mut Option<PathBuf>,
    ) -> (Option<FileAction>, bool) {
        let root = prefix.is_empty();
        if root {
            selection::begin(ui.ctx(), active);
        }
        let mut open = None;
        let mut revealed = false;
        // During a drag the deepest folder under the pointer is the drop target, and the
        // one the drag rested on last frame keeps its name lit and opens after a pause.
        let dragging = egui::DragAndDrop::payload::<Dragged>(ui.ctx());
        let resting = dragging.as_ref().and_then(|_| {
            ui.ctx()
                .data(|data| data.get_temp::<DropHover>(drop_hover_id()))
        });
        let now = ui.input(|input| input.time);
        let folders: Box<dyn Iterator<Item = (&String, &Tree)>> = if reverse {
            Box::new(self.folders.iter().rev())
        } else {
            Box::new(self.folders.iter())
        };
        for (name, folder) in folders {
            let id = format!("{prefix}/{name}");
            let entry = Path::new(id.trim_start_matches('/'));
            let persistent = ui.make_persistent_id(&id);
            let selection_len = selection::record(ui.ctx(), [entry.to_owned()]);
            let (selection_changed, collapse) = ui.ctx().data_mut(|data| {
                let key = persistent.with("collapse");
                let collapse = data
                    .get_temp::<u64>(key)
                    .is_some_and(|previous| previous != collapse_epoch);
                data.insert_temp(key, collapse_epoch);
                let previous =
                    data.get_temp_mut_or_default::<Option<PathBuf>>(persistent.with("active"));
                let changed = previous.as_deref() != active;
                *previous = active.map(Path::to_owned);
                (changed, collapse)
            });
            let resting_since = resting
                .as_ref()
                .filter(|resting| resting.folder.as_path() == entry)
                .map(|resting| resting.since);
            let reveal =
                if reveal_folder.is_some_and(|path| path.starts_with(id.trim_start_matches('/'))) {
                    Some(true)
                } else if collapse {
                    Some(false)
                } else if resting_since.is_some_and(|since| now - since >= EXPAND_AFTER) {
                    Some(true)
                } else {
                    (selection_changed
                        && active.is_some_and(|path| path.starts_with(id.trim_start_matches('/'))))
                    .then_some(true)
                };
            let framed = ui.visuals().collapsing_header_frame;
            let idle_fill = ui.visuals().widgets.inactive.bg_fill;
            let idle_stroke = ui.visuals().widgets.inactive.bg_stroke;
            let pressed_stroke = ui.visuals().widgets.active.bg_stroke;
            ui.visuals_mut().collapsing_header_frame = true;
            ui.visuals_mut().widgets.inactive.bg_fill = if selection::contains(ui.ctx(), entry) {
                ui.visuals().selection.bg_fill
            } else {
                Color32::TRANSPARENT
            };
            ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
            if dragging.is_some() {
                // The dragged folder looks pressed, without the keyboard-focus outline.
                ui.visuals_mut().widgets.active.bg_stroke = Stroke::NONE;
            }
            let hovered = ui.is_enabled()
                && (resting_since.is_some()
                    || ui.rect_contains_pointer(egui::Rect::from_min_size(
                        ui.cursor().min,
                        egui::vec2(ui.available_width(), 31.0),
                    )));
            let first_child = visible_files.as_ref().map_or(0, Vec::len);
            let highlight = dragging
                .is_some()
                .then(|| ui.painter().add(egui::Shape::Noop));
            let was_open = reveal.unwrap_or_else(|| {
                egui::collapsing_header::CollapsingState::load(ui.ctx(), persistent)
                    .is_some_and(|state| state.is_open())
            });
            let header = egui::CollapsingHeader::new(
                RichText::new(name)
                    .size(13.0)
                    .line_height(Some(16.9))
                    .color(if hovered {
                        ui.visuals().text_color()
                    } else {
                        ui.visuals().weak_text_color()
                    }),
            )
            .id_salt(&id)
            .open(reveal)
            .icon(theme::disclosure)
            .default_open(false)
            .show(ui, |ui| {
                let (action, visible) = folder.show(
                    ui,
                    &id,
                    active,
                    reverse,
                    collapse_epoch,
                    reveal_folder,
                    visible_files,
                    drop_folder,
                );
                if action.is_some() {
                    open = action;
                }
                revealed |= visible;
            });
            if selection::row(ui, &header.header_response, entry) {
                if let Some(mut state) =
                    egui::collapsing_header::CollapsingState::load(ui.ctx(), persistent)
                {
                    state.set_open(was_open);
                    state.store(ui.ctx());
                    ui.ctx().request_repaint();
                }
            }
            start_drag(&header.header_response, entry, name);
            if let (Some(source), Some(slot)) = (&dragging, highlight) {
                let block = header
                    .body_response
                    .as_ref()
                    .map_or(header.header_response.rect, |body| {
                        header.header_response.rect.union(body.rect)
                    });
                if drop_folder.is_none() && ui.rect_contains_pointer(block) {
                    *drop_folder = Some(entry.to_owned());
                    if !entry.starts_with(&source.path) {
                        paint_drop_target(ui, slot, block, entry);
                    }
                }
            }
            if egui::collapsing_header::CollapsingState::load(ui.ctx(), persistent)
                .is_some_and(|state| !state.is_open())
                && let Some(paths) = visible_files.as_mut()
            {
                paths.truncate(first_child);
            }
            if egui::collapsing_header::CollapsingState::load(ui.ctx(), persistent)
                .is_some_and(|state| !state.is_open())
            {
                selection::truncate(ui.ctx(), selection_len);
            }
            ui.visuals_mut().collapsing_header_frame = framed;
            ui.visuals_mut().widgets.inactive.bg_fill = idle_fill;
            ui.visuals_mut().widgets.inactive.bg_stroke = idle_stroke;
            ui.visuals_mut().widgets.active.bg_stroke = pressed_stroke;
            header.header_response.context_menu(|ui| {
                theme::menu_style(ui);
                if selection::menu(ui, &mut open) {
                    return;
                }
                let path = PathBuf::from(id.trim_start_matches('/'));
                creation_menu(ui, &path, &mut open);
                ui.separator();
                let response = ui.button("Rename or move…");
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("folder-rename:{}", path.display()),
                    response.rect,
                );
                if response.clicked() {
                    open = Some(FileAction::Rename(path.clone()));
                    ui.close();
                }
                let (delete, activated) = theme::delete_menu_item(ui, "Delete");
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("folder-delete:{}", path.display()),
                    delete.rect,
                );
                if activated {
                    open = Some(FileAction::Trash(path));
                    ui.close();
                }
            });
            if reveal_folder == Some(Path::new(id.trim_start_matches('/'))) {
                let rect = header.header_response.rect;
                if rect.top() >= ui.clip_rect().top() && rect.bottom() <= ui.clip_rect().bottom() {
                    revealed = true;
                } else {
                    ui.scroll_to_rect(rect, Some(egui::Align::Center));
                    ui.ctx().request_repaint();
                }
            }
            crate::ui_probe::record(
                ui.ctx(),
                format!("folder:{}", id.trim_start_matches('/')),
                header.header_response.rect,
            );
        }
        if reverse {
            selection::record(ui.ctx(), self.files.iter().rev().cloned());
        } else {
            selection::record(ui.ctx(), self.files.iter().cloned());
        }
        let row_height = 31.0 + ui.spacing().item_spacing.y;
        let top = ui.cursor().top();
        let first = (((ui.clip_rect().top() - top) / row_height).floor().max(0.0) as usize)
            .min(self.files.len())
            .saturating_sub(1);
        let last = ((((ui.clip_rect().bottom() - top) / row_height)
            .ceil()
            .max(0.0) as usize)
            .saturating_add(1))
        .clamp(first, self.files.len());
        if let Some(paths) = visible_files.as_mut() {
            if reverse {
                paths.extend(self.files.iter().rev().cloned());
            } else {
                paths.extend(self.files.iter().cloned());
            }
        }
        if let Some(target) = pending_reveal(ui.ctx())
            && let Some(index) = self.files.iter().position(|path| *path == target)
        {
            let slot = if reverse {
                self.files.len() - 1 - index
            } else {
                index
            };
            reveal_note_row(
                ui,
                &target,
                egui::Rect::from_min_size(
                    egui::pos2(ui.cursor().left(), top + slot as f32 * row_height),
                    egui::vec2(ui.available_width(), 31.0),
                ),
            );
        }
        ui.add_space(first as f32 * row_height);
        for slot in first..last {
            let index = if reverse {
                self.files.len() - 1 - slot
            } else {
                slot
            };
            let path = &self.files[index];
            let title = rustidian::vault::file_title(path);
            let selected = selection::contains(ui.ctx(), path);
            let response = file_row(ui, path, &title, selected);
            let selection_click = selection::row(ui, &response, path);
            start_drag(&response, path, &title);
            crate::ui_probe::record(ui.ctx(), format!("file:{}", path.display()), response.rect);
            let response = response.on_hover_text(path.display().to_string());
            response.context_menu(|ui| {
                theme::menu_style(ui);
                if selection::menu(ui, &mut open) {
                    return;
                }
                if theme::menu_item(ui, theme::Icon::NewTab, "Open in new tab").clicked() {
                    open = Some(FileAction::OpenInNewTab(path.clone()));
                    ui.close();
                }
                ui.separator();
                if theme::menu_item(ui, theme::Icon::Copy, "Make a copy").clicked() {
                    open = Some(FileAction::Copy(path.clone()));
                    ui.close();
                }
                if theme::menu_item(ui, theme::Icon::Files, "Move file to...").clicked() {
                    open = Some(FileAction::Move(path.clone()));
                    ui.close();
                }
                let bookmark = theme::menu_item(ui, theme::Icon::Bookmark, "Bookmark…");
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("file-bookmark:{}", path.display()),
                    bookmark.rect,
                );
                if bookmark.clicked() {
                    open = Some(FileAction::Bookmark(path.clone()));
                    ui.close();
                }
                if theme::menu_item(ui, theme::Icon::Copy, "Copy path").clicked() {
                    open = Some(FileAction::CopyPath(path.clone()));
                    ui.close();
                }
                ui.separator();
                if theme::menu_item(ui, theme::Icon::Edit, "Rename…").clicked() {
                    open = Some(FileAction::Rename(path.clone()));
                    ui.close();
                }
                if theme::delete_menu_item(ui, "Delete").1 {
                    open = Some(FileAction::Trash(path.clone()));
                    ui.close();
                }
            });
            if response.clicked() && !selection_click {
                open = Some(FileAction::Open(path.clone()));
            }
        }
        ui.add_space((self.files.len() - last) as f32 * row_height);
        if root {
            if let Some(action) = selection::finish(ui) {
                open = Some(action);
            }
        }
        (open, revealed)
    }
}

#[cfg(test)]
mod tests {
    use eframe::egui;
    use std::path::Path;

    #[test]
    fn a_reveal_lapses_after_a_second_and_cancelling_drops_it() {
        let ctx = egui::Context::default();
        let at = |time: f64| {
            ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |_| {},
            )
            .drop_without_applying_deltas();
        };
        let note = Path::new("Folder/Note.md");
        at(1.0);
        super::reveal(&ctx, note);
        at(1.9);
        assert_eq!(super::pending_reveal(&ctx).as_deref(), Some(note));
        at(2.1);
        assert_eq!(super::pending_reveal(&ctx), None);
        super::reveal(&ctx, note);
        super::cancel_reveal(&ctx);
        assert_eq!(super::pending_reveal(&ctx), None);
    }
}
