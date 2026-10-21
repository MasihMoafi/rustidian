use eframe::egui::{
    self, FontId, Key, RichText, TextEdit,
    text::{CCursor, CCursorRange},
};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Title {
    owner: PathBuf,
    draft: String,
    editing: bool,
    focus: bool,
}

impl Title {
    pub fn show(&mut self, ui: &mut egui::Ui, path: &Path, title: &str) -> Option<String> {
        if self.owner != path {
            *self = Self {
                owner: path.to_owned(),
                ..Self::default()
            };
        }
        let font = FontId::new(32.436, egui::FontFamily::Name("bold".into()));
        if !self.editing {
            let response = ui.add(
                egui::Label::new(
                    RichText::new(title)
                        .font(font)
                        .color(crate::theme::heading(ui, 1))
                        .line_height(Some(38.9232)),
                )
                .selectable(false)
                .sense(egui::Sense::click()),
            );
            crate::ui_probe::record(ui.ctx(), "note-title", response.rect);
            if response.clicked() {
                self.draft = title.to_owned();
                self.editing = true;
                self.focus = true;
                ui.ctx().request_repaint();
            }
            response
                .on_hover_cursor(egui::CursorIcon::Text)
                .on_hover_text("Rename note");
            return None;
        }
        let id = ui.make_persistent_id(("note-title-input", path));
        let starting = self.focus;
        if starting {
            ui.memory_mut(|memory| memory.request_focus(id));
        }
        let mut output = TextEdit::singleline(&mut self.draft)
            .id(id)
            .font(font)
            .frame(egui::Frame::NONE)
            .margin(egui::Vec2::ZERO)
            .desired_width(f32::INFINITY)
            .show(ui);
        crate::ui_probe::record(ui.ctx(), "note-title-input", output.response.rect);
        if starting && output.response.has_focus() {
            output.state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(0),
                CCursor::new(self.draft.chars().count()),
            )));
            output.state.store(ui.ctx(), id);
            self.focus = false;
        }
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            self.editing = false;
            output.response.surrender_focus();
            return None;
        }
        let enter = output.response.has_focus()
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Enter));
        if enter || (!starting && output.response.lost_focus()) {
            self.editing = false;
            output.response.surrender_focus();
            return (self.draft.trim() != title).then(|| self.draft.trim().to_owned());
        }
        None
    }

    pub fn reject(&mut self) {
        self.editing = true;
        self.focus = true;
    }
}
