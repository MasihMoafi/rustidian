use eframe::egui::{
    self, Color32, FontFamily, FontId, Key, RichText, Stroke, TextEdit,
    text::{CCursor, CCursorRange, LayoutJob},
    widgets::text_edit::TextEditState,
};
use rustidian::{
    editor::{Markdown, complete_link, find_matches, link_at, link_completion, wrap},
    highlight::Color,
    search::FileMatcher,
    tab::Tab,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

#[derive(Default)]
pub struct Find {
    pub open: bool,
    pub focus: bool,
    query: String,
    replace: String,
    replacing: bool,
    selected: usize,
    restore: Option<Range<usize>>,
    /// When the closed bar began waiting for the editor to take focus.
    restore_since: Option<f64>,
}

#[derive(Default)]
pub struct Editor {
    pub jump: Option<usize>,
    pub focus: bool,
    pub error: Option<String>,
    pending_edit: Option<PendingEdit>,
    selection: Option<(u64, CCursorRange)>,
    source: String,
    /// Bumped whenever `source` is replaced, so a cached layout can name its text.
    source_version: u64,
    parsed: Markdown,
    sections: rustidian::folding::Sections,
    layout_cache: Option<LayoutCache>,
    tables: crate::table_ui::Tables,
    completion_key: String,
    completion_selected: usize,
    completion_dismissed: bool,
    focus_range: Option<(egui::Id, CCursorRange)>,
    image_cursor: Option<(usize, usize)>,
    image_cursor_time: f64,
    pressed_link: Option<(u64, egui::PointerButton, String)>,
    /// The release of a right-click that opened the note menu, held back from egui.
    menu_release: bool,
    /// Text the note menu asked the sidebar search to find.
    pub search_request: Option<String>,
}

/// Where a right-click opens the note's own menu before egui sees it: egui's
/// press would first place the caret and drop the selection the menu acts on.
#[derive(Clone, Copy, Default, PartialEq)]
pub enum MenuArea {
    #[default]
    Editor,
    Reading,
    /// A control inside the note with a menu of its own, such as a formula.
    Native,
}

#[derive(Clone)]
struct MenuTarget {
    rect: egui::Rect,
    layer: egui::LayerId,
    area: MenuArea,
}

fn menu_targets_id() -> egui::Id {
    egui::Id::unique("note-menu-targets")
}

fn menu_click_id() -> egui::Id {
    egui::Id::unique("note-menu-click")
}

/// Let a right-click inside `rect` open the menu of `area` in the next frame.
pub fn menu_target(ui: &egui::Ui, rect: egui::Rect, area: MenuArea) {
    let rect = rect.intersect(ui.clip_rect());
    if rect.is_positive() {
        let target = MenuTarget {
            rect,
            layer: ui.layer_id(),
            area,
        };
        ui.ctx().data_mut(|data| {
            data.get_temp_mut_or_default::<Vec<MenuTarget>>(menu_targets_id())
                .push(target)
        });
    }
}

/// The right-click held back for `area` in this frame, if any.
pub fn take_menu_click(ctx: &egui::Context, area: MenuArea) -> Option<egui::Pos2> {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|data| {
        let (pos, owner, at) = data.get_temp::<(egui::Pos2, MenuArea, u64)>(menu_click_id())?;
        if at == frame && owner != area {
            return None;
        }
        data.remove::<(egui::Pos2, MenuArea, u64)>(menu_click_id());
        (at == frame).then_some(pos)
    })
}

type Histories = HashMap<std::path::PathBuf, TextEditState>;

fn histories(root: &std::path::Path) -> egui::Id {
    egui::Id::unique(("document-edit-histories", root))
}

pub fn document_history(
    ctx: &egui::Context,
    root: &std::path::Path,
    path: &std::path::Path,
) -> TextEditState {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Histories>(histories(root))
            .entry(path.to_owned())
            .or_default()
            .clone()
    })
}

pub fn clear_layout_history(ctx: &egui::Context, root: &std::path::Path) {
    let states = ctx.data(|data| {
        data.get_temp::<Histories>(histories(root))
            .unwrap_or_default()
    });
    for mut state in states.into_values() {
        let mut history = state.undoer();
        history.clear_markers();
        state.set_undoer(history);
    }
}

pub fn record_document_edit(
    ctx: &egui::Context,
    root: &std::path::Path,
    before: &rustidian::vault::Document,
    after: &rustidian::vault::Document,
) {
    let mut state = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Histories>(histories(root))
            .entry(before.path.clone())
            .or_default()
            .clone()
    });
    let range = state
        .cursor
        .char_range()
        .unwrap_or_else(|| CCursorRange::one(CCursor::new(0)));
    let mut undoer = state.undoer();
    undoer.add_undo(&(range, before.text.clone()));
    undoer.feed_state(ctx.input(|input| input.time), &(range, after.text.clone()));
    undoer.add_undo(&(range, after.text.clone()));
    state.set_undoer(undoer);
}

pub fn relocate_histories(
    ctx: &egui::Context,
    root: &std::path::Path,
    from: &std::path::Path,
    to: &std::path::Path,
) {
    ctx.data_mut(|data| {
        let entries = data.get_temp_mut_or_default::<Histories>(histories(root));
        let moving: Vec<_> = entries
            .keys()
            .filter(|path| path.starts_with(from))
            .cloned()
            .collect();
        for path in moving {
            if let Some(state) = entries.remove(&path) {
                entries.insert(rustidian::paths::moved_path(&path, from, to, true), state);
            }
        }
    });
    crate::table_layout::relocate(ctx, from, to);
}

pub fn clear_document_history(ctx: &egui::Context, root: &std::path::Path, path: &std::path::Path) {
    let state = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Histories>(histories(root))
            .get(path)
            .cloned()
    });
    if let Some(mut state) = state {
        state.clear_undoer();
    }
}

struct PendingEdit {
    tab: u64,
    path: std::path::PathBuf,
    source: String,
    operation: Edit,
    selection: Option<CCursorRange>,
}

enum Edit {
    Highlight(Color),
    NativeHighlight(rustidian::highlight::NativeColor),
    Insert { text: String, caret_back: usize },
    Wrap(&'static str, &'static str),
    Heading(usize),
}

/// The note menu's formatting, in Obsidian's order: label, shortcut, markers.
const FORMATS: [(&str, &str, &str, &str); 6] = [
    ("Bold", "Ctrl+B", "**", "**"),
    ("Italic", "Ctrl+I", "*", "*"),
    ("Strikethrough", "", "~~", "~~"),
    ("Highlight", "Ctrl+Shift+H", "==", "=="),
    ("Code", "", "`", "`"),
    ("Link", "", "[[", "]]"),
];

#[derive(Clone, PartialEq)]
struct LiveImage {
    range: Range<usize>,
    block: Option<Range<usize>>,
    uri: String,
    size: egui::Vec2,
    math: Option<usize>,
}

// Keep the original allocation alive so its pointer identifies the font
// layout safely. One cached value bounds memory while avoiding row/mesh
// cloning again on idle frames and repeated layout passes during typing.
/// The last galley with everything it was built from. Rebuilding the layout
/// job for an unchanged note costs milliseconds in a large note, so a frame
/// without edits (or a second layouter call in the same frame) reuses it.
struct LayoutCache {
    key: LayoutKey,
    /// The font layout before folds, tables and images reshape it, by
    /// paragraph: an edit lays out only the paragraphs it changed.
    paragraphs: egui::text::ParagraphLayout,
    galley: Arc<egui::Galley>,
}

#[derive(Clone, PartialEq)]
struct LayoutKey {
    version: u64,
    width: f32,
    live: bool,
    plain_text: bool,
    path: std::path::PathBuf,
    active_line: Option<Range<usize>>,
    matched: Option<Range<usize>>,
    images: Vec<LiveImage>,
    hidden: Vec<Range<usize>>,
    tables: Vec<crate::table_ui::Placement>,
    theme: ThemeKey,
    syntax: u64,
    /// Galleys point into the font atlas; a new atlas or scale needs new ones.
    fonts: (u64, f32),
}

/// Everything `layout` reads from the theme.
#[derive(Clone, PartialEq)]
struct ThemeKey {
    style: usize,
    dark: bool,
    scale: f32,
    text: Color32,
    weak: Color32,
    code_background: Color32,
    accent: Color32,
}

