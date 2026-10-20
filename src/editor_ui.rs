use eframe::egui::{
    self, Color32, FontFamily, FontId, Key, RichText, Stroke, TextEdit,
    text::{CCursor, CCursorRange, LayoutJob},
    widgets::text_edit::TextEditState,
};
use rust_notes::{
    editor::{Markdown, complete_link, find_matches, link_at, link_completion, surround},
    search::FileMatcher,
    tab::Tab,
};
use std::{collections::HashSet, ops::Range, sync::Arc};

#[derive(Default)]
pub struct Find {
    pub open: bool,
    pub focus: bool,
    query: String,
    replace: String,
    replacing: bool,
    selected: usize,
    restore: Option<Range<usize>>,
}

#[derive(Default)]
pub struct Editor {
    pub jump: Option<usize>,
    pub focus: bool,
    source: String,
    parsed: Markdown,
    sections: rust_notes::folding::Sections,
    folded_layout: Option<FoldedLayout>,
    tables: crate::table_ui::Tables,
    completion_key: String,
    completion_selected: usize,
    completion_dismissed: bool,
    focus_range: Option<(egui::Id, CCursorRange)>,
    image_cursor: Option<(usize, usize)>,
    image_cursor_time: f64,
}

struct LiveImage {
    range: Range<usize>,
    uri: String,
    size: egui::Vec2,
}

// Keep the original allocation alive so its pointer identifies the font
// layout safely. One cached value bounds memory while avoiding row/mesh
// cloning again on idle frames and repeated layout passes during typing.
struct FoldedLayout {
    original: Arc<egui::Galley>,
    live: bool,
    images: Vec<(Range<usize>, [u32; 2], bool)>,
    hidden: Vec<Range<usize>>,
    tables: Vec<crate::table_ui::Placement>,
    galley: Arc<egui::Galley>,
}

struct Decorations<'a> {
    live: bool,
    active_line: Option<&'a Range<usize>>,
    matched: Option<&'a Range<usize>>,
    images: &'a [LiveImage],
}

/// Index the final wrapped rows once; decoration lookups must not rescan the galley.
struct PositionedText<'a> {
    output: &'a egui::text_edit::TextEditOutput,
    row_ends: Vec<usize>,
}

impl<'a> PositionedText<'a> {
    fn new(output: &'a egui::text_edit::TextEditOutput) -> Self {
        let mut character = 0;
        let row_ends = output
            .galley
            .rows
            .iter()
            .map(|row| {
                character += row.char_count_excluding_newline().0;
                let end = character;
                character += usize::from(row.ends_with_newline);
                end
            })
            .collect();
        Self { output, row_ends }
    }

    // Decoration positions prefer the preceding row at a soft-wrap boundary,
    // exactly as CCursor::new does. Interactive caret affinity stays in egui.
    fn at(&self, character: usize) -> egui::Rect {
        let row = self
            .row_ends
            .partition_point(|&end| end < character)
            .min(self.row_ends.len().saturating_sub(1));
        let Some(placed) = self.output.galley.rows.get(row) else {
            return egui::Rect::ZERO;
        };
        let start = if row == 0 {
            0
        } else {
            self.row_ends[row - 1] + usize::from(self.output.galley.rows[row - 1].ends_with_newline)
        };
        let x = placed.pos.x + placed.x_offset((character - start).into());
        egui::Rect::from_min_max(egui::pos2(x, placed.min_y()), egui::pos2(x, placed.max_y()))
    }
}

fn live_images(
    ui: &egui::Ui,
    text: &str,
    vault: Option<&rust_notes::vault::Vault>,
    from: &std::path::Path,
) -> Vec<LiveImage> {
    if !text.contains("![") {
        return Vec::new();
    }
    let width_limit = (ui.available_width() - 4.0).max(1.0);
    rust_notes::assets::embedded_images(text)
        .into_iter()
        .filter_map(|image| {
            if text[image.range.clone()].contains('\n') {
                return None;
            }
            let uri = rust_notes::assets::image_uri(vault, from, &image.target)?;
            let natural = match ui
                .ctx()
                .try_load_image(&uri, egui::load::SizeHint::Width(width_limit.ceil() as u32))
            {
                Ok(egui::load::ImagePoll::Ready { image }) => {
                    egui::vec2(image.size[0] as f32, image.size[1] as f32)
                }
                Ok(egui::load::ImagePoll::Pending { size: Some(size) }) => size,
                _ => egui::vec2(image.width.unwrap_or(180.0), image.height.unwrap_or(120.0)),
            };
            let width = image.width.unwrap_or(natural.x).min(width_limit);
            let height = image
                .height
                .unwrap_or(natural.y * width / natural.x.max(1.0));
            Some(LiveImage {
                range: image.range,
                uri,
                size: egui::vec2(width, height),
            })
        })
        .collect()
}

pub fn find_bar(ui: &mut egui::Ui, find: &mut Find, tab: &mut Tab, id: egui::Id) {
    if !find.open {
        return;
    }
    let matches = find_matches(&tab.document.text, &find.query);
    find.selected = find.selected.min(matches.len().saturating_sub(1));
    let mut select = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Find").size(13.0));
        let response = ui.add(
            TextEdit::singleline(&mut find.query)
                .id_salt("find-note")
                .desired_width(220.0)
                .font(FontId::proportional(14.0)),
        );
        if find.focus {
            response.request_focus();
            find.focus = false;
        }
        if response.changed() {
            find.selected = 0;
            select = true;
        }
        ui.label(
            RichText::new(if matches.is_empty() {
                "0 results".into()
            } else {
                format!("{} / {}", find.selected + 1, matches.len())
            })
            .size(12.0),
        );
        if ui.button("Previous").clicked() && !matches.is_empty() {
            find.selected = (find.selected + matches.len() - 1) % matches.len();
            select = true;
        }
        if (ui.button("Next").clicked()
            || response.has_focus()
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter)))
            && !matches.is_empty()
        {
            find.selected = (find.selected + 1) % matches.len();
            select = true;
        }
        ui.toggle_value(&mut find.replacing, "Replace");
        if ui.button("×").clicked()
            || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape))
        {
            find.open = false;
            select = true;
            ui.memory_mut(|m| m.request_focus(id));
        }
    });
    if find.replacing {
        ui.horizontal(|ui| {
            ui.label(RichText::new("With").size(13.0));
            ui.add(
                TextEdit::singleline(&mut find.replace)
                    .desired_width(220.0)
                    .font(FontId::proportional(14.0)),
            );
            if ui
                .add_enabled(!matches.is_empty(), egui::Button::new("Replace"))
                .clicked()
                && let Some(range) = matches.get(find.selected)
            {
                replace_chars(&mut tab.document.text, range.clone(), &find.replace);
                tab.edited();
            }
            if ui
                .add_enabled(!matches.is_empty(), egui::Button::new("Replace all"))
                .clicked()
            {
                for range in matches.iter().rev() {
                    replace_chars(&mut tab.document.text, range.clone(), &find.replace);
                }
                tab.edited();
            }
        });
    }
    if select {
        let matches = find_matches(&tab.document.text, &find.query);
        if let Some(range) = matches.get(find.selected.min(matches.len().saturating_sub(1))) {
            find.restore = Some(range.clone());
            let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
            state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new(range.start),
                CCursor::new(range.end),
            )));
            state.store(ui.ctx(), id);
        }
    }
    ui.separator();
}

fn replace_chars(text: &mut String, range: Range<usize>, replacement: &str) {
    let offsets: Vec<_> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    text.replace_range(offsets[range.start]..offsets[range.end], replacement);
}

