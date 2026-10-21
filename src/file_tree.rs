use crate::theme;
use eframe::egui::{self, Color32, FontId, RichText, Stroke};
use rust_notes::vault::Vault;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

pub enum FileAction {
    Open(PathBuf),
    Rename(PathBuf),
    Trash(PathBuf),
    Bookmark(PathBuf),
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
    if selected || response.hovered() {
        let fill = if response.hovered() {
            theme::hover_fill(ui)
        } else if selected {
            Color32::from_rgba_unmultiplied(23, 127, 131, 40)
        } else {
            theme::hover_fill(ui)
        };
        ui.painter().rect_filled(rect, 6, fill);
    }
    let color = if selected || response.hovered() {
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
    ui.painter().galley(position, galley, color);
    response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), title));
    if selected {
        ui.painter().line_segment(
            [
                response.rect.left_top() + egui::vec2(1.5, 3.0),
                response.rect.left_bottom() + egui::vec2(1.5, -3.0),
            ],
            Stroke::new(3.0, theme::interactive(ui)),
        );
    }
    response
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

    pub fn show(
        &self,
        ui: &mut egui::Ui,
        prefix: &str,
        active: Option<&Path>,
        reverse: bool,
        collapse_epoch: u64,
        reveal_folder: Option<&Path>,
    ) -> (Option<FileAction>, bool) {
        let mut open = None;
        let mut revealed = false;
        let folders: Box<dyn Iterator<Item = (&String, &Tree)>> = if reverse {
            Box::new(self.folders.iter().rev())
        } else {
            Box::new(self.folders.iter())
        };
        for (name, folder) in folders {
            let id = format!("{prefix}/{name}");
            let persistent = ui.make_persistent_id(&id);
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
            let reveal =
                if reveal_folder.is_some_and(|path| path.starts_with(id.trim_start_matches('/'))) {
                    Some(true)
                } else if collapse {
                    Some(false)
                } else {
                    (selection_changed
                        && active.is_some_and(|path| path.starts_with(id.trim_start_matches('/'))))
                    .then_some(true)
                };
            let framed = ui.visuals().collapsing_header_frame;
            let idle_fill = ui.visuals().widgets.inactive.bg_fill;
            let idle_stroke = ui.visuals().widgets.inactive.bg_stroke;
            ui.visuals_mut().collapsing_header_frame = true;
            ui.visuals_mut().widgets.inactive.bg_fill = Color32::TRANSPARENT;
            ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::NONE;
            let header = egui::CollapsingHeader::new(
                RichText::new(name)
                    .size(13.0)
                    .line_height(Some(16.9))
                    .color(ui.visuals().weak_text_color()),
            )
            .id_salt(&id)
            .open(reveal)
            .icon(theme::disclosure)
            .default_open(false)
            .show(ui, |ui| {
                let (action, visible) =
                    folder.show(ui, &id, active, reverse, collapse_epoch, reveal_folder);
                if action.is_some() {
                    open = action;
                }
                revealed |= visible;
            });
            ui.visuals_mut().collapsing_header_frame = framed;
            ui.visuals_mut().widgets.inactive.bg_fill = idle_fill;
            ui.visuals_mut().widgets.inactive.bg_stroke = idle_stroke;
            header.header_response.context_menu(|ui| {
                let path = PathBuf::from(id.trim_start_matches('/'));
                let response = ui.button("Rename or move…");
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("folder-rename:{}", path.display()),
                    response.rect,
                );
                if response.clicked() {
                    open = Some(FileAction::Rename(path));
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
        ui.add_space(first as f32 * row_height);
        for slot in first..last {
            let index = if reverse {
                self.files.len() - 1 - slot
            } else {
                slot
            };
            let path = &self.files[index];
            let title = rust_notes::vault::file_title(path);
            let selected = active == Some(path.as_path());
            let response = file_row(ui, path, &title, selected);
            crate::ui_probe::record(ui.ctx(), format!("file:{}", path.display()), response.rect);
            let response = response.on_hover_text(path.display().to_string());
            response.context_menu(|ui| {
                let bookmark = ui.button("Bookmark");
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("file-bookmark:{}", path.display()),
                    bookmark.rect,
                );
                if bookmark.clicked() {
                    open = Some(FileAction::Bookmark(path.clone()));
                    ui.close();
                }
                if ui.button("Rename or move…").clicked() {
                    open = Some(FileAction::Rename(path.clone()));
                    ui.close();
                }
                if ui.button("Move to system Trash…").clicked() {
                    open = Some(FileAction::Trash(path.clone()));
                    ui.close();
                }
            });
            if response.clicked() {
                open = Some(FileAction::Open(path.clone()));
            }
        }
        ui.add_space((self.files.len() - last) as f32 * row_height);
        (open, revealed)
    }
}