impl ThemeKey {
    fn new(ui: &egui::Ui) -> Self {
        let visuals = ui.visuals();
        Self {
            style: Arc::as_ptr(&ui.ctx().global_style()) as usize,
            dark: visuals.dark_mode,
            scale: crate::theme::document_size(ui, 1.0),
            text: visuals.text_color(),
            weak: visuals.weak_text_color(),
            code_background: visuals.code_bg_color,
            accent: visuals.hyperlink_color,
        }
    }
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
        self.at_row(character, row)
    }

    /// Like `at`, but a soft-wrap boundary belongs to the following row, where
    /// an inline image that begins there is laid out.
    fn at_start(&self, character: usize) -> egui::Rect {
        let rows = &self.output.galley.rows;
        let mut row = self
            .row_ends
            .partition_point(|&end| end < character)
            .min(self.row_ends.len().saturating_sub(1));
        if self.row_ends.get(row) == Some(&character)
            && rows
                .get(row)
                .is_some_and(|placed| !placed.ends_with_newline)
            && row + 1 < rows.len()
        {
            row += 1;
        }
        self.at_row(character, row)
    }

    fn at_row(&self, character: usize, row: usize) -> egui::Rect {
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

fn embedded_images(
    ui: &egui::Ui,
    text: &str,
    vault: Option<&rustidian::vault::Vault>,
    from: &std::path::Path,
    maths: &[rustidian::math::Span],
) -> Vec<LiveImage> {
    if !text.contains("![") {
        return Vec::new();
    }
    let width_limit = (ui.available_width() - 4.0).max(1.0);
    rustidian::assets::embedded_images(text)
        .into_iter()
        .filter_map(|image| {
            if maths.iter().any(|math| {
                math.range.start < image.range.end && math.range.end > image.range.start
            }) {
                return None;
            }
            if text[image.range.clone()].contains('\n') {
                return None;
            }
            let uri = rustidian::assets::image_uri(vault, from, &image.target)?;
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
                block: None,
                uri,
                size: egui::vec2(width, height),
                math: None,
            })
        })
        .collect()
}