// Completion and Markdown Enter run before TextEdit. Apply only the plain text
// preceding their first key; other editing/navigation stays with TextEdit in order.
fn text_before_action(events: &mut Vec<egui::Event>) -> Option<String> {
    let Some(end) = events.iter().position(|event| {
        matches!(
            event,
            egui::Event::Key {
                key: Key::Tab | Key::Enter,
                pressed: true,
                ..
            }
        )
    }) else {
        return Some(String::new());
    };
    if !events[..end].iter().all(|event| match event {
        egui::Event::Text(_) | egui::Event::Key { pressed: false, .. } => true,
        egui::Event::ModifiersChanged(_) => true,
        egui::Event::Key {
            key:
                Key::ShiftLeft
                | Key::ShiftRight
                | Key::ControlLeft
                | Key::ControlRight
                | Key::AltLeft
                | Key::AltRight
                | Key::SuperLeft
                | Key::SuperRight,
            ..
        } => true,
        egui::Event::Key { key, modifiers, .. } => {
            !modifiers.command
                && !modifiers.ctrl
                && !modifiers.alt
                && !matches!(
                    key,
                    Key::ArrowDown
                        | Key::ArrowLeft
                        | Key::ArrowRight
                        | Key::ArrowUp
                        | Key::Home
                        | Key::End
                        | Key::PageUp
                        | Key::PageDown
                        | Key::Backspace
                        | Key::Delete
                        | Key::Insert
                        | Key::Escape
                        | Key::Copy
                        | Key::Cut
                        | Key::Paste
                )
        }
        _ => false,
    }) {
        return None;
    }
    let mut text = String::new();
    let mut index = 0;
    events.retain(|event| {
        let before = index < end;
        index += 1;
        if before && let egui::Event::Text(value) = event {
            if value != "\n" && value != "\r" {
                text.push_str(value);
            }
            false
        } else {
            true
        }
    });
    Some(text)
}