fn live_images(
    ui: &egui::Ui,
    text: &str,
    vault: Option<&rustidian::vault::Vault>,
    from: &std::path::Path,
    parsed: &Markdown,
    active_line: Option<&Range<usize>>,
) -> Vec<LiveImage> {
    let mut images = embedded_images(ui, text, vault, from, &parsed.maths);
    for block in &parsed.code_blocks {
        if !block.language.eq_ignore_ascii_case("mermaid")
            || active_line
                .is_some_and(|line| line.start < block.range.end && line.end >= block.range.start)
        {
            continue;
        }
        let Some(closing) = &block.closing else {
            continue;
        };
        let body = &text[(block.opening.end + 1).min(closing.start)..closing.start];
        if let Ok(Some((uri, size))) = crate::diagrams::image(ui, body) {
            images.push(LiveImage {
                range: block.opening.clone(),
                block: Some(block.range.clone()),
                uri,
                size,
                math: None,
            });
        }
    }
    for (ordinal, math) in parsed.maths.iter().enumerate() {
        if parsed
            .callouts
            .iter()
            .any(|callout| callout.header.contains(&math.range.start))
        {
            continue;
        }
        if active_line
            .is_some_and(|line| line.start < math.range.end && line.end >= math.range.start)
        {
            continue;
        }
        // Inline line breaks cannot be collapsed without moving editing positions.
        if !math.display && text[math.range.clone()].contains('\n') {
            continue;
        }
        let style = parsed
            .spans
            .get(
                parsed
                    .spans
                    .partition_point(|span| span.range.end <= math.range.start),
            )
            .filter(|span| span.range.contains(&math.range.start))
            .map(|span| span.style)
            .unwrap_or_default();
        let color = if let Some([r, g, b, a]) = style.html.color {
            Color32::from_rgba_unmultiplied(r, g, b, a)
        } else if style.link {
            crate::theme::accent(ui)
        } else if style.heading > 0 {
            crate::theme::heading(ui, style.heading)
        } else {
            ui.visuals().text_color()
        };
        if let Ok(Some(formula)) = crate::math_ui::image(
            ui,
            &math.formula,
            math.display,
            crate::theme::prose_size(ui, style.heading),
            color,
        ) {
            let scale = (ui.available_width().max(1.0) / formula.size.x).min(1.0);
            let opening_end = text[math.range.clone()]
                .find('\n')
                .map_or(math.range.end, |offset| math.range.start + offset);
            images.push(LiveImage {
                range: if math.display {
                    math.range.start..opening_end
                } else {
                    math.range.clone()
                },
                block: math.display.then(|| math.range.clone()),
                uri: formula.uri,
                size: formula.size * scale,
                math: Some(ordinal),
            });
        }
    }
    images.sort_by_key(|image| image.range.start);
    images
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
            find.restore_since = None;
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

/// Make each line the selection touches a heading of `level`, or body text when
/// every one already is that heading, like Obsidian's heading commands; blank
/// lines among several stay blank. Takes and returns the selection in characters.
fn toggle_heading(text: &mut String, characters: Range<usize>, level: usize) -> Range<usize> {
    let byte = |character: usize| {
        text.char_indices()
            .nth(character)
            .map_or(text.len(), |(index, _)| index)
    };
    let (start, end) = (byte(characters.start), byte(characters.end));
    let first = text[..start].rfind('\n').map_or(0, |index| index + 1);
    let last = text[end..]
        .find('\n')
        .map_or(text.len(), |index| end + index);
    let lines: Vec<&str> = text[first..last].split('\n').collect();
    let several = lines.len() > 1;
    let skipped = |line: &str| several && line.trim().is_empty();
    let marker = format!("{} ", "#".repeat(level));
    let prefix = |line: &str| {
        let hashes = line.bytes().take_while(|byte| *byte == b'#').count();
        if (1..=6).contains(&hashes) && line[hashes..].starts_with(' ') {
            hashes + 1
        } else {
            0
        }
    };
    let remove = lines
        .iter()
        .filter(|line| !skipped(line))
        .all(|line| line.starts_with(&marker));
    let mut rebuilt = String::new();
    let mut ends = [characters.start, characters.end];
    let mut mapped = [false; 2];
    let mut old_line = text[..first].chars().count();
    let mut new_line = old_line;
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            rebuilt.push('\n');
        }
        let (old_prefix, new_prefix) = if skipped(line) {
            (0, "")
        } else if remove {
            (prefix(line), "")
        } else {
            (prefix(line), marker.as_str())
        };
        rebuilt.push_str(new_prefix);
        rebuilt.push_str(&line[old_prefix..]);
        let length = line.chars().count();
        for (end, done) in ends.iter_mut().zip(&mut mapped) {
            if !*done && *end <= old_line + length {
                *end = new_line + new_prefix.len() + (*end - old_line).saturating_sub(old_prefix);
                *done = true;
            }
        }
        old_line += length + 1;
        new_line += new_prefix.len() + length - old_prefix + 1;
    }
    text.replace_range(first..last, &rebuilt);
    ends[0]..ends[1]
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
    pub fn raw_input(
        &mut self,
        ctx: &egui::Context,
        raw: &mut egui::RawInput,
        live_tab: Option<u64>,
    ) {
        self.hold_menu_click(ctx, raw);
        self.tables.raw_input(ctx, raw, live_tab);
    }

    /// Keep a right-click on selected or reading text from egui and open the
    /// note menu for it; elsewhere egui places the caret and opens it itself.
    fn hold_menu_click(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        let targets = ctx
            .data_mut(|data| data.remove_temp::<Vec<MenuTarget>>(menu_targets_id()))
            .unwrap_or_default();
        let frame = ctx.cumulative_frame_nr();
        raw.events.retain(|event| {
            let egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Secondary,
                pressed,
                ..
            } = event
            else {
                return true;
            };
            if !pressed {
                return !std::mem::take(&mut self.menu_release);
            }
            let layer = ctx.layer_id_at(*pos);
            let under =
                |target: &&MenuTarget| Some(target.layer) == layer && target.rect.contains(*pos);
            if targets
                .iter()
                .filter(under)
                .any(|target| target.area == MenuArea::Native)
            {
                return true;
            }
            let Some(target) = targets.iter().find(under) else {
                return true;
            };
            ctx.data_mut(|data| data.insert_temp(menu_click_id(), (*pos, target.area, frame)));
            self.menu_release = true;
            false
        });
    }

    pub fn highlight(&mut self, tab: &Tab, color: Color) {
        self.edit(tab, Edit::Highlight(color));
    }

    pub fn native_highlight(&mut self, tab: &Tab, color: rustidian::highlight::NativeColor) {
        self.edit(tab, Edit::NativeHighlight(color));
    }

    pub fn insert_text(&mut self, tab: &Tab, text: String) {
        self.edit(
            tab,
            Edit::Insert {
                text,
                caret_back: 0,
            },
        );
    }

    pub fn insert_wiki_link(&mut self, tab: &Tab, text: String) {
        self.edit(
            tab,
            Edit::Insert {
                text,
                caret_back: 2,
            },
        );
    }

    fn edit(&mut self, tab: &Tab, operation: Edit) {
        self.pending_edit = Some(PendingEdit {
            tab: tab.id(),
            path: tab.document.path.clone(),
            source: tab.document.text.clone(),
            operation,
            selection: self
                .selection
                .filter(|(id, _)| *id == tab.id())
                .map(|(_, range)| range),
        });
        self.focus = true;
    }

    /// Obsidian's editor menu: the clipboard, then formatting for the selection.
    /// Actions run on the selection the editor held when the menu opened.
    fn note_menu(&mut self, ui: &mut egui::Ui, tab: &Tab, id: egui::Id, body_start: usize) {
        crate::theme::menu_style(ui);
        ui.set_min_width(240.0);
        let selected = self
            .selection
            .filter(|(owner, range)| {
                *owner == tab.id() && range.primary.index != range.secondary.index
            })
            .map(|(_, range)| {
                let range = range.as_sorted_char_range();
                tab.document
                    .text
                    .chars()
                    .skip(range.start.0)
                    .take(range.end.0 - range.start.0)
                    .collect::<String>()
            });
        let item = |ui: &mut egui::Ui, label: &str, shortcut: &str, enabled: bool| {
            let mut button = egui::Button::new(label);
            if !shortcut.is_empty() {
                button = button.shortcut_text(shortcut);
            }
            let response = ui.add_enabled(enabled, button);
            crate::ui_probe::record(ui.ctx(), format!("editor-menu:{label}"), response.rect);
            response.clicked()
        };
        let mut done = false;
        if item(ui, "Cut", "Ctrl+X", selected.is_some())
            && let Some(text) = &selected
        {
            ui.ctx().copy_text(text.clone());
            self.edit(
                tab,
                Edit::Insert {
                    text: String::new(),
                    caret_back: 0,
                },
            );
            done = true;
        }
        if item(ui, "Copy", "Ctrl+C", selected.is_some())
            && let Some(text) = &selected
        {
            ui.ctx().copy_text(text.clone());
            self.focus = true;
            done = true;
        }
        if item(ui, "Paste", "Ctrl+V", true) {
            // The editor takes focus again in the next frame, then receives the paste.
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
            self.focus = true;
            done = true;
        }
        if item(ui, "Select all", "Ctrl+A", true) {
            set_cursor_range(
                ui.ctx(),
                id,
                CCursorRange::two(
                    CCursor::new(body_start),
                    CCursor::new(tab.document.text.chars().count()),
                ),
            );
            self.focus = true;
            done = true;
        }
        ui.separator();
        for (label, shortcut, opening, closing) in FORMATS {
            if item(ui, label, shortcut, true) {
                self.edit(tab, Edit::Wrap(opening, closing));
                done = true;
            }
        }
        ui.separator();
        for level in 1..=3 {
            if item(ui, format!("Heading {level}").as_str(), "", true) {
                self.edit(tab, Edit::Heading(level));
                done = true;
            }
        }
        if let Some(text) = &selected {
            ui.separator();
            if item(ui, "Search for selection", "", true) {
                self.search_request = Some(text.split_whitespace().collect::<Vec<_>>().join(" "));
                done = true;
            }
        }
        if done {
            ui.close();
            ui.ctx().request_repaint();
        }
    }

    pub fn has_focus(&self, ctx: &egui::Context, id: egui::Id) -> bool {
        ctx.memory(|memory| memory.has_focus(id)) || self.tables.has_focus(ctx, id)
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        tab: &mut Tab,
        id: egui::Id,
        find: &mut Find,
        files: &mut FileMatcher,
        vault: Option<&rustidian::vault::Vault>,
    ) -> Option<String> {
        if let Some(vault) = vault {
            let mut local = TextEditState::load(ui.ctx(), id).unwrap_or_default();
            let shared = ui.ctx().data_mut(|data| {
                data.get_temp_mut_or_default::<Histories>(histories(&vault.root))
                    .entry(tab.document.path.clone())
                    .or_insert_with(|| local.clone())
                    .clone()
            });
            local.share_undoer_with(&shared);
            local.store(ui.ctx(), id);
        }
        if self.pending_edit.as_ref().is_some_and(|edit| {
            edit.tab != tab.id()
                || edit.path != tab.document.path
                || edit.source != tab.document.text
        }) {
            self.pending_edit = None;
            self.error = Some("This note changed before the edit was applied. Select the text or insertion point again.".into());
            ui.ctx().request_repaint();
        }
        if self.focus {
            // A closing modal remains the top interaction layer for one pass.
            // Keep the request until the editor can register its focus interest.
            if !ui.is_sizing_pass() && ui.memory(|memory| memory.allows_interaction(ui.layer_id()))
            {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
            ui.ctx().request_repaint();
        }
        let live = tab.view == rustidian::tab::View::Live;
        let callout_input = ui.input(|input| input.events.iter().any(|event| matches!(
            event,
            egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Cut | egui::Event::Ime(_)
        ) || matches!(event, egui::Event::Key { key, pressed: true, .. } if !matches!(key, Key::S | Key::Escape))));
        self.tables.begin(ui, tab, id, live);
        let mut callout_change = None;
        let mut follow = None;
        let header_end = if live {
            rustidian::metadata::frontmatter(&tab.document.text)
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
                if consume_action(ui, egui::Modifiers::COMMAND, &[Key::Enter]) {
                    follow = link_at(&tab.document.text, cursor);
                    action_handled = true;
                } else if let Some(completion) = completion
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
                            &rustidian::paths::wiki_destination(path),
                        );
                        set_cursor(ui.ctx(), id, caret);
                        tab.edited();
                        action_handled = true;
                    }
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
                if let Some(range) = rustidian::editor::markdown_enter(
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
                find.restore_since = None;
                let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
                state.cursor.set_char_range(Some(CCursorRange::two(
                    CCursor::new(range.start),
                    CCursor::new(range.end),
                )));
                state.store(ui.ctx(), id);
            } else {
                // Focus went elsewhere as the bar closed. Wait briefly for the
                // editor to take it, then drop the selection instead of
                // redrawing an idle window forever.
                let now = ui.input(|input| input.time);
                if now - *find.restore_since.get_or_insert(now) < 0.5 {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(16));
                } else {
                    find.restore = None;
                    find.restore_since = None;
                }
            }
        }
        if ui.memory(|m| m.has_focus(id)) {
            let pending = self.pending_edit.take();
            let shortcut = [
                (Key::B, egui::Modifiers::COMMAND, "**"),
                (Key::I, egui::Modifiers::COMMAND, "*"),
                (
                    Key::H,
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    "==",
                ),
            ]
            .into_iter()
            .find_map(|(key, modifiers, marker)| {
                ui.input_mut(|i| i.consume_key(modifiers, key))
                    .then_some((marker, marker))
            });
            let operation = pending.as_ref().map(|edit| &edit.operation);
            let markers = match operation {
                Some(Edit::Highlight(color)) => Some(color.markers()),
                Some(Edit::Wrap(opening, closing)) => Some((*opening, *closing)),
                Some(Edit::Insert { .. } | Edit::NativeHighlight(_) | Edit::Heading(_)) => None,
                None => shortcut,
            };
            if markers.is_some()
                || matches!(
                    operation,
                    Some(Edit::Insert { .. } | Edit::NativeHighlight(_) | Edit::Heading(_))
                )
            {
                let mut state = TextEditState::load(ui.ctx(), id).unwrap_or_default();
                if let Some(selection) = pending
                    .as_ref()
                    .and_then(|edit| edit.selection)
                    .or_else(|| state.cursor.char_range())
                {
                    let mut undoer = state.undoer();
                    undoer.add_undo(&(selection, tab.document.text.clone()));
                    let selection_range = selection.as_sorted_char_range();
                    let characters = selection_range.start.0..selection_range.end.0;
                    let range = if let Some(Edit::NativeHighlight(color)) = operation {
                        rustidian::highlight::toggle_native(
                            &mut tab.document.text,
                            characters,
                            *color,
                        )
                    } else if let Some(Edit::Insert { text, caret_back }) = operation {
                        let caret =
                            characters.start + text.chars().count().saturating_sub(*caret_back);
                        replace_chars(&mut tab.document.text, characters, text);
                        caret..caret
                    } else if let Some(Edit::Heading(level)) = operation {
                        toggle_heading(&mut tab.document.text, characters, *level)
                    } else {
                        let (opening, closing) = markers.unwrap();
                        wrap(&mut tab.document.text, characters, opening, closing)
                    };
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
        if self.source != tab.document.text {
            self.parsed = Markdown::parse(&tab.document.text);
            self.sections = rustidian::folding::Sections::new(&tab.document.text, &self.parsed);
            self.source.clone_from(&tab.document.text);
            self.source_version += 1;
        }
        let folds_id = id.with("editing-folds");
        let callouts_id = id.with("callout-folds");
        let mut callout_folds = ui
            .ctx()
            .data(|data| data.get_temp::<HashMap<String, bool>>(callouts_id))
            .unwrap_or_default();
        let reveal = jump.or_else(|| find.restore.as_ref().map(|range| range.start));
        if let Some(character) = reveal {
            reveal_callouts(
                &tab.document.text,
                &self.parsed,
                character,
                &mut callout_folds,
            );
        }
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
        // One walk over the text per cursor end; a caret needs only one.
        let byte_of = |character: usize| {
            tab.document
                .text
                .char_indices()
                .nth(character)
                .map_or(tab.document.text.len(), |(i, _)| i)
        };
        let cursor_bytes = cursor.map(|cursor| {
            let [first, last] = cursor.sorted_cursors();
            let first_byte = byte_of(first.index.0);
            let last_byte = if last.index == first.index {
                first_byte
            } else {
                byte_of(last.index.0)
            };
            (first_byte, last_byte)
        });
        let active_line = if live && ui.memory(|m| m.has_focus(id)) {
            cursor_bytes.map(|(first_byte, byte)| {
                let start = tab.document.text[..first_byte]
                    .rfind('\n')
                    .map_or(0, |i| i + 1);
                let end = tab.document.text[byte..]
                    .find('\n')
                    .map_or(tab.document.text.len(), |i| byte + i);
                start..end
            })
        } else {
            None
        };
        let caret = active_line.as_ref().and(cursor.zip(cursor_bytes)).and_then(
            |(cursor, (first_byte, _))| (cursor.primary == cursor.secondary).then_some(first_byte),
        );
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
        let layout_cache = &mut self.layout_cache;
        let source_version = self.source_version;
        let tables = &mut self.tables;
        let mut table_placements = Vec::new();
        let from = &tab.document.path;
        let plain_text = tab.document.is_plain_text();
        let mut images = if live {
            live_images(ui, source, vault, from, parsed, active_line.as_ref())
        } else {
            Vec::new()
        };
        retain_visible_images(&mut images, sections, &collapsed);
        let active_image = images.iter().any(|image| {
            active_line
                .as_ref()
                .is_some_and(|line| line.start < image.range.end && line.end >= image.range.start)
        });
        let mut updated_parsed = None;
        let mut layouter =
            |ui: &egui::Ui, buffer: &dyn egui::TextBuffer, width: f32| -> Arc<egui::Galley> {
                let text = buffer.as_str();
                if text == source && updated_parsed.take().is_some() && live {
                    images = live_images(ui, source, vault, from, parsed, active_line.as_ref());
                    retain_visible_images(&mut images, sections, &collapsed);
                }
                // An edit moves the caret to its end; the line there is the active one,
                // which the next frame's caret confirms. Using the pre-edit line would
                // style the new text as inactive for one frame and lay out twice.
                let active_line = match &active_line {
                    Some(line) if text != source => Some(edited_line(source, text, line)),
                    line => line.clone(),
                };
                let parsed = if text == source {
                    parsed
                } else {
                    updated_parsed = Some(Markdown::parse(text));
                    if live {
                        images = live_images(
                            ui,
                            text,
                            vault,
                            from,
                            updated_parsed.as_ref().unwrap(),
                            active_line.as_ref(),
                        );
                    }
                    updated_parsed.as_ref().unwrap()
                };
                let updated_sections;
                let current_sections = if text != source && !collapsed.is_empty() {
                    updated_sections = rustidian::folding::Sections::new(text, parsed);
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
                let mut hidden = current_sections.hidden(&collapsed);
                if live {
                    for (callout, key) in parsed.callouts.iter().zip(callout_keys(parsed)) {
                        if callout_folds
                            .get(&key)
                            .copied()
                            .unwrap_or(callout.folded.unwrap_or(false))
                            && !callout.body.is_empty()
                        {
                            let start = text[..callout.body.start].chars().count();
                            hidden.push(start..start + text[callout.body.clone()].chars().count());
                        }
                    }
                    for image in &images {
                        if let Some(block) = &image.block {
                            let start = (image.range.end + 1).min(block.end);
                            hidden.push(
                                text[..start].chars().count()..text[..block.end].chars().count(),
                            );
                        }
                    }
                    hidden.sort_by_key(|range| range.start);
                    let mut merged: Vec<Range<usize>> = Vec::new();
                    for range in hidden {
                        if let Some(previous) = merged
                            .last_mut()
                            .filter(|previous| previous.end >= range.start)
                        {
                            previous.end = previous.end.max(range.end);
                        } else {
                            merged.push(range);
                        }
                    }
                    hidden = merged;
                    images.retain(|image| {
                        let character = text[..image.range.start].chars().count();
                        !hidden.iter().any(|range| range.contains(&character))
                    });
                }
                table_placements = if live {
                    tables.prepare(
                        ui,
                        (&tab.document.path, text),
                        &parsed.tables,
                        active_line.as_ref(),
                        caret,
                        &hidden,
                    )
                } else {
                    Vec::new()
                };
                let key = LayoutKey {
                    version: if text == source {
                        source_version
                    } else {
                        source_version + 1
                    },
                    width,
                    live,
                    plain_text,
                    path: from.to_owned(),
                    active_line: active_line.clone(),
                    matched: selected_match.clone(),
                    images: images.clone(),
                    hidden,
                    tables: table_placements.clone(),
                    theme: ThemeKey::new(ui),
                    syntax: crate::syntax::generation(),
                    fonts: (
                        ui.fonts(|fonts| fonts.layout_generation()),
                        ui.ctx().pixels_per_point(),
                    ),
                };
                if text == source
                    && let Some(cached) = layout_cache.as_ref()
                    && cached.key == key
                {
                    return cached.galley.clone();
                }
                let hidden = &key.hidden;
                let job = if plain_text {
                    crate::syntax::source_job(ui, text, from, width, selected_match.as_ref())
                } else {
                    layout(
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
                    )
                };
                let previous = layout_cache
                    .as_ref()
                    .filter(|cached| cached.key.fonts == key.fonts)
                    .map(|cached| &cached.paragraphs);
                let (original, paragraphs) =
                    ui.fonts_mut(|fonts| fonts.layout_job_incremental(job, previous));
                let mut galley = original;
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
                    collapse_rows(&mut galley, hidden);
                }
                *layout_cache = Some(LayoutCache {
                    key,
                    paragraphs,
                    galley: galley.clone(),
                });
                galley
            };
        let text_height = layouter(ui, &tab.document.text, ui.available_width())
            .size()
            .y;
        let minimum_height = text_height + ui.clip_rect().height() / 2.0;
        let selection = ui.visuals().selection.bg_fill;
        let cursor_style = ui.visuals().text_cursor.clone();
        let code_background = ui.painter().add(egui::Shape::Noop);
        let callout_background = ui.painter().add(egui::Shape::Noop);
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
                    .font(FontId::new(
                        crate::theme::document_size(ui, 18.0),
                        crate::theme::fonts::text("regular"),
                    ))
                    .layouter(&mut layouter)
                    .event_filter(egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    })
                    .show(ui)
            })
            .inner;
        if bulk_edit && output.response.changed() {
            save_undo(ui.ctx(), &tab.document.text);
        }
        if self.focus
            && !ui.is_sizing_pass()
            && ui.memory(|memory| memory.allows_interaction(ui.layer_id()))
        {
            output.response.request_focus();
            // Window activation can follow later. Once the real editor owns
            // keyboard input, repeated requests would steal it from other fields.
            self.focus = !ui.memory(|memory| memory.has_focus(id));
            ui.ctx().request_repaint();
        }
        crate::ui_probe::record(ui.ctx(), "editor", output.response.rect);
        if crate::ui_probe::enabled() {
            crate::ui_probe::metric(
                ui.ctx(),
                "window-focused",
                f32::from(u8::from(ui.input(|input| input.focused))),
            );
            crate::ui_probe::metric(
                ui.ctx(),
                "editor-memory-focus",
                f32::from(u8::from(ui.memory(|memory| memory.has_focus(id)))),
            );
            crate::ui_probe::metric(
                ui.ctx(),
                "editor-focus-request",
                f32::from(u8::from(self.focus)),
            );
        }
        if output.response.has_focus() {
            crate::ui_probe::record(ui.ctx(), "editor-focused", output.response.rect);
            if let Some(range) = output.cursor_range {
                let caret = output.galley.pos_from_cursor(range.primary);
                crate::ui_probe::record(
                    ui.ctx(),
                    "editor-caret",
                    caret
                        .translate(output.galley_pos.to_vec2())
                        .expand2(egui::vec2(1.0, 0.0)),
                );
            }
        }
        if output.response.has_focus()
            && let Some(range) = output.cursor_range
        {
            self.selection = Some((tab.id(), range));
        }
        // A right-click on the selection keeps it for the menu (see hold_menu_click).
        if output.response.has_focus()
            && let Some(range) = output.cursor_range
            && range.primary.index != range.secondary.index
            && ui.memory(|memory| memory.allows_interaction(ui.layer_id()))
        {
            for rect in selection_rects(&output, range, &images, &tab.document.text) {
                menu_target(ui, rect, MenuArea::Editor);
            }
        }
        let mut menu = egui::Popup::context_menu(&output.response);
        if take_menu_click(ui.ctx(), MenuArea::Editor).is_some() {
            menu = menu.open_memory(Some(egui::SetOpenCommand::Bool(true)));
        }
        if let Some(menu) = menu.show(|ui| self.note_menu(ui, tab, id, body_start)) {
            crate::ui_probe::record(ui.ctx(), "editor-menu", menu.response.rect);
        }
        if header_end > 0 && !tab.document.text.starts_with(&self.source[..header_end]) {
            tab.document.text.clone_from(&self.source);
            self.focus_range = Some((id, CCursorRange::one(CCursor::new(body_start))));
            ui.ctx().request_repaint();
            images.clear();
            updated_parsed = None;
        }
        if let Some(parsed) = updated_parsed {
            self.sections = rustidian::folding::Sections::new(&tab.document.text, &parsed);
            self.parsed = parsed;
            self.source.clone_from(&tab.document.text);
            // The layouter's last call laid out this text under the next version.
            self.source_version += 1;
        }
        if output.response.changed() {
            tab.edited();
            ui.ctx().request_repaint();
        }
        if callout_input
            && output.response.has_focus()
            && let Some(cursor) = output.cursor_range
            && reveal_callouts(
                &tab.document.text,
                &self.parsed,
                cursor.primary.index.0,
                &mut callout_folds,
            )
        {
            ui.ctx().request_repaint();
        }
        let positioned = PositionedText::new(&output);
        paint_heading_folds(ui, &positioned, &self.sections, &mut collapsed);
        if live {
            let mut hidden = self.sections.hidden(&collapsed);
            for (callout, key) in self.parsed.callouts.iter().zip(callout_keys(&self.parsed)) {
                if callout_folds
                    .get(&key)
                    .copied()
                    .unwrap_or(callout.folded.unwrap_or(false))
                {
                    let start = tab.document.text[..callout.body.start].chars().count();
                    hidden.push(
                        start..start + tab.document.text[callout.body.clone()].chars().count(),
                    );
                }
            }
            let task_clip = output.text_clip_rect.intersect(ui.clip_rect());
            for (ordinal, task) in self.parsed.tasks.iter().enumerate() {
                let character = task.character;
                if hidden.iter().any(|range| range.contains(&character)) {
                    continue;
                }
                let point = positioned
                    .at(character)
                    .translate(output.galley_pos.to_vec2());
                let size = crate::theme::document_size(ui, 18.0);
                let rect = egui::Rect::from_center_size(
                    egui::pos2(point.min.x + size / 2.0, point.center().y),
                    egui::vec2(size, size),
                );
                if !rect.intersects(task_clip) {
                    continue;
                }
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt((id, "task", ordinal))
                        .max_rect(rect),
                );
                child.set_clip_rect(task_clip);
                let mut checked = task.checked;
                let response = crate::theme::checkbox(&mut child, &mut checked);
                crate::ui_probe::record(ui.ctx(), format!("task-live:{ordinal}"), response.rect);
                if response.clicked() {
                    crate::table_ui::checkpoint(ui.ctx(), id, &tab.document.text);
                    if rustidian::editor::toggle_task(&mut tab.document.text, ordinal) {
                        crate::table_ui::checkpoint(ui.ctx(), id, &tab.document.text);
                        ui.memory_mut(|memory| memory.request_focus(id));
                        tab.edited();
                        ui.ctx().request_repaint();
                    }
                }
            }
            if let Some(action) = paint_callouts(
                ui,
                &positioned,
                &self.source,
                &self.parsed,
                active_line.as_ref(),
                &mut callout_folds,
                callout_background,
            ) {
                match action {
                    CalloutAction::Follow(link) => follow = Some(link),
                    CalloutAction::Kind(ordinal, kind) => {
                        callout_change = Some((self.parsed.callouts[ordinal].clone(), kind));
                        ui.ctx().request_repaint();
                    }
                }
            }
            ui.ctx()
                .data_mut(|data| data.insert_temp(callouts_id, callout_folds));
        }
        ui.ctx()
            .data_mut(|data| data.insert_temp(folds_id, collapsed));
        paint_code_blocks(
            ui,
            &positioned,
            &self.source,
            self.parsed
                .code_blocks
                .iter()
                .enumerate()
                .filter(|(_, block)| {
                    !images
                        .iter()
                        .any(|image| image.block.as_ref() == Some(&block.range))
                }),
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
            let caret = if active || image.block.is_some() {
                positioned.at(character)
            } else {
                positioned.at_start(character)
            };
            let mut origin = output.galley_pos + caret.min.to_vec2();
            if image.block.is_some() && image.math.is_none() {
                origin.x = output.galley_pos.x;
            }
            let rect = egui::Rect::from_min_size(
                origin
                    + egui::vec2(
                        if active || image.block.is_some() && image.math.is_none() {
                            0.0
                        } else {
                            -image.size.x
                        },
                        if active {
                            29.16
                        } else if image.math.is_some() && image.block.is_none() {
                            (caret.height() - image.size.y).max(0.0) / 2.0
                        } else {
                            0.0
                        },
                    ),
                image.size,
            );
            ui.expand_to_include_rect(rect);
            let widget = egui::Image::new(&image.uri)
                .fit_to_exact_size(image.size)
                .sense(egui::Sense::click());
            let response = if let Some(math) = image.math {
                // Revealing one source removes its image. Keep the remaining
                // formulas' interaction identities stable across that change.
                ui.push_id((id, "live-math", math), |ui| ui.put(rect, widget))
                    .inner
            } else {
                ui.put(rect, widget)
            };
            crate::ui_probe::record(ui.ctx(), format!("embed:{ordinal}"), response.rect);
            if let Some(math) = image.math {
                crate::ui_probe::record(
                    ui.ctx(),
                    format!(
                        "math-live:{}:{math}",
                        if image.block.is_some() {
                            "display"
                        } else {
                            "inline"
                        }
                    ),
                    response.rect,
                );
            } else if image.block.is_some() {
                crate::ui_probe::record(ui.ctx(), "mermaid-live", response.rect);
            }
            if active {
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("embed-source:{ordinal}"),
                    egui::Rect::from_min_size(origin, egui::vec2(ui.available_width(), 29.16)),
                );
            }
            if response.clicked() {
                let source_range = image.block.as_ref().unwrap_or(&image.range);
                let start = tab.document.text[..source_range.start].chars().count();
                let end = tab.document.text[..source_range.end].chars().count();
                self.focus_range = Some((
                    id,
                    CCursorRange::two(CCursor::new(end), CCursor::new(start)),
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
        let pressed_button = ui.input(|input| {
            [egui::PointerButton::Primary, egui::PointerButton::Middle]
                .into_iter()
                .find(|button| input.pointer.button_pressed(*button))
        });
        if pressed_button.is_some() {
            self.pressed_link = None;
        }
        if let Some(pointer) = output.response.hover_pos() {
            let character = output
                .galley
                .cursor_from_pos(pointer - output.galley_pos)
                .index
                .0;
            let byte = tab
                .document
                .text
                .char_indices()
                .nth(character)
                .map_or(tab.document.text.len(), |(byte, _)| byte);
            let index = self
                .parsed
                .spans
                .partition_point(|span| span.range.end <= byte);
            let link =
                self.parsed.spans.get(index).is_some_and(|span| {
                    span.style.link && !span.style.code && !span.style.property
                });
            let rendered = live
                && !active_line
                    .as_ref()
                    .is_some_and(|line| line.contains(&byte));
            if link && (rendered || ui.input(|input| input.modifiers.command)) {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                if ui.input(|input| input.modifiers.command)
                    && let Some(target) = link_at(&tab.document.text, character)
                    && let Some(span) = self.parsed.spans.get(index)
                {
                    let start = tab.document.text[..span.range.start].chars().count();
                    let end = start + tab.document.text[span.range.clone()].chars().count();
                    let first = output.galley.pos_from_cursor(CCursor::new(start));
                    let last = output.galley.pos_from_cursor(CCursor::new(end));
                    let anchor = first.union(last).translate(output.galley_pos.to_vec2());
                    crate::page_preview::offer(ui, &tab.document.path, &target, anchor);
                }
                if let Some(button) = pressed_button
                    && let Some(target) = link_at(&tab.document.text, character)
                {
                    self.pressed_link = Some((tab.id(), button, target));
                }
            }
        }
        if self
            .pressed_link
            .as_ref()
            .is_some_and(|(_, button, _)| ui.input(|input| input.pointer.button_released(*button)))
            && let Some((tab_id, button, target)) = self.pressed_link.take()
            && tab_id == tab.id()
            && output.response.clicked_by(button)
        {
            follow = Some(target);
        }
        if crate::ui_probe::enabled() {
            // Table cells can edit the document after this frame's text layout.
            // Probe the same source that owns the parsed ranges and galley.
            let mut character = 0;
            for span in &self.parsed.spans {
                let length = self.source[span.range.clone()].chars().count();
                if span.style.link
                    && !span.style.hidden
                    && !span.style.code
                    && !span.style.property
                    && let Some(target) = link_at(&self.source, character)
                {
                    let first = output.galley.pos_from_cursor(CCursor::new(character));
                    let last = output
                        .galley
                        .pos_from_cursor(CCursor::new(character + length.min(3)));
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("editor-link:{target}"),
                        first.union(last).translate(output.galley_pos.to_vec2()),
                    );
                }
                character += length;
            }
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
                                    RichText::new(rustidian::paths::display_path(path)).size(14.0),
                                )
                                .selected(index == self.completion_selected)
                                .frame_when_inactive(false)
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
                    &rustidian::paths::wiki_destination(&path),
                );
                set_cursor(ui.ctx(), id, caret);
                ui.memory_mut(|m| m.request_focus(id));
                tab.edited();
            }
        }
        // Finish source-indexed overlays before changing a marker's byte length.
        if let Some((callout, kind)) = callout_change {
            crate::table_ui::checkpoint(ui.ctx(), id, &tab.document.text);
            if callout.change_kind(&mut tab.document.text, kind) {
                crate::table_ui::checkpoint(ui.ctx(), id, &tab.document.text);
                tab.edited();
                self.focus = true;
                ui.ctx().request_repaint();
            }
        }
        if let Some(vault) = vault
            && let Some(state) = TextEditState::load(ui.ctx(), id)
        {
            ui.ctx().data_mut(|data| {
                data.get_temp_mut_or_default::<Histories>(histories(&vault.root))
                    .insert(tab.document.path.clone(), state);
            });
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
    sections: &rustidian::folding::Sections,
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

fn callout_keys(parsed: &Markdown) -> Vec<String> {
    let mut occurrences = HashMap::new();
    parsed
        .callouts
        .iter()
        .map(|callout| {
            let identity = format!("{}\u{1f}{}", callout.identifier, callout.title);
            let count = occurrences.entry(identity.clone()).or_insert(0);
            let key = format!("{identity}\u{1f}{count}");
            *count += 1;
            key
        })
        .collect()
}

fn reveal_callouts(
    source: &str,
    parsed: &Markdown,
    character: usize,
    folded: &mut HashMap<String, bool>,
) -> bool {
    let mut changed = false;
    for (callout, key) in parsed.callouts.iter().zip(callout_keys(parsed)) {
        let start = source[..callout.body.start].chars().count();
        let end = start + source[callout.body.clone()].chars().count();
        if ((start..end).contains(&character)
            || start < end && character == end && end == source.chars().count())
            && folded
                .get(&key)
                .copied()
                .unwrap_or(callout.folded.unwrap_or(false))
        {
            folded.insert(key, false);
            changed = true;
        }
    }
    changed
}

enum CalloutAction {
    Follow(String),
    Kind(usize, rustidian::callouts::Kind),
}

#[allow(clippy::too_many_arguments)]
fn paint_callouts(
    ui: &mut egui::Ui,
    positioned: &PositionedText<'_>,
    source: &str,
    parsed: &Markdown,
    active_line: Option<&Range<usize>>,
    folded: &mut HashMap<String, bool>,
    background: egui::layers::ShapeIdx,
) -> Option<CalloutAction> {
    let output = positioned.output;
    let mut shapes = Vec::new();
    let mut action = None;
    for (ordinal, (callout, key)) in parsed.callouts.iter().zip(callout_keys(parsed)).enumerate() {
        let start = source[..callout.header.start].chars().count();
        let first = positioned.at(start).translate(output.galley_pos.to_vec2());
        if first.height() <= 0.0 {
            continue;
        }
        let last_byte = callout.range.start
            + source[callout.range.clone()]
                .trim_end_matches(['\r', '\n'])
                .len();
        let end = source[..last_byte].chars().count();
        let last = positioned.at(end).translate(output.galley_pos.to_vec2());
        let depth = parsed
            .callouts
            .iter()
            .filter(|parent| {
                parent.range.start < callout.range.start
                    && parent.range.contains(&callout.range.start)
            })
            .count();
        let rect = egui::Rect::from_min_max(
            egui::pos2(
                output.response.rect.left() + depth as f32 * 24.0,
                first.top(),
            ),
            egui::pos2(
                output.response.rect.right(),
                last.bottom().max(first.bottom()),
            ),
        );
        if !rect.intersects(ui.clip_rect()) {
            continue;
        }
        shapes.push(egui::Shape::rect_filled(
            rect,
            7,
            crate::callout_ui::background(ui, callout.kind),
        ));
        let collapsed = folded
            .get(&key)
            .copied()
            .unwrap_or(callout.folded.unwrap_or(false));
        if !collapsed && !callout.body.is_empty() {
            let body = positioned
                .at(source[..callout.body.start].chars().count())
                .translate(output.galley_pos.to_vec2());
            crate::ui_probe::record(
                ui.ctx(),
                format!("callout-body:{ordinal}"),
                egui::Rect::from_min_max(egui::pos2(rect.left(), body.top()), rect.max),
            );
        }
        let active = active_line.is_some_and(|line| {
            line.start < callout.header.end && line.end >= callout.header.start
        });
        if active {
            continue;
        }
        let header_rect = egui::Rect::from_min_max(
            rect.min + egui::vec2(24.0, 12.0),
            egui::pos2(rect.right() - 12.0, first.bottom() - 12.0),
        );
        ui.scope_builder(
            egui::UiBuilder::new()
                .id_salt(("live-callout", ordinal))
                .max_rect(header_rect),
            |ui| {
                let (header, link) = crate::callout_ui::header(ui, callout, ordinal, collapsed);
                if let Some(link) = link {
                    action = Some(CalloutAction::Follow(link));
                } else if header.clicked() && callout.folded.is_some() {
                    folded.insert(key, !collapsed);
                    set_cursor(ui.ctx(), output.response.id, start);
                    ui.memory_mut(|memory| memory.surrender_focus(output.response.id));
                    ui.ctx().request_repaint();
                }
                if header.double_clicked() {
                    set_cursor(
                        ui.ctx(),
                        output.response.id,
                        source[..callout.marker.end].chars().count(),
                    );
                    ui.memory_mut(|memory| memory.request_focus(output.response.id));
                    ui.ctx().request_repaint();
                }
                if header.secondary_clicked() {
                    ui.memory_mut(|memory| memory.surrender_focus(output.response.id));
                }
                header.context_menu(|ui| {
                    crate::theme::menu_style(ui);
                    for kind in rustidian::callouts::Kind::ALL {
                        let response = ui.selectable_label(kind == callout.kind, kind.identifier());
                        crate::ui_probe::record(
                            ui.ctx(),
                            format!("callout-type:{}", kind.identifier()),
                            response.rect,
                        );
                        if response.clicked() {
                            action = Some(CalloutAction::Kind(ordinal, kind));
                            ui.close();
                        }
                    }
                });
            },
        );
    }
    ui.painter().set(background, egui::Shape::Vec(shapes));
    action
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
    sections: &rustidian::folding::Sections,
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

fn paint_code_blocks<'a>(
    ui: &egui::Ui,
    positioned: &PositionedText<'_>,
    source: &str,
    blocks: impl Iterator<Item = (usize, &'a rustidian::editor::CodeBlock)>,
    live: bool,
    active_line: Option<&Range<usize>>,
    background: egui::layers::ShapeIdx,
) {
    let output = positioned.output;
    let mut shapes = Vec::new();
    let mut previous_byte = 0;
    let mut character = 0;
    for (ordinal, block) in blocks {
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
        if parsed
            .callouts
            .iter()
            .any(|callout| callout.range.contains(&quote.range.end))
        {
            continue;
        }
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
    let shapes = selection_rects(output, range, images, source)
        .into_iter()
        .map(|rect| egui::Shape::rect_filled(rect, 0.0, color))
        .collect();
    ui.painter().set(shape, egui::Shape::Vec(shapes));
}

/// The painted rectangles of `range`, row by row, in screen coordinates.
fn selection_rects(
    output: &egui::text_edit::TextEditOutput,
    range: CCursorRange,
    images: &[LiveImage],
    source: &str,
) -> Vec<egui::Rect> {
    let [start, end] = range.sorted_cursors();
    let mut character = 0;
    let mut selected = Vec::new();
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
                selected.push(
                    rect.translate(output.galley_pos.to_vec2() + row.pos.to_vec2())
                        .intersect(output.text_clip_rect),
                );
            }
        }
        character = row_end + usize::from(row.ends_with_newline);
    }
    selected
}