fn action_matches(
    input: &egui::InputState,
    modifiers: egui::Modifiers,
    keys: &[Key],
) -> Option<usize> {
    let (index, key, event_modifiers) =
        input.events.iter().enumerate().find_map(|(index, event)| {
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
    (keys.contains(&key) && event_modifiers.matches_exact(modifiers)).then_some(index)
}

fn consume_action(ui: &egui::Ui, modifiers: egui::Modifiers, keys: &[Key]) -> bool {
    ui.input_mut(|input| {
        if let Some(index) = action_matches(input, modifiers, keys) {
            input.events.remove(index);
            true
        } else {
            false
        }
    })
}

impl Editor {
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut Tab,
        id: egui::Id,
        find: &mut Find,
        files: &mut FileMatcher,
        vault: Option<&rust_notes::vault::Vault>,
    ) -> Option<String> {
        if self.focus {
            // A closing modal remains the top interaction layer for one pass.
            // Keep the request until the editor can register its focus interest.
            if ui.memory(|memory| memory.allows_interaction(ui.layer_id())) {
                ui.memory_mut(|memory| memory.request_focus(id));
                self.focus = false;
            }
            ui.ctx().request_repaint();
        }
        let live = tab.view == rust_notes::tab::View::Live;
        self.tables.begin(ui, tab, id, live);
        let mut follow = None;
        let header_end = if live {
            rust_notes::metadata::frontmatter(&tab.document.text)
                .ok()
                .flatten()
                .map_or(0, |properties| properties.range.end)
        } else {
            0
        };
        let body_start = tab.document.text[..header_end].chars().count();
        if header_end > 0 {
            let header = &tab.document.text[..header_end];
            let changed = ui.ctx().data_mut(|data| {
                let previous = data.get_temp_mut_or_default::<String>(id.with("properties"));
                if previous == header {
                    false
                } else {
                    header.clone_into(previous);
                    true
                }
            });
            if changed && let Some(mut state) = TextEditState::load(ui.ctx(), id) {
                state.clear_undoer();
                state.store(ui.ctx(), id);
            }
        }
        if body_start > 0 && ui.memory(|memory| memory.has_focus(id)) {
            let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
            if let Some(mut cursor) = state.cursor.char_range() {
                cursor.primary.index.0 = cursor.primary.index.0.max(body_start);
                cursor.secondary.index.0 = cursor.secondary.index.0.max(body_start);
                if ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, Key::A)) {
                    cursor = CCursorRange::two(
                        CCursor::new(body_start),
                        CCursor::new(tab.document.text.chars().count()),
                    );
                }
                if cursor.is_empty() && cursor.primary.index.0 == body_start {
                    ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, Key::Backspace));
                }
                state.cursor.set_char_range(Some(cursor));
                state.store(ui.ctx(), id);
            }
        }
        let jump = self.jump.take();
        if let Some(character) = jump {
            set_cursor(ui.ctx(), id, character);
        }
        if let Some((target, range)) = self.focus_range {
            if target != id {
                self.focus_range = None;
            } else {
                ui.memory_mut(|memory| memory.request_focus(id));
                if ui.memory(|memory| memory.owns_ime_events(id)) {
                    set_cursor_range(ui.ctx(), id, range);
                    self.focus_range = None;
                }
                ui.ctx().request_repaint();
            }
        }
        let mut action_handled = false;
        let mut action_ready = true;
        if ui.memory(|m| m.has_focus(id)) {
            let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
            if let Some(mut selection) = state.cursor.char_range() {
                if let Some(text) = ui.input_mut(|input| text_before_action(&mut input.events)) {
                    if !text.is_empty() {
                        let mut undoer = state.undoer();
                        if !selection.is_empty() {
                            undoer.add_undo(&(selection, tab.document.text.clone()));
                        }
                        undoer.feed_state(
                            ui.input(|input| input.time),
                            &(selection, tab.document.text.clone()),
                        );
                        let range = selection.as_sorted_char_range();
                        let caret = range.start.0 + text.chars().count();
                        replace_chars(&mut tab.document.text, range.start.0..range.end.0, &text);
                        selection = CCursorRange::one(CCursor::new(caret));
                        state.cursor.set_char_range(Some(selection));
                        undoer.feed_state(
                            ui.input(|input| input.time),
                            &(selection, tab.document.text.clone()),
                        );
                        state.set_undoer(undoer);
                        state.store(ui.ctx(), id);
                        tab.edited();
                    }
                } else {
                    action_ready = false;
                }
            } else {
                action_ready = false;
            }
        }
        if action_ready && ui.memory(|m| m.has_focus(id)) {
            let cursor = TextEditState::load(ui.ctx(), id)
                .and_then(|s| s.cursor.char_range())
                .map(|c| c.primary.index.0);
            if let Some(cursor) = cursor {
                let completion = link_completion(&tab.document.text, cursor);
                let key = completion
                    .as_ref()
                    .map(|c| {
                        format!(
                            "{}:{}:{}",
                            tab.document.path.display(),
                            c.range.start,
                            c.query
                        )
                    })
                    .unwrap_or_default();
                if self.completion_key != key {
                    self.completion_key = key;
                    self.completion_selected = 0;
                    self.completion_dismissed = false;
                }
                if let Some(completion) = completion
                    && !self.completion_dismissed
                {
                    let candidates = files.rank_links(&completion.query, 8);
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
                        self.completion_dismissed = true;
                    }
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowDown)) {
                        self.completion_selected =
                            (self.completion_selected + 1).min(candidates.len().saturating_sub(1));
                    }
                    if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::ArrowUp)) {
                        self.completion_selected = self.completion_selected.saturating_sub(1);
                    }
                    if let Some(path) = candidates.get(self.completion_selected)
                        && consume_action(ui, egui::Modifiers::NONE, &[Key::Tab, Key::Enter])
                    {
                        let caret = complete_link(
                            &mut tab.document.text,
                            &completion,
                            &rust_notes::paths::wiki_destination(path),
                        );
                        set_cursor(ui.ctx(), id, caret);
                        tab.edited();
                        action_handled = true;
                    }
                } else if consume_action(ui, egui::Modifiers::COMMAND, &[Key::Enter]) {
                    follow = link_at(&tab.document.text, cursor);
                    action_handled = true;
                }
            }
        }
        if action_ready
            && !action_handled
            && ui.memory(|memory| memory.has_focus(id))
            && ui.input(|input| {
                action_matches(input, egui::Modifiers::NONE, &[Key::Enter]).is_some()
            })
        {
            let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
            if let Some(selection) = state.cursor.char_range() {
                let selection = selection.as_sorted_char_range();
                if let Some(range) = rust_notes::editor::markdown_enter(
                    &mut tab.document.text,
                    selection.start.0..selection.end.0,
                ) {
                    consume_action(ui, egui::Modifiers::NONE, &[Key::Enter]);
                    state.cursor.set_char_range(Some(CCursorRange::two(
                        CCursor::new(range.start),
                        CCursor::new(range.end),
                    )));
                    state.store(ui.ctx(), id);
                    tab.edited();
                }
            }
        }
        if !find.open && find.restore.is_some() {
            if ui.memory(|m| m.owns_ime_events(id)) {
                let range = find.restore.take().unwrap();
                let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
                state.cursor.set_char_range(Some(CCursorRange::two(
                    CCursor::new(range.start),
                    CCursor::new(range.end),
                )));
                state.store(ui.ctx(), id);
            } else {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        if ui.memory(|m| m.has_focus(id)) {
            for (key, marker) in [(Key::B, "**"), (Key::I, "*")] {
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, key)) {
                    let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
                    if let Some(selection) = state.cursor.char_range() {
                        let mut undoer = state.undoer();
                        undoer.add_undo(&(selection, tab.document.text.clone()));
                        let range = surround(
                            &mut tab.document.text,
                            selection.as_sorted_char_range().start.0
                                ..selection.as_sorted_char_range().end.0,
                            marker,
                        );
                        let cursor =
                            CCursorRange::two(CCursor::new(range.start), CCursor::new(range.end));
                        state.cursor.set_char_range(Some(cursor));
                        undoer.add_undo(&(cursor, tab.document.text.clone()));
                        state.set_undoer(undoer);
                        state.store(ui.ctx(), id);
                        tab.edited();
                    }
                }
            }
        }
        if self.source != tab.document.text {
            self.parsed = Markdown::parse(&tab.document.text);
            self.sections = rust_notes::folding::Sections::new(&tab.document.text, &self.parsed);
            self.source.clone_from(&tab.document.text);
        }
        let folds_id = id.with("editing-folds");
        let mut collapsed = ui.ctx().data(|data| {
            data.get_temp::<HashSet<String>>(folds_id)
                .unwrap_or_default()
        });
        if let Some(character) = jump {
            self.sections.reveal(character, &mut collapsed);
        }
        if let Some(range) = &find.restore {
            self.sections.reveal(range.start, &mut collapsed);
        }
        let cursor = TextEditState::load(ui.ctx(), id).and_then(|s| s.cursor.char_range());
        if let Some(cursor) = cursor {
            self.sections
                .expand_at(cursor.primary.index.0, &mut collapsed);
        }
        let active_line = if live && ui.memory(|m| m.has_focus(id)) {
            cursor.map(|cursor| {
                let byte = tab
                    .document
                    .text
                    .char_indices()
                    .nth(cursor.primary.index.0)
                    .map_or(tab.document.text.len(), |(i, _)| i);
                let start = tab.document.text[..byte].rfind('\n').map_or(0, |i| i + 1);
                let end = tab.document.text[byte..]
                    .find('\n')
                    .map_or(tab.document.text.len(), |i| byte + i);
                start..end
            })
        } else {
            None
        };
        let selected_match = if find.open {
            find_matches(&tab.document.text, &find.query)
                .get(find.selected)
                .cloned()
        } else {
            None
        };
        let source = &self.source;
        let parsed = &self.parsed;
        let sections = &self.sections;
        let folded_layout = &mut self.folded_layout;
        let tables = &mut self.tables;
        let mut table_placements = Vec::new();
        let from = &tab.document.path;
        let mut images = if live {
            live_images(ui, source, vault, from)
        } else {
            Vec::new()
        };
        retain_visible_images(&mut images, sections, &collapsed);
        let active_image = images.iter().any(|image| {
            active_line
                .as_ref()
                .is_some_and(|line| line.start < image.range.end && line.end >= image.range.start)
        });
        let mut cached_layout: Option<(f32, Arc<egui::Galley>)> = None;
        let mut updated_parsed = None;
        let mut layouter =
            |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, width: f32| -> Arc<egui::Galley> {
                let text = buffer.as_str();
                if text == source && updated_parsed.take().is_some() && live {
                    images = live_images(ui, source, vault, from);
                    retain_visible_images(&mut images, sections, &collapsed);
                }
                if let Some((previous_width, galley)) = &cached_layout
                    && galley.text() == text
                    && (width - previous_width).abs() < 0.1
                {
                    return galley.clone();
                }
                let parsed = if text == source {
                    parsed
                } else {
                    updated_parsed = Some(Markdown::parse(text));
                    if live {
                        images = live_images(ui, text, vault, from);
                    }
                    updated_parsed.as_ref().unwrap()
                };
                let updated_sections;
                let current_sections = if text != source && !collapsed.is_empty() {
                    updated_sections = rust_notes::folding::Sections::new(text, parsed);
                    &updated_sections
                } else {
                    sections
                };
                retain_visible_images(&mut images, current_sections, &collapsed);
                images.retain(|image| {
                    !parsed
                        .tables
                        .iter()
                        .any(|table| table.range.contains(&image.range.start))
                });
                let hidden = current_sections.hidden(&collapsed);
                table_placements = if live {
                    tables.prepare(ui, text, &parsed.tables, active_line.as_ref(), &hidden)
                } else {
                    Vec::new()
                };
                let job = layout(
                    ui,
                    text,
                    parsed,
                    width,
                    Decorations {
                        live,
                        active_line: active_line.as_ref(),
                        matched: selected_match.as_ref(),
                        images: &images,
                    },
                );
                let original = ui.fonts_mut(|fonts| fonts.layout_job(job));
                let decorated = !hidden.is_empty() || !table_placements.is_empty();
                let image_layout: Vec<_> = if !decorated {
                    Vec::new()
                } else {
                    images
                        .iter()
                        .map(|image| {
                            (
                                image.range.clone(),
                                [image.size.x.to_bits(), image.size.y.to_bits()],
                                active_line.as_ref().is_some_and(|line| {
                                    line.start < image.range.end && line.end >= image.range.start
                                }),
                            )
                        })
                        .collect()
                };
                if decorated
                    && let Some(cached) = folded_layout.as_ref()
                    && Arc::ptr_eq(&cached.original, &original)
                    && cached.live == live
                    && cached.images == image_layout
                    && cached.hidden == hidden
                    && cached.tables == table_placements
                {
                    cached_layout = Some((width, cached.galley.clone()));
                    return cached.galley.clone();
                }
                let mut galley = original.clone();
                space_rows(
                    &mut galley,
                    text,
                    parsed,
                    live,
                    &images,
                    active_line.as_ref(),
                );
                reserve_table_rows(&mut galley, &table_placements);
                if !hidden.is_empty() {
                    collapse_rows(&mut galley, &hidden);
                }
                if decorated {
                    *folded_layout = Some(FoldedLayout {
                        original,
                        live,
                        images: image_layout,
                        hidden,
                        tables: table_placements.clone(),
                        galley: galley.clone(),
                    });
                } else {
                    *folded_layout = None;
                }
                cached_layout = Some((width, galley.clone()));
                galley
            };
        let text_height = layouter(ui, &tab.document.text, ui.available_width())
            .size()
            .y;
        let minimum_height = text_height + ui.clip_rect().height() / 2.0;
        let selection = ui.visuals().selection.bg_fill;
        let cursor_style = ui.visuals().text_cursor.clone();
        let code_background = ui.painter().add(egui::Shape::Noop);
        let selection_shape = ui.painter().add(egui::Shape::Noop);
        let bulk_edit = ui.memory(|memory| memory.has_focus(id))
            && ui.input(|input| {
                input.events.iter().any(|event| match event {
                    egui::Event::Paste(text) => !text.is_empty(),
                    egui::Event::Cut => cursor.is_some_and(|range| !range.is_empty()),
                    egui::Event::Key {
                        key: Key::Backspace | Key::Delete,
                        pressed: true,
                        ..
                    } => cursor.is_some_and(|range| !range.is_empty()),
                    _ => false,
                })
            });
        let selection_typing = ui.memory(|memory| memory.has_focus(id))
            && cursor.is_some_and(|range| !range.is_empty())
            && ui.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Text(text) if !text.is_empty()))
            });
        let save_undo = |ctx: &egui::Context, text: &str| {
            if let Some(mut state) = TextEditState::load(ctx, id)
                && let Some(range) = state.cursor.char_range()
            {
                let mut undoer = state.undoer();
                undoer.add_undo(&(range, text.to_owned()));
                state.set_undoer(undoer);
                state.store(ctx, id);
            }
        };
        if bulk_edit || selection_typing {
            save_undo(ui.ctx(), &tab.document.text);
        }
        let output = ui
            .scope(|ui| {
                if active_image {
                    ui.visuals_mut().selection.bg_fill = Color32::TRANSPARENT;
                    ui.visuals_mut().text_cursor.stroke.color = Color32::TRANSPARENT;
                }
                TextEdit::multiline(&mut tab.document.text)
                    .id(id)
                    .desired_width(f32::INFINITY)
                    .desired_rows(1)
                    .min_size(egui::vec2(0.0, minimum_height))
                    .frame(egui::Frame::NONE)
                    .font(FontId::proportional(18.0))
                    .layouter(&mut layouter)
                    .lock_focus(true)
                    .show(ui)
            })
            .inner;
        if bulk_edit && output.response.changed() {
            save_undo(ui.ctx(), &tab.document.text);
        }
        crate::ui_probe::record(ui.ctx(), "editor", output.response.rect);
        if header_end > 0 && !tab.document.text.starts_with(&self.source[..header_end]) {
            tab.document.text.clone_from(&self.source);
            self.focus_range = Some((id, CCursorRange::one(CCursor::new(body_start))));
            ui.ctx().request_repaint();
            images.clear();
            updated_parsed = None;
        }
        if let Some(parsed) = updated_parsed {
            self.sections = rust_notes::folding::Sections::new(&tab.document.text, &parsed);
            self.parsed = parsed;
            self.source.clone_from(&tab.document.text);
        }
        if output.response.changed() {
            tab.edited();
            ui.ctx().request_repaint();
        }
        let positioned = PositionedText::new(&output);
        paint_heading_folds(ui, &positioned, &self.sections, &mut collapsed);
        ui.ctx()
            .data_mut(|data| data.insert_temp(folds_id, collapsed));
        paint_code_blocks(
            ui,
            &positioned,
            &self.source,
            &self.parsed,
            live,
            active_line.as_ref(),
            code_background,
        );
        paint_markers(
            ui,
            &positioned,
            &self.source,
            &self.parsed,
            live,
            active_line.as_ref(),
        );
        for (ordinal, image) in images.iter().enumerate() {
            let active = active_line
                .as_ref()
                .is_some_and(|line| line.start < image.range.end && line.end >= image.range.start);
            let character = tab.document.text[..image.range.start].chars().count();
            let caret = positioned.at(character);
            let origin = output.galley_pos + caret.min.to_vec2();
            let rect = egui::Rect::from_min_size(
                origin
                    + egui::vec2(
                        if active { 0.0 } else { -image.size.x },
                        if active { 29.16 } else { 0.0 },
                    ),
                image.size,
            );
            ui.expand_to_include_rect(rect);
            let response = ui.put(
                rect,
                egui::Image::new(&image.uri)
                    .fit_to_exact_size(image.size)
                    .sense(egui::Sense::click()),
            );
            crate::ui_probe::record(ui.ctx(), format!("embed:{ordinal}"), response.rect);
            if active {
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("embed-source:{ordinal}"),
                    egui::Rect::from_min_size(origin, egui::vec2(ui.available_width(), 29.16)),
                );
            }
            if response.clicked() {
                let end = tab.document.text[..image.range.end].chars().count();
                self.focus_range = Some((
                    id,
                    CCursorRange::two(CCursor::new(end), CCursor::new(character)),
                ));
                ui.ctx().request_repaint();
            }
        }
        let table_origins: Vec<_> = table_placements
            .iter()
            .map(|table| {
                let caret = positioned.at(table.characters.start);
                egui::pos2(output.galley_pos.x, output.galley_pos.y + caret.top())
            })
            .collect();
        if let Some(link) = self
            .tables
            .show(ui, tab, id, &table_placements, &table_origins)
        {
            follow = Some(link);
        }
        if let Some(character) = self.tables.take_exit() {
            self.focus_range = Some((id, CCursorRange::one(CCursor::new(character))));
        }
        if active_image && output.response.has_focus() {
            paint_image_selection(
                ui,
                &output,
                &images,
                &tab.document.text,
                selection_shape,
                selection,
            );
            if let Some(range) = output.cursor_range {
                let key = (range.primary.index.0, range.secondary.index.0);
                let now = ui.input(|input| input.time);
                if self.image_cursor != Some(key) || output.response.changed() {
                    self.image_cursor = Some(key);
                    self.image_cursor_time = now;
                }
                let cycle = (cursor_style.on_duration + cursor_style.off_duration) as f64;
                if !cursor_style.blink
                    || (now - self.image_cursor_time) % cycle < cursor_style.on_duration as f64
                {
                    let caret = output.galley.pos_from_cursor(range.primary);
                    let top = output.galley_pos + caret.min.to_vec2();
                    ui.painter()
                        .line_segment([top, top + egui::vec2(0.0, 29.16)], cursor_style.stroke);
                }
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
        }
        if let Some(character) = jump {
            output.response.request_focus();
            ui.ctx().request_repaint();
            let rect = output
                .galley
                .pos_from_cursor(CCursor::new(character))
                .translate(output.galley_pos.to_vec2());
            ui.scroll_to_rect(rect, Some(egui::Align::Center));
        }
        if ui.input(|i| i.modifiers.command)
            && output.response.clicked()
            && let Some(pointer) = output.response.interact_pointer_pos()
        {
            follow = link_at(
                &tab.document.text,
                output
                    .galley
                    .cursor_from_pos(pointer - output.galley_pos)
                    .index
                    .0,
            );
        }
        if output.response.has_focus()
            && !self.completion_dismissed
            && let Some(cursor) = output.cursor_range
            && let Some(completion) = link_completion(&tab.document.text, cursor.primary.index.0)
        {
            let candidates = files.rank_links(&completion.query, 8);
            let caret = output
                .galley
                .pos_from_cursor(cursor.primary)
                .translate(output.galley_pos.to_vec2());
            let screen = ui.ctx().content_rect();
            let height = (candidates.len().max(1) as f32 * 34.0 + 18.0).min(310.0);
            let position = egui::pos2(
                caret.left().min(screen.right() - 350.0).max(screen.left()),
                if caret.bottom() + height < screen.bottom() {
                    caret.bottom() + 4.0
                } else {
                    (caret.top() - height).max(screen.top())
                },
            );
            let mut chosen = None;
            egui::Area::new(id.with("link-completion"))
                .order(egui::Order::Foreground)
                .fixed_pos(position)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(330.0);
                        for (index, path) in candidates.iter().enumerate() {
                            let response = ui.add(
                                egui::Button::new(
                                    RichText::new(rust_notes::paths::display_path(path)).size(14.0),
                                )
                                .selected(index == self.completion_selected)
                                .frame(false)
                                .min_size(egui::vec2(330.0, 30.0)),
                            );
                            if response.clicked() {
                                chosen = Some(path.clone());
                            }
                        }
                        if candidates.is_empty() {
                            ui.label(
                                RichText::new(
                                    "No matching files. Finish the link to create a note.",
                                )
                                .size(13.0)
                                .weak(),
                            );
                        }
                    });
                });
            if let Some(path) = chosen {
                let caret = complete_link(
                    &mut tab.document.text,
                    &completion,
                    &rust_notes::paths::wiki_destination(&path),
                );
                set_cursor(ui.ctx(), id, caret);
                ui.memory_mut(|m| m.request_focus(id));
                tab.edited();
            }
        }
        follow
    }
}

fn set_cursor(ctx: &egui::Context, id: egui::Id, caret: usize) {
    set_cursor_range(ctx, id, CCursorRange::one(CCursor::new(caret)));
}

fn set_cursor_range(ctx: &egui::Context, id: egui::Id, range: CCursorRange) {
    let mut state = TextEditState::load(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(range));
    state.store(ctx, id);
}

fn retain_visible_images(
    images: &mut Vec<LiveImage>,
    sections: &rust_notes::folding::Sections,
    collapsed: &HashSet<String>,
) {
    if collapsed.is_empty() {
        return;
    }
    images.retain(|image| {
        !sections.0.iter().any(|section| {
            collapsed.contains(&section.key) && section.body.contains(&image.range.start)
        })
    });
}

fn reserve_table_rows(galley: &mut Arc<egui::Galley>, tables: &[crate::table_ui::Placement]) {
    if tables.is_empty() {
        return;
    }
    let galley = Arc::make_mut(galley);
    let heights: Vec<_> = galley
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            galley
                .rows
                .get(index + 1)
                .map_or(galley.rect.max.y, |next| next.pos.y)
                - row.pos.y
        })
        .collect();
    let mut character = 0;
    let mut offset = 0.0;
    let mut table_index = 0;
    let mut bounds = egui::Rect::NOTHING;
    for (index, row) in galley.rows.iter_mut().enumerate() {
        while tables
            .get(table_index)
            .is_some_and(|table| table.characters.end <= character)
        {
            table_index += 1;
        }
        let length = row.char_count_including_newline().0;
        row.pos.y += offset;
        if let Some(table) = tables
            .get(table_index)
            .filter(|table| table.characters.contains(&character))
        {
            let height = if character == table.characters.start {
                table.height() + 2.0
            } else {
                0.0
            };
            let mut data = row.row.clone_without_visuals();
            data.size.y = height;
            row.row = Arc::new(data);
            offset += height - heights[index];
        } else {
            bounds |= row.visuals.mesh_bounds.translate(row.pos.to_vec2());
        }
        character += length;
    }
    galley.rect.max.y += offset;
    galley.mesh_bounds = bounds;
}