/// The line holding the end of the change from `before` to `after`.
fn edited_line(before: &str, after: &str, previous: &Range<usize>) -> Range<usize> {
    let mut prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !after.is_char_boundary(prefix) {
        prefix -= 1;
    }
    let suffix = before[prefix..]
        .bytes()
        .rev()
        .zip(after[prefix..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut end = after.len() - suffix;
    while !after.is_char_boundary(end) {
        end += 1;
    }
    if before.len() == after.len() && prefix == after.len() {
        return previous.clone();
    }
    // An insertion that repeats what precedes it (Enter at the end of a line
    // adds a newline before the existing one) happened at the earlier place.
    if prefix + suffix == before.len() {
        while prefix > 0
            && end > prefix
            && after.as_bytes()[prefix - 1] == after.as_bytes()[end - 1]
            && after.is_char_boundary(prefix - 1)
            && after.is_char_boundary(end - 1)
        {
            prefix -= 1;
            end -= 1;
        }
    }
    let start = after[..end].rfind('\n').map_or(0, |i| i + 1);
    let end = after[end..].find('\n').map_or(after.len(), |i| end + i);
    start..end
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
    let font_scale = crate::theme::document_size(ui, 1.0);
    let colors = crate::syntax::highlights(ui, text, parsed);
    let callout_heights: Vec<_> = if live {
        parsed
            .callouts
            .iter()
            .map(|callout| {
                let title = crate::preview::TableCell::new(&callout.title);
                let depth = parsed
                    .callouts
                    .iter()
                    .filter(|parent| {
                        parent.range.start < callout.range.start
                            && parent.range.contains(&callout.range.start)
                    })
                    .count();
                let header_width = width - 24.0 * depth as f32 - 36.0;
                let height = title
                    .layout(ui, true, crate::callout_ui::title_width(header_width))
                    .size()
                    .y
                    + 24.0;
                (callout.header.clone(), height)
            })
            .collect()
    } else {
        Vec::new()
    };
    let bold = crate::theme::fonts::text("bold");
    let semibold = crate::theme::fonts::text("semibold");
    let italic = crate::theme::fonts::text("italic");
    let mut character = 0;
    // Spans and code blocks are both in text order: walk the blocks once.
    let mut code_index = 0;
    for span in &parsed.spans {
        // Split at line boundaries so a multiline emphasis doesn't reveal all its markers.
        let mut start = span.range.start;
        for section in text[span.range.clone()].split_inclusive('\n') {
            let end = start + section.len();
            let style = span.style;
            let active = active_line.is_some_and(|line| start <= line.end && end > line.start);
            while parsed
                .code_blocks
                .get(code_index)
                .is_some_and(|block| block.range.end <= start)
            {
                code_index += 1;
            }
            let code_block = parsed
                .code_blocks
                .get(code_index)
                .filter(|block| block.range.contains(&start));
            let code_active = code_block.is_some_and(|block| {
                active_line.is_some_and(|line| {
                    line.start < block.range.end && line.end >= block.range.start
                })
            });
            let image = images.iter().find(|image| {
                let content_end = if image.math.is_some() && image.block.is_some() {
                    start + section.trim_end_matches(['\r', '\n']).len()
                } else {
                    end
                };
                start >= image.range.start && content_end <= image.range.end
            });
            let hidden = live
                && (style.task_marker
                    || style.property
                    || style.hidden && !active && !style.image
                    || style.quote_marker && !active
                    || style.fence && !code_active
                    || style.callout_header && !active
                    || image.is_some() && !active);
            let size = if live && style.task_marker {
                12.0
            } else if hidden {
                0.01
            } else if style.heading > 0 {
                crate::theme::prose_size(ui, style.heading) / font_scale
            } else if style.block_code {
                crate::code_ui::FONT_SIZE
            } else if style.code {
                16.0
            } else {
                18.0
            };
            let size = size * font_scale;
            let family = if style.code || style.math {
                FontFamily::Monospace
            } else if style.heading == 1 || style.html.bold == Some(true) {
                bold.clone()
            } else if style.html.bold.unwrap_or(style.strong) {
                semibold.clone()
            } else if style.html.italic.unwrap_or(style.italic) {
                italic.clone()
            } else {
                crate::theme::fonts::text("regular")
            };
            let color = if hidden {
                Color32::TRANSPARENT
            } else if style.marker {
                crate::theme::marker(ui)
            } else if let Some([red, green, blue, alpha]) = style.html.color {
                Color32::from_rgba_unmultiplied(red, green, blue, alpha)
            } else if style.link {
                crate::theme::accent(ui)
            } else if style.heading > 0 {
                crate::theme::heading(ui, style.heading)
            } else if style.block_code || (live && style.task_checked) {
                ui.visuals().weak_text_color()
            } else {
                ui.visuals().text_color()
            };
            let count = if matched.is_some() {
                section.chars().count()
            } else {
                0
            };
            let highlighted = matched
                .is_some_and(|range| character < range.end && character + count > range.start);
            let display = if live && style.bullet && !active && !style.task_marker {
                "•"
            } else {
                section
            };
            let quote_indent = if live && !active && style.quote_marker {
                parsed
                    .quotes
                    .binary_search_by_key(&start, |quote| quote.range.start)
                    .map_or(0.0, |index| {
                        let callout_depth = parsed
                            .callouts
                            .iter()
                            .filter(|callout| {
                                callout.range.contains(&start) || callout.range.contains(&end)
                            })
                            .count();
                        17.0 * parsed.quotes[index].depth as f32 + 7.0 * callout_depth as f32
                    })
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
                line_height: Some(if live && style.callout_header && !active {
                    callout_heights
                        .iter()
                        .find(|(range, _)| range.contains(&start))
                        .map_or_else(|| 29.16 * font_scale, |(_, height)| *height)
                } else if live && style.property {
                    0.01
                } else if let Some(image) = image.filter(|_| !active) {
                    if image.math.is_some() && image.block.is_none() {
                        image.size.y.max(29.16 * font_scale)
                    } else {
                        image.size.y + 16.0
                    }
                } else if style.block_code {
                    crate::code_ui::LINE_HEIGHT * font_scale
                } else if section == "\n" {
                    29.16 * font_scale
                } else if style.heading > 0 {
                    size.max(24.0 * font_scale) * 1.2
                } else {
                    29.16 * font_scale
                }),
                valign: if image.is_some() && active {
                    egui::Align::Min
                } else {
                    egui::Align::Center
                },
                italics: style.html.italic.unwrap_or(style.italic),
                extra_letter_spacing: if style.heading > 0 {
                    -0.025 * size
                } else {
                    0.0
                },
                strikethrough: if style.html.strike.unwrap_or(style.strike)
                    || (live && style.task_checked)
                {
                    Stroke::new(1.0, color)
                } else {
                    Stroke::NONE
                },
                underline: if live
                    && style.html.underline.unwrap_or(style.link)
                    && !style.marker
                    && !hidden
                {
                    Stroke::new(1.0, color)
                } else {
                    Stroke::NONE
                },
                background: if let Some([red, green, blue, alpha]) = style
                    .native_highlight
                    .map(|color| color.rgba(ui.visuals().dark_mode))
                    .or(style.highlight)
                    .filter(|_| live && !hidden)
                {
                    Color32::from_rgba_unmultiplied(red, green, blue, alpha)
                } else if highlighted {
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
    #[test]
    fn heading_menu_toggles_the_lines_of_the_selection() {
        // (text, selection, level) -> (text, selection), selections in characters.
        for (text, selection, level, expected, expected_selection) in [
            ("Title", 2..2, 1, "# Title", 4..4),
            ("# Title", 3..3, 1, "Title", 1..1),
            ("## Title", 5..5, 1, "# Title", 4..4),
            ("a\n\nb", 0..4, 2, "## a\n\n## b", 3..10),
            ("سلام", 4..4, 3, "### سلام", 8..8),
            ("# a\n# b", 0..7, 1, "a\nb", 0..3),
            ("# a\nb", 0..5, 1, "# a\n# b", 2..7),
            ("intro\nTitle\nend", 8..8, 2, "intro\n## Title\nend", 11..11),
            ("", 0..0, 1, "# ", 2..2),
        ] {
            let mut edited = text.to_owned();
            let selection = super::toggle_heading(&mut edited, selection, level);
            assert_eq!(
                (edited.as_str(), selection),
                (expected, expected_selection),
                "{text:?}"
            );
        }
    }

    #[test]
    fn edited_line_is_the_line_where_the_change_ends() {
        let before = "one\ntwo\nthree";
        let previous = 4..7;
        assert_eq!(
            super::edited_line(before, "one\ntwo!\nthree", &previous),
            4..8
        );
        assert_eq!(
            super::edited_line(before, "one\ntw\nthree", &previous),
            4..6
        );
        assert_eq!(
            super::edited_line(before, "one\ntwo\n\nthree", &previous),
            8..8
        );
        assert_eq!(
            super::edited_line(before, "one\ntwoo\nthree", &previous),
            4..8
        );
        assert_eq!(super::edited_line(before, "one\nthree", &previous), 4..9);
        assert_eq!(
            super::edited_line(before, "one\ntwo\nthree πx", &previous),
            8..17
        );
        assert_eq!(
            super::edited_line(before, "πne\ntwo\nthree", &previous),
            0..4
        );
        assert_eq!(super::edited_line(before, before, &previous), previous);
    }

    /// The paragraph-reusing layout must equal egui's own layout of the same
    /// job after any edit, and must reuse every unchanged paragraph.
    #[test]
    fn incremental_layout_matches_full_layout_across_random_edits() {
        use egui::text::{LayoutJob, TextFormat};
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let formats = [
            TextFormat {
                font_id: FontId::new(14.0, FontFamily::Proportional),
                ..Default::default()
            },
            TextFormat {
                font_id: FontId::new(12.0, FontFamily::Monospace),
                color: Color32::RED,
                ..Default::default()
            },
            TextFormat {
                font_id: FontId::new(18.0, FontFamily::Name("bold".into())),
                italics: true,
                underline: Stroke::new(1.0, Color32::GREEN),
                line_height: Some(30.0),
                ..Default::default()
            },
        ];
        let job_of = |pieces: &[(String, TextFormat)], width: f32| {
            let mut job = LayoutJob {
                first_row_min_height: 20.0,
                ..Default::default()
            };
            job.wrap.max_width = width;
            for (text, format) in pieces {
                job.append(text, 0.0, format.clone());
            }
            job
        };
        let alphabet: Vec<char> = "ab c\nd\nπ e".chars().collect();
        let mut rng = fastrand::Rng::with_seed(20261027);
        let mut pieces: Vec<(String, TextFormat)> = (0..12)
            .map(|i| {
                (
                    format!("piece {i} with some words\n"),
                    formats[i % formats.len()].clone(),
                )
            })
            .collect();
        let mut previous: Option<egui::text::ParagraphLayout> = None;
        for round in 0..60 {
            for _ in 0..rng.usize(1..4) {
                let index = rng.usize(..pieces.len());
                match rng.usize(..3) {
                    0 => {
                        let text = &mut pieces[index].0;
                        let offsets: Vec<_> = text.char_indices().map(|(i, _)| i).collect();
                        let at = offsets
                            .get(rng.usize(..offsets.len() + 1))
                            .copied()
                            .unwrap_or(text.len());
                        text.insert(at, alphabet[rng.usize(..alphabet.len())]);
                    }
                    1 => {
                        let text = &mut pieces[index].0;
                        let count = text.chars().count();
                        if let Some((at, c)) = text.char_indices().nth(rng.usize(..count.max(1))) {
                            text.replace_range(at..at + c.len_utf8(), "");
                        }
                    }
                    _ => pieces[index].1 = formats[rng.usize(..formats.len())].clone(),
                }
            }
            let width = if round % 2 == 0 { 120.0 } else { f32::INFINITY };
            let job = job_of(&pieces, width);
            let full = ctx.fonts_mut(|fonts| fonts.layout_job(job.clone()));
            let (incremental, layout) =
                ctx.fonts_mut(|fonts| fonts.layout_job_incremental(job, previous.as_ref()));
            assert_eq!(
                format!("{incremental:#?}"),
                format!("{full:#?}"),
                "round {round}"
            );
            previous = Some(layout);
        }
        // One edited single-line piece lays out one paragraph; the rest keep their galleys.
        let pieces: Vec<(String, TextFormat)> = (0..12)
            .map(|i| (format!("piece {i}\n"), formats[i % formats.len()].clone()))
            .collect();
        let (_, first) =
            ctx.fonts_mut(|fonts| fonts.layout_job_incremental(job_of(&pieces, 120.0), None));
        let mut edited = pieces.clone();
        edited[5].0.insert(7, 'x');
        let (_, second) = ctx
            .fonts_mut(|fonts| fonts.layout_job_incremental(job_of(&edited, 120.0), Some(&first)));
        for (index, (old, new)) in first.galleys().iter().zip(second.galleys()).enumerate() {
            assert_eq!(Arc::ptr_eq(old, new), index != 5, "paragraph {index}");
        }
        edited[8].1 = formats[0].clone();
        let (_, third) = ctx
            .fonts_mut(|fonts| fonts.layout_job_incremental(job_of(&edited, 120.0), Some(&second)));
        for (index, (old, new)) in second.galleys().iter().zip(third.galleys()).enumerate() {
            assert_eq!(Arc::ptr_eq(old, new), index != 8, "paragraph {index}");
        }
    }

    #[test]
    fn external_document_edit_replaces_a_real_widget_redo_branch() {
        use rustidian::vault::Document;
        let ctx = egui::Context::default();
        let root = std::path::Path::new("/disposable-agent-history");
        let id = egui::Id::unique("agent-history-widget");
        let mut document = Document::new("Note.md".into());
        document.text = "Original note".into();
        document_history(&ctx, root, &document.path).store(&ctx, id);
        let frame = |document: &mut Document, time, events: Vec<egui::Event>| {
            ctx.memory_mut(|memory| memory.request_focus(id));
            let events = std::iter::once(egui::Event::ModifiersChanged(
                egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
            ))
            .chain(events)
            .collect();
            ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.add(
                        TextEdit::multiline(&mut document.text)
                            .id(id)
                            .desired_width(400.0),
                    );
                },
            )
            .drop_without_applying_deltas();
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        };
        frame(&mut document, 0.0, vec![]);
        frame(
            &mut document,
            0.1,
            vec![egui::Event::Paste("Prior edit ".into())],
        );
        assert_ne!(document.text, "Original note");
        frame(&mut document, 0.2, vec![key(Key::Z)]);
        assert_eq!(document.text, "Original note");
        let before = document.clone();
        document.text = "Agent replacement".into();
        record_document_edit(&ctx, root, &before, &document);
        frame(&mut document, 0.3, vec![key(Key::Y)]);
        assert_eq!(
            document.text, "Agent replacement",
            "Applying an external edit must discard the old Redo branch"
        );
        frame(&mut document, 0.4, vec![key(Key::Z)]);
        assert_eq!(document.text, "Original note");
        frame(&mut document, 0.5, vec![key(Key::Y)]);
        assert_eq!(document.text, "Agent replacement");
        frame(&mut document, 0.6, vec![key(Key::Y)]);
        assert_eq!(
            document.text, "Agent replacement",
            "Extra Redo must never resurrect the earlier discarded edit"
        );
    }

    #[test]
    fn inactive_display_math_hides_opening_delimiters_and_retains_source_positions() {
        let source = "Before\n\n$$\nx^2\n$$\n\nAfter";
        let parsed = Markdown::parse(source);
        let image = LiveImage {
            range: 8..10,
            block: Some(8..17),
            uri: "bytes://math-layout-test.svg".into(),
            size: egui::vec2(80.0, 40.0),
            math: Some(0),
        };
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let job = layout(
                ui,
                source,
                &parsed,
                920.0,
                Decorations {
                    live: true,
                    active_line: None,
                    matched: None,
                    images: std::slice::from_ref(&image),
                },
            );
            let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            assert_eq!(galley.text(), source);
            assert_eq!(galley.end().index.0, source.chars().count());
            let opening = galley.layout_from_cursor(CCursor::new(8));
            assert!(
                galley.rows[opening.row].visuals.mesh.is_empty(),
                "Inactive display math must not paint its opening dollars under the formula"
            );
            assert!(galley.rows[opening.row].height() >= 40.0);
            let after = galley.layout_from_cursor(CCursor::new(19));
            assert!(
                !galley.rows[after.row].visuals.mesh.is_empty(),
                "Hiding formula source must preserve the following prose"
            );
        })
        .drop_without_applying_deltas();
    }

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
        let sections = rustidian::folding::Sections::new(source, &Markdown::parse(source));
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
            available: 920.0,
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
            crate::theme::document_style(ui, crate::theme::DEFAULT_DOCUMENT_FONT_SIZE);
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
    fn an_inline_image_keeps_its_reserved_width_when_the_row_wraps_before_it() {
        // Live preview reserves an inline formula's width as the leading space
        // of its hidden source. A soft wrap just before it must keep that space.
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let format = egui::TextFormat::simple(FontId::proportional(18.0), Color32::WHITE);
            let mut wrapped = 0;
            for width in (120..400).map(|width| width as f32) {
                let mut job = egui::text::LayoutJob::default();
                job.wrap.max_width = width;
                job.append("and the weights sum to ", 0.0, format.clone());
                job.append("$1$", 24.0, format.clone());
                job.append(":", 0.0, format.clone());
                let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                let mut start = 0;
                for row in &galley.rows {
                    let count = row.char_count_excluding_newline().0;
                    if start == 23 && count > 0 {
                        wrapped += 1;
                        assert!(
                            (row.pos.x + row.glyphs[0].pos.x - 24.0).abs() < 0.5,
                            "width={width}: the formula's space must open its row, row={:?}",
                            row.rect()
                        );
                    }
                    start += count;
                }
            }
            assert!(wrapped > 0, "Some width must wrap just before the formula");
        })
        .drop_without_applying_deltas();
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

    #[test]
    fn a_find_selection_stops_waiting_for_an_editor_without_focus() {
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut document = rustidian::vault::Document::new("Note.md".into());
        document.text = "alpha beta".into();
        let mut tab = Tab::new(document);
        // The bar closed on "beta" while focus went elsewhere.
        let mut find = Find {
            restore: Some(6..10),
            ..Default::default()
        };
        let mut editor = Editor::default();
        let mut files = FileMatcher::new(Vec::new());
        let id = egui::Id::new("note-editor");
        let mut waiting = Vec::new();
        for frame in 0..8 {
            ctx.run_ui(
                egui::RawInput {
                    time: Some(f64::from(frame) * 0.1),
                    ..Default::default()
                },
                |ui| {
                    editor.show(ui, &mut tab, id, &mut find, &mut files, None);
                },
            )
            .drop_without_applying_deltas();
            waiting.push(find.restore.is_some());
        }
        assert!(
            waiting[0],
            "The selection waits a moment for the editor's focus"
        );
        assert!(!waiting[7], "The wait for the editor's focus must end");
    }
}