fn collapse_rows(galley: &mut Arc<egui::Galley>, hidden: &[Range<usize>]) {
    if hidden.is_empty() {
        return;
    }
    let galley = Arc::make_mut(galley);
    let heights: Vec<_> = galley
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            galley
                .rows
                .get(index + 1)
                .map_or(galley.rect.max.y, |next| next.pos.y)
                - row.pos.y
        })
        .collect();
    let mut character = 0;
    let mut removed = 0.0;
    let mut hidden_index = 0;
    let total = galley.job.text.chars().count();
    let mut bounds = egui::Rect::NOTHING;
    for (index, row) in galley.rows.iter_mut().enumerate() {
        let length = row.char_count_including_newline().0;
        row.pos.y -= removed;
        while hidden
            .get(hidden_index)
            .is_some_and(|range| range.end <= character && character != total)
        {
            hidden_index += 1;
        }
        if hidden.get(hidden_index).is_some_and(|range| {
            range.contains(&character) || character == range.end && character == total
        }) {
            removed += heights[index];
            let mut data = row.row.clone_without_visuals();
            data.size.y = 0.0;
            row.row = Arc::new(data);
        } else {
            bounds |= row.visuals.mesh_bounds.translate(row.pos.to_vec2());
        }
        character += length;
    }
    galley.rect.max.y -= removed;
    galley.mesh_bounds = bounds;
}

fn paint_heading_folds(
    ui: &mut egui::Ui,
    positioned: &PositionedText<'_>,
    sections: &rust_notes::folding::Sections,
    collapsed: &mut HashSet<String>,
) {
    let output = positioned.output;
    for (index, section) in sections.0.iter().enumerate() {
        let heading = positioned
            .at(section.heading_chars.start)
            .translate(output.galley_pos.to_vec2());
        if heading.height() <= 0.0 {
            continue;
        }
        let row = egui::Rect::from_min_max(
            egui::pos2(output.response.rect.left(), heading.top()),
            egui::pos2(output.response.rect.right(), heading.bottom()),
        );
        if !row.intersects(ui.clip_rect()) {
            continue;
        }
        crate::ui_probe::record(ui.ctx(), format!("edit-heading:{index}"), row);
        if section.body_chars.is_empty() {
            continue;
        }
        let folded = collapsed.contains(&section.key);
        let body = positioned
            .at(section.body_chars.start)
            .translate(output.galley_pos.to_vec2());
        if !folded && body.height() > 0.0 {
            crate::ui_probe::record(
                ui.ctx(),
                format!("edit-body:{index}"),
                body.expand2(egui::vec2(8.0, 0.0)),
            );
        }
        let rect = egui::Rect::from_center_size(
            egui::pos2(row.left() - 16.0, row.center().y),
            egui::vec2(16.0, 18.0),
        );
        let toggle = ui.interact(
            rect,
            output.response.id.with(("heading-fold", &section.key)),
            egui::Sense::click(),
        );
        crate::ui_probe::record(ui.ctx(), format!("edit-fold:{index}"), rect);
        if toggle.clicked() {
            output.response.request_focus();
            if folded {
                collapsed.remove(&section.key);
            } else {
                collapsed.insert(section.key.clone());
                if let Some(mut state) = TextEditState::load(ui.ctx(), output.response.id)
                    && state.cursor.char_range().is_some_and(|cursor| {
                        section.body_chars.contains(&cursor.primary.index.0)
                            || cursor.primary.index.0 == section.body_chars.end
                                && section.body_chars.end == output.galley.end().index.0
                    })
                {
                    state
                        .cursor
                        .set_char_range(Some(CCursorRange::one(CCursor::new(
                            section.heading_chars.end.saturating_sub(1),
                        ))));
                    state.store(ui.ctx(), output.response.id);
                }
            }
            ui.ctx().request_repaint();
        }
        if folded
            || row.contains(ui.input(|input| input.pointer.hover_pos().unwrap_or_default()))
            || toggle.hovered()
            || toggle.has_focus()
        {
            crate::theme::disclosure(ui, if folded { 0.0 } else { 1.0 }, &toggle);
        }
        toggle.on_hover_text(if folded {
            "Expand section"
        } else {
            "Collapse section"
        });
    }
}

fn space_rows(
    galley: &mut Arc<egui::Galley>,
    source: &str,
    parsed: &Markdown,
    live: bool,
    images: &[LiveImage],
    active_line: Option<&Range<usize>>,
) {
    let galley = Arc::make_mut(galley);
    let mut character = 0;
    let mut byte = 0;
    let mut offset = 0.0;
    let mut line_start = true;
    let mut list_line = false;
    let mut scalars = source.chars();
    let active_images: Vec<_> = images
        .iter()
        .filter(|image| {
            active_line
                .is_some_and(|line| line.start < image.range.end && line.end >= image.range.start)
        })
        .map(|image| {
            (
                source[..image.range.end].chars().count() - 1,
                image.size.y + 16.0,
            )
        })
        .collect();
    for row in &mut galley.rows {
        if live && line_start && parsed.heading_starts.binary_search(&byte).is_ok() {
            offset += 16.0;
        }
        if line_start {
            list_line = parsed.item_starts.binary_search(&byte).is_ok();
            if list_line {
                offset += 1.35;
            }
        }
        row.pos.y += offset;
        let length = row.char_count_excluding_newline().0;
        let row_end = character + length;
        let extra = active_images
            .iter()
            .filter_map(|&(closing, height)| {
                (closing >= character && closing < row_end).then_some(height)
            })
            .fold(0.0, f32::max);
        if extra > 0.0 {
            Arc::make_mut(&mut row.row).size.y += extra;
            offset += extra;
        }
        line_start = row.ends_with_newline;
        character = row_end + usize::from(row.ends_with_newline);
        byte += scalars
            .by_ref()
            .take(length + usize::from(row.ends_with_newline))
            .map(char::len_utf8)
            .sum::<usize>();
        if list_line && (line_start || byte == source.len()) {
            offset += 1.35;
        }
    }
    galley.rect.max.y += offset;
    galley.mesh_bounds.max.y += offset;
}

fn paint_code_blocks(
    ui: &egui::Ui,
    positioned: &PositionedText<'_>,
    source: &str,
    parsed: &Markdown,
    live: bool,
    active_line: Option<&Range<usize>>,
    background: egui::layers::ShapeIdx,
) {
    let output = positioned.output;
    let mut shapes = Vec::new();
    let mut previous_byte = 0;
    let mut character = 0;
    for (ordinal, block) in parsed.code_blocks.iter().enumerate() {
        character += source[previous_byte..block.range.start].chars().count();
        let start = character;
        let end_byte = block.range.end
            - (source[block.range.clone()].len()
                - source[block.range.clone()]
                    .trim_end_matches(['\r', '\n'])
                    .len());
        character += source[block.range.start..end_byte].chars().count();
        let end = character;
        previous_byte = end_byte;
        let first = positioned.at(start);
        let last = positioned.at(end);
        if first.height() <= 0.0 && last.height() <= 0.0 {
            continue;
        }
        let rect = egui::Rect::from_min_max(
            egui::pos2(
                output.response.rect.left(),
                output.galley_pos.y + first.top(),
            ),
            egui::pos2(
                output.response.rect.right(),
                output.galley_pos.y + last.bottom(),
            ),
        );
        if !rect.intersects(ui.clip_rect()) {
            continue;
        }
        shapes.push(egui::Shape::rect_filled(
            rect,
            6,
            ui.visuals().code_bg_color,
        ));
        crate::ui_probe::record(ui.ctx(), format!("code-block:{ordinal}"), rect);
        let active = !live
            || active_line
                .is_some_and(|line| line.start < block.range.end && line.end >= block.range.start);
        if active {
            crate::ui_probe::record(ui.ctx(), format!("code-source:{ordinal}"), rect);
        }
        if let Some(code) = block.content(source) {
            crate::code_ui::copy(
                ui,
                output.response.id.with(block.range.start),
                rect,
                code,
                ordinal,
                false,
            );
        }
        if !ui.rect_contains_pointer(rect)
            && !active
            && let Some(language) = block.language.split_whitespace().next()
        {
            ui.painter().text(
                rect.right_top() + egui::vec2(-12.0, 6.0),
                egui::Align2::RIGHT_TOP,
                language,
                FontId::proportional(12.0),
                ui.visuals().weak_text_color(),
            );
        }
    }
    ui.painter().set(background, egui::Shape::Vec(shapes));
}

fn paint_markers(
    ui: &egui::Ui,
    positioned: &PositionedText<'_>,
    source: &str,
    parsed: &Markdown,
    live: bool,
    active_line: Option<&Range<usize>>,
) {
    let output = positioned.output;
    let mut previous_byte = 0;
    let mut character = 0;
    let mut position = |byte| {
        if byte < previous_byte {
            previous_byte = 0;
            character = 0;
        }
        character += source[previous_byte..byte].chars().count();
        previous_byte = byte;
        positioned
            .at(character)
            .translate(output.galley_pos.to_vec2())
    };
    for (ordinal, range) in parsed.setext_markers.iter().enumerate() {
        let start = position(range.start);
        let end = position(range.end);
        crate::ui_probe::record(
            ui.ctx(),
            format!("setext:{ordinal}"),
            egui::Rect::from_min_max(start.min, egui::pos2(end.left(), end.bottom())),
        );
    }
    if !live {
        return;
    }
    for (ordinal, quote) in parsed.quotes.iter().enumerate() {
        let start = position(quote.range.start);
        let end_byte = source[quote.range.end..]
            .find('\n')
            .map_or(source.len(), |end| quote.range.end + end);
        let end = position(end_byte);
        let active = active_line
            .is_some_and(|line| line.start <= quote.range.start && line.end >= quote.range.end);
        let left = start.left()
            - if active {
                0.0
            } else {
                17.0 * quote.depth as f32
            };
        let rect = egui::Rect::from_min_max(
            egui::pos2(left, start.top()),
            egui::pos2(output.response.rect.right(), end.bottom()),
        );
        for depth in 0..quote.depth {
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    rect.min + egui::vec2(17.0 * depth as f32, 0.0),
                    egui::pos2(left + 17.0 * depth as f32 + 2.0, rect.bottom()),
                ),
                0.0,
                crate::theme::interactive(ui),
            );
        }
        crate::ui_probe::record(ui.ctx(), format!("quote:{ordinal}"), rect);
        if active {
            crate::ui_probe::record(ui.ctx(), format!("quote-source:{ordinal}"), rect);
        }
    }
}

fn paint_image_selection(
    ui: &egui::Ui,
    output: &egui::text_edit::TextEditOutput,
    images: &[LiveImage],
    source: &str,
    shape: egui::layers::ShapeIdx,
    color: Color32,
) {
    let Some(range) = output.cursor_range else {
        return;
    };
    let [start, end] = range.sorted_cursors();
    let mut character = 0;
    let mut shapes = Vec::new();
    for row in &output.galley.rows {
        let length = row.char_count_excluding_newline().0;
        let row_end = character + length;
        if start.index.0 <= row_end && end.index.0 > character {
            let from = start.index.0.saturating_sub(character).min(length);
            let to = end.index.0.saturating_sub(character).min(length);
            let image_row = images.iter().any(|image| {
                let first = source[..image.range.start].chars().count();
                first >= character && first <= row_end
            });
            let height = if image_row { 29.16 } else { row.height() };
            let mut rects = row.selection_rects(from..to);
            if end.index.0 > row_end {
                rects.push(egui::Rect::from_min_size(
                    egui::pos2(row.size.x, 0.0),
                    egui::vec2(4.0, height),
                ));
            }
            for mut rect in rects {
                rect.max.y = height;
                rect = rect
                    .translate(output.galley_pos.to_vec2() + row.pos.to_vec2())
                    .intersect(output.text_clip_rect);
                shapes.push(egui::Shape::rect_filled(rect, 0.0, color));
            }
        }
        character = row_end + usize::from(row.ends_with_newline);
    }
    ui.painter().set(shape, egui::Shape::Vec(shapes));
}

fn layout(
    ui: &egui::Ui,
    text: &str,
    parsed: &Markdown,
    width: f32,
    decorations: Decorations<'_>,
) -> LayoutJob {
    let Decorations {
        live,
        active_line,
        matched,
        images,
    } = decorations;
    let mut job = LayoutJob::default();
    job.wrap.max_width = width;
    let colors = crate::syntax::highlights(ui, text, parsed);
    let mut character = 0;
    for span in &parsed.spans {
        // Split at line boundaries so a multiline emphasis doesn't reveal all its markers.
        let mut start = span.range.start;
        for section in text[span.range.clone()].split_inclusive('\n') {
            let end = start + section.len();
            let style = span.style;
            let active = active_line.is_some_and(|line| start <= line.end && end > line.start);
            let code_block = parsed
                .code_blocks
                .get(
                    parsed
                        .code_blocks
                        .partition_point(|block| block.range.end <= start),
                )
                .filter(|block| block.range.contains(&start));
            let code_active = code_block.is_some_and(|block| {
                active_line.is_some_and(|line| {
                    line.start < block.range.end && line.end >= block.range.start
                })
            });
            let image = images
                .iter()
                .find(|image| start >= image.range.start && end <= image.range.end);
            let hidden = live
                && (style.property
                    || style.hidden && !active && !style.image
                    || style.quote_marker && !active
                    || style.fence && !code_active
                    || image.is_some() && !active);
            let size = if hidden {
                0.01
            } else if style.heading > 0 {
                match style.heading {
                    1 => 32.436,
                    2 => 28.836,
                    3 => 25.632,
                    _ => 22.788,
                }
            } else if style.block_code {
                crate::code_ui::FONT_SIZE
            } else if style.code {
                16.0
            } else {
                18.0
            };
            let family = if style.code {
                FontFamily::Monospace
            } else if style.heading == 1 {
                FontFamily::Name("bold".into())
            } else if style.strong {
                FontFamily::Name("semibold".into())
            } else if style.italic {
                FontFamily::Name("italic".into())
            } else {
                FontFamily::Proportional
            };
            let color = if hidden {
                Color32::TRANSPARENT
            } else if style.marker {
                crate::theme::marker(ui)
            } else if style.link {
                crate::theme::accent(ui)
            } else if style.heading > 0 {
                crate::theme::heading(ui, style.heading)
            } else if style.block_code {
                ui.visuals().weak_text_color()
            } else {
                ui.visuals().text_color()
            };
            let count = section.chars().count();
            let highlighted = matched
                .is_some_and(|range| character < range.end && character + count > range.start);
            let display = if live && style.bullet && !active {
                "•"
            } else {
                section
            };
            let quote_indent = if live && !active && style.quote_marker {
                parsed
                    .quotes
                    .binary_search_by_key(&start, |quote| quote.range.start)
                    .map_or(0.0, |index| 17.0 * parsed.quotes[index].depth as f32)
            } else {
                0.0
            };
            let list_indent = if live && style.bullet { 13.5 } else { 0.0 };
            let code_indent =
                if code_block.is_some() && (start == 0 || text[..start].ends_with('\n')) {
                    crate::code_ui::HORIZONTAL_INSET
                } else {
                    0.0
                };
            let leading = code_indent
                + quote_indent
                + list_indent
                + image
                    .filter(|image| !active && start == image.range.start)
                    .map_or(0.0, |image| image.size.x);
            let format = egui::TextFormat {
                font_id: FontId::new(size, family),
                color,
                line_height: Some(if live && style.property {
                    0.01
                } else if style.block_code {
                    crate::code_ui::LINE_HEIGHT
                } else if let Some(image) = image.filter(|_| !active) {
                    image.size.y + 16.0
                } else if section == "\n" {
                    29.16
                } else if style.heading > 0 {
                    size.max(24.0) * 1.2
                } else {
                    29.16
                }),
                valign: if image.is_some() && active {
                    egui::Align::Min
                } else {
                    egui::Align::Center
                },
                italics: style.italic,
                extra_letter_spacing: if style.heading > 0 {
                    -0.025 * size
                } else {
                    0.0
                },
                strikethrough: if style.strike {
                    Stroke::new(1.0, color)
                } else {
                    Stroke::NONE
                },
                underline: if live && style.link && !style.marker && !hidden {
                    Stroke::new(1.0, color)
                } else {
                    Stroke::NONE
                },
                background: if highlighted {
                    Color32::from_rgba_unmultiplied(170, 135, 20, 95)
                } else if style.code && !style.block_code {
                    ui.visuals().code_bg_color
                } else {
                    Color32::TRANSPARENT
                },
                ..Default::default()
            };
            if code_block.is_some() && style.block_code && !style.fence && !hidden {
                crate::code_ui::append_colored(&mut job, display, start, leading, format, &colors);
            } else {
                job.append(display, leading, format);
            }
            character += count;
            start = end;
        }
    }
    job
}

#[cfg(test)]
mod tests {
    fn pressed(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn pre_editor_actions_take_only_preceding_plain_text_and_defer_unsafe_sequences() {
        let mut events = vec![
            pressed(Key::A),
            egui::Event::Text("π یادداشت".into()),
            pressed(Key::Enter),
            egui::Event::Text("after".into()),
            pressed(Key::Enter),
        ];
        assert_eq!(
            text_before_action(&mut events).as_deref(),
            Some("π یادداشت")
        );
        assert_eq!(
            events,
            vec![
                pressed(Key::A),
                pressed(Key::Enter),
                egui::Event::Text("after".into()),
                pressed(Key::Enter)
            ]
        );
        for event in [
            pressed(Key::ArrowLeft),
            pressed(Key::Backspace),
            egui::Event::Paste("paste".into()),
        ] {
            let mut events = vec![egui::Event::Text("before".into()), event, pressed(Key::Tab)];
            let original = events.clone();
            assert!(text_before_action(&mut events).is_none());
            assert_eq!(events, original);
        }
        let mut ordinary = vec![egui::Event::Text("ordinary typing".into())];
        let original = ordinary.clone();
        assert_eq!(text_before_action(&mut ordinary), Some(String::new()));
        assert_eq!(ordinary, original);
    }

    #[test]
    fn modifier_transitions_alone_do_not_defer_link_following() {
        let mut action = pressed(Key::Enter);
        if let egui::Event::Key { modifiers, .. } = &mut action {
            *modifiers = egui::Modifiers::COMMAND;
        }
        let mut events = vec![
            pressed(Key::ControlLeft),
            egui::Event::ModifiersChanged(egui::Modifiers::COMMAND),
            action,
        ];
        let original = events.clone();
        assert_eq!(text_before_action(&mut events), Some(String::new()));
        assert_eq!(events, original);
    }

    #[test]
    fn shifted_completion_text_keeps_modifier_events_without_deferring_tab() {
        let shift = egui::Event::ModifiersChanged(egui::Modifiers::SHIFT);
        let release = egui::Event::ModifiersChanged(egui::Modifiers::NONE);
        let mut events = vec![
            shift.clone(),
            egui::Event::Text("[[Viewer".into()),
            release.clone(),
            pressed(Key::Tab),
        ];
        assert_eq!(text_before_action(&mut events).as_deref(), Some("[[Viewer"));
        assert_eq!(events, vec![shift, release, pressed(Key::Tab)]);
        for modifiers in [
            egui::Modifiers::CTRL,
            egui::Modifiers::ALT,
            egui::Modifiers::COMMAND,
        ] {
            let mut events = vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::Text("before".into()),
                pressed(Key::Tab),
            ];
            assert_eq!(text_before_action(&mut events).as_deref(), Some("before"));
            assert_eq!(
                events,
                vec![egui::Event::ModifiersChanged(modifiers), pressed(Key::Tab)]
            );
            let mut shortcut = pressed(Key::A);
            if let egui::Event::Key {
                modifiers: state, ..
            } = &mut shortcut
            {
                *state = modifiers;
            }
            let mut events = vec![
                shortcut,
                egui::Event::Text("before".into()),
                pressed(Key::Tab),
            ];
            let original = events.clone();
            assert_eq!(text_before_action(&mut events), None);
            assert_eq!(events, original);
        }
    }

    #[test]
    fn completion_consumes_only_the_first_action_in_event_order() {
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events: vec![pressed(Key::Enter), pressed(Key::Tab), pressed(Key::Enter)],
                ..Default::default()
            },
            |ui| {
                assert!(!consume_action(ui, egui::Modifiers::NONE, &[Key::Tab]));
                assert!(consume_action(
                    ui,
                    egui::Modifiers::NONE,
                    &[Key::Tab, Key::Enter]
                ));
                assert_eq!(
                    ui.input(|input| input
                        .events
                        .iter()
                        .filter_map(|event| {
                            if let egui::Event::Key {
                                key, pressed: true, ..
                            } = event
                            {
                                Some(*key)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()),
                    vec![Key::Tab, Key::Enter]
                );
                assert!(!consume_action(ui, egui::Modifiers::COMMAND, &[Key::Tab]));
                ui.input_mut(|input| {
                    let mut shifted = pressed(Key::Enter);
                    if let egui::Event::Key { modifiers, .. } = &mut shifted {
                        *modifiers = egui::Modifiers::SHIFT;
                    }
                    input.events = vec![shifted];
                });
                assert!(!consume_action(ui, egui::Modifiers::NONE, &[Key::Enter]));
            },
        );
        output.textures_delta.clear();
    }

    #[test]
    fn folded_rows_keep_logical_text_but_hit_testing_and_arrows_skip_the_hidden_body() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let source = "## Parent π\n\nHIDDEN\n\n## Sibling\nVISIBLE";
        let sections = rust_notes::folding::Sections::new(source, &Markdown::parse(source));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut galley = ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap(source.into(), FontId::proportional(18.0), Color32::WHITE)
            });
            let plain = galley.clone();
            let start = sections.0[1].heading_chars.start;
            let end = sections.0[0].heading_chars.end - 1;
            collapse_rows(&mut galley, &[sections.0[0].body_chars.clone()]);
            assert_eq!(galley.text(), source);
            assert_eq!(galley.end().index.0, source.chars().count());
            assert!(galley.size().y < plain.size().y);
            let hidden: Vec<_> = galley
                .rows
                .iter()
                .filter(|row| row.height() == 0.0)
                .collect();
            assert!(!hidden.is_empty());
            assert!(hidden.iter().all(|row| row.visuals.mesh.is_empty()));
            for (folded, original) in galley.rows.iter().zip(&plain.rows) {
                assert_eq!(folded.glyphs, original.glyphs);
                if folded.height() > 0.0 {
                    assert_eq!(folded.visuals, original.visuals);
                }
            }
            assert!(plain.rows.iter().any(|row| !row.visuals.mesh.is_empty()));
            assert_eq!(
                galley
                    .cursor_right_one_character(&CCursor::new(end))
                    .index
                    .0,
                start
            );
            assert_eq!(
                galley
                    .cursor_left_one_character(&CCursor::new(start))
                    .index
                    .0,
                end
            );
            assert_eq!(
                galley.cursor_down_one_row(&CCursor::new(0), None).0.index.0,
                start
            );
            assert_eq!(
                galley
                    .cursor_up_one_row(&CCursor::new(start), None)
                    .0
                    .index
                    .0,
                0
            );
            let point = galley.pos_from_cursor(CCursor::new(start)).center();
            assert_eq!(galley.cursor_from_pos(point.to_vec2()).index.0, start);
            assert_eq!(
                plain.cursor_right_one_character(&CCursor::new(end)).index.0,
                end + 1
            );
        });
        output.textures_delta.clear();
    }

    #[test]
    fn live_table_space_retains_logical_text_and_only_removes_its_own_visuals() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let source = "Before π\n\n| A | B |\n| --- | --- |\n| α | β |\n\nAfter سلام\n";
        let parsed = Markdown::parse(source);
        let table = &parsed.tables[0];
        let start = source[..table.range.start].chars().count();
        let end = source[..table.range.end].chars().count();
        let placement = crate::table_ui::Placement {
            ordinal: 0,
            bytes: table.range.clone(),
            characters: start..end,
            widths: vec![80.0, 80.0],
            heights: vec![32.4, 32.4],
        };
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut galley = ui.fonts_mut(|fonts| {
                fonts.layout_no_wrap(source.into(), FontId::proportional(18.0), Color32::WHITE)
            });
            let original = galley.clone();
            reserve_table_rows(&mut galley, std::slice::from_ref(&placement));
            assert_eq!(galley.text(), source);
            assert_eq!(galley.end().index.0, source.chars().count());
            let mut character = 0;
            let mut reserved = 0.0;
            let mut table_rows = 0;
            for (row, plain) in galley.rows.iter().zip(&original.rows) {
                assert_eq!(row.glyphs, plain.glyphs);
                if placement.characters.contains(&character) {
                    assert!(row.visuals.mesh.is_empty());
                    assert!(!plain.visuals.mesh.is_empty());
                    reserved += row.height();
                    table_rows += 1;
                } else {
                    assert_eq!(row.visuals, plain.visuals);
                }
                character += row.char_count_including_newline().0;
            }
            assert_eq!(table_rows, 3);
            assert!((reserved - placement.height() - 2.0).abs() < 0.01);
            let after = galley.pos_from_cursor(CCursor::new(end));
            let before = galley.pos_from_cursor(CCursor::new(start));
            assert!((after.top() - before.top() - reserved).abs() < 0.01);
            assert!(original.rows.iter().all(|row| row.height() > 0.0));
        });
        output.textures_delta.clear();
    }

    use super::*;

    #[test]
    fn syntax_layout_preserves_original_bytes_and_indents_each_code_line_once() {
        let source = include_str!("../tests/fixtures/syntax.md");
        let parsed = Markdown::parse(source);
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, true);
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            for live in [false, true] {
                let job = layout(
                    ui,
                    source,
                    &parsed,
                    920.0,
                    Decorations {
                        live,
                        active_line: None,
                        matched: None,
                        images: &[],
                    },
                );
                assert_eq!(job.text, source);
                for (needle, expected) in [
                    ("fn main", Color32::from_rgb(250, 153, 205)),
                    ("**literal**", Color32::from_rgb(68, 207, 110)),
                    ("42", Color32::from_rgb(168, 130, 255)),
                    ("// A comment", crate::theme::DARK_FAINT),
                ] {
                    let byte = source.find(needle).unwrap();
                    let section = job
                        .sections
                        .iter()
                        .find(|section| section.byte_range.contains(&byte.into()))
                        .unwrap();
                    assert_eq!(section.format.color, expected, "{needle}");
                    assert_eq!(section.format.font_id.size, 15.75);
                }
                let first = source.find("fn main").unwrap();
                let end = first + source[first..].find('\n').unwrap();
                let leading: f32 = job
                    .sections
                    .iter()
                    .filter(|section| {
                        section.byte_range.start.0 >= first && section.byte_range.start.0 <= end
                    })
                    .map(|section| section.leading_space)
                    .sum();
                assert_eq!(leading, 16.0);
            }
        });
        output.textures_delta.clear();
    }

    #[test]
    fn indexed_decorations_match_egui_at_unicode_and_wrapped_row_boundaries() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        for source in [
            "",
            "é 👋 你好\n\nlast\n",
            "A long wrapped paragraph with many words and é 👋 characters.\nAnother long wrapped paragraph with many words.",
        ] {
            let mut text = source.to_owned();
            let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
                let output = TextEdit::multiline(&mut text)
                    .desired_width(80.0)
                    .font(FontId::proportional(18.0))
                    .show(ui);
                let positioned = PositionedText::new(&output);
                for character in 0..=source.chars().count() {
                    assert_eq!(
                        positioned.at(character),
                        output.galley.pos_from_cursor(CCursor::new(character)),
                        "character {character} in {source:?}",
                    );
                }
                assert_eq!(
                    positioned.at(source.chars().count() + 10),
                    output.galley.pos_from_cursor(output.galley.end())
                );
                if source.len() > 70 {
                    assert!(output.galley.rows.len() > 3, "Exercise soft wrapping");
                }
            });
            frame.textures_delta.clear();
            assert_eq!(text, source);
        }
    }
}
