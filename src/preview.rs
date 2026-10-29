use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::{
    cell::RefCell,
    sync::{Arc, Weak},
};

#[derive(Clone, Default)]
struct Inline {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    highlight: Option<rustidian::highlight::Rgba>,
    native_highlight: Option<rustidian::highlight::NativeColor>,
    html: rustidian::inline_style::Style,
    link: Option<String>,
    math: Option<rustidian::math::Span>,
}
#[derive(Clone, Default)]
struct Block {
    parts: Vec<Inline>,
    heading: u8,
    quote: bool,
    code: Option<Code>,
    math: Option<Inline>,
    table: Option<Table>,
    rule: bool,
    image: Option<String>,
    task: Option<(usize, bool)>,
    list_depth: usize,
    loose_item: bool,
    callout: Option<CalloutBlock>,
    ordinal: usize,
}
#[derive(Clone)]
struct CalloutBlock {
    model: rustidian::callouts::Callout,
    children: Vec<Block>,
    ordinal: usize,
}
struct CalloutFrame {
    model: rustidian::callouts::Callout,
    parents: Vec<Block>,
    ordinal: usize,
}
#[derive(Clone, Default)]
struct Code {
    language: String,
    body: String,
}
#[derive(Clone, Default)]
struct Table {
    alignments: Vec<pulldown_cmark::Alignment>,
    rows: Vec<Vec<Vec<Inline>>>,
}

/// Share the reading renderer's inline styles with editable table cells.
pub(crate) struct TableCell {
    parts: Vec<Inline>,
    painted: RefCell<Option<(Weak<egui::Galley>, Vec<crate::math_inline::Placement>)>>,
}

impl TableCell {
    pub fn new(source: &str) -> Self {
        let (prepared, _) = prepare(source);
        let parsed = Parsed::new(&prepared);
        Self {
            parts: parsed
                .blocks
                .into_iter()
                .flat_map(|block| block.math.into_iter().chain(block.parts))
                .collect(),
            painted: RefCell::new(None),
        }
    }

    pub fn layout(&self, ui: &egui::Ui, header: bool, width: f32) -> Arc<egui::Galley> {
        self.layout_color(ui, header, width, None)
    }

    pub fn layout_color(
        &self,
        ui: &egui::Ui,
        header: bool,
        width: f32,
        color: Option<egui::Color32>,
    ) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = width;
        job.first_row_min_height = crate::theme::document_size(ui, 23.4);
        let mut math = Vec::new();
        for part in &self.parts {
            let mut format = inline_format(ui, part, 0, Some(header));
            if let Some(color) = color {
                format.color = color;
            }
            if let Some(formula) = &part.math {
                if let Some(placement) = crate::math_inline::append(
                    ui,
                    &mut job,
                    &part.text,
                    &formula.formula,
                    formula.display,
                    format,
                    0.0,
                ) {
                    math.push(placement);
                }
            } else {
                job.append(&part.text, 0.0, format);
            }
        }
        if job.sections.is_empty() {
            job.append(
                "",
                0.0,
                inline_format(ui, &Inline::default(), 0, Some(header)),
            );
        }
        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        *self.painted.borrow_mut() = Some((Arc::downgrade(&galley), math));
        galley
    }

    pub fn paint(&self, ui: &egui::Ui, galley: &Arc<egui::Galley>, origin: egui::Pos2) {
        if let Some((owner, placements)) = &*self.painted.borrow()
            && owner.ptr_eq(&Arc::downgrade(galley))
        {
            for placement in placements {
                crate::math_inline::paint(ui, galley, origin, placement);
            }
        }
    }

    pub fn link_at(&self, character: usize) -> Option<&str> {
        let mut offset = 0;
        self.parts.iter().find_map(|part| {
            let end = offset + part.text.chars().count();
            let hit = (offset..end).contains(&character);
            offset = end;
            hit.then_some(part.link.as_deref()).flatten()
        })
    }
}
#[derive(Clone, Default)]
struct Parsed {
    source: String,
    blocks: Vec<Block>,
    fallback: bool,
}

impl Parsed {
    fn new(source: &str) -> Self {
        let mut parsed = Self {
            source: source.into(),
            ..Self::default()
        };
        let mut marked = source.to_owned();
        let math = rustidian::math::spans(source);
        if source.contains("==") {
            for range in rustidian::editor::Markdown::parse(source)
                .highlights
                .into_iter()
                .filter(|range| !math.iter().any(|math| ranges_overlap(range, &math.range)))
                .rev()
            {
                let native = rustidian::highlight::NativeColor::prefix(
                    &source[range.start + 2..range.end - 2],
                );
                marked.replace_range(range.end - 2..range.end, "</mark>");
                marked.replace_range(
                    range.start..range.start + 2 + native.map_or(0, |(_, size)| size),
                    native.map_or("<mark>", |(color, _)| color.tag()),
                );
            }
        }
        let source = marked.as_str();
        let math = rustidian::math::spans(source);
        let mut block = Block::default();
        let mut style = Inline::default();
        let mut highlights = Vec::new();
        let mut list: Vec<Option<u64>> = Vec::new();
        let mut task_count = 0;
        let mut quote = 0;
        let mut plain_quotes = 0;
        let mut quote_frames: Vec<Option<CalloutFrame>> = Vec::new();
        let mut callout_ordinal = 0;
        let mut code_ordinal = 0;
        let mut table_ordinal = 0;
        let mut original_code = None;
        let mut table_row = Vec::new();
        let flush = |block: &mut Block, blocks: &mut Vec<Block>| {
            if !block.parts.is_empty()
                || block.rule
                || block.image.is_some()
                || block.task.is_some()
                || block.code.is_some()
                || block.math.is_some()
                || block.table.is_some()
                || block.callout.is_some()
            {
                blocks.push(std::mem::take(block));
            }
        };
        for (event, range) in Parser::new_ext(
            source,
            Options::ENABLE_STRIKETHROUGH
                | Options::ENABLE_TABLES
                | Options::ENABLE_TASKLISTS
                | Options::ENABLE_MATH,
        )
        .into_offset_iter()
        {
            if quote_frames
                .iter()
                .rev()
                .flatten()
                .next()
                .is_some_and(|frame| {
                    range.start >= frame.model.marker.start && range.end <= frame.model.header.end
                })
                && matches!(
                    event,
                    Event::Text(_)
                        | Event::Code(_)
                        | Event::InlineMath(_)
                        | Event::DisplayMath(_)
                        | Event::SoftBreak
                        | Event::HardBreak
                        | Event::Start(
                            Tag::Strong | Tag::Emphasis | Tag::Strikethrough | Tag::Link { .. }
                        )
                        | Event::End(
                            TagEnd::Strong
                                | TagEnd::Emphasis
                                | TagEnd::Strikethrough
                                | TagEnd::Link
                        )
                )
            {
                continue;
            }
            match event {
                Event::TaskListMarker(checked) => {
                    block.task = Some((task_count, checked));
                    task_count += 1;
                    block.parts.pop();
                }
                Event::Start(Tag::Image { dest_url, .. }) => {
                    if block.table.is_some() {
                        parsed.fallback = true;
                    }
                    flush(&mut block, &mut parsed.blocks);
                    block.image = Some(dest_url.into_string());
                }
                Event::End(TagEnd::Image) => flush(&mut block, &mut parsed.blocks),
                Event::Start(Tag::Heading { level, .. }) => block.heading = level as u8,
                Event::Start(Tag::CodeBlock(kind)) => {
                    flush(&mut block, &mut parsed.blocks);
                    block.ordinal = code_ordinal;
                    code_ordinal += 1;
                    let language = match kind {
                        CodeBlockKind::Fenced(language) => {
                            if quote == 0 && list.is_empty() {
                                original_code = rustidian::editor::CodeBlock::fenced(
                                    source,
                                    range,
                                    language.to_string(),
                                )
                                .content(source)
                                .map(str::to_owned);
                            }
                            language.into_string()
                        }
                        CodeBlockKind::Indented => String::new(),
                    };
                    block.quote = plain_quotes > 0;
                    block.list_depth = list.len();
                    block.code = Some(Code {
                        language,
                        body: String::new(),
                    });
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    flush(&mut block, &mut parsed.blocks);
                    if let Some(callout) = rustidian::callouts::Callout::from_range(source, range) {
                        quote_frames.push(Some(CalloutFrame {
                            model: callout,
                            parents: std::mem::take(&mut parsed.blocks),
                            ordinal: callout_ordinal,
                        }));
                        callout_ordinal += 1;
                    } else {
                        quote_frames.push(None);
                        plain_quotes += 1;
                    }
                    quote += 1;
                    block.quote = plain_quotes > 0;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    flush(&mut block, &mut parsed.blocks);
                    if let Some(CalloutFrame {
                        model,
                        parents,
                        ordinal,
                    }) = quote_frames.pop().flatten()
                    {
                        let children = std::mem::replace(&mut parsed.blocks, parents);
                        parsed.blocks.push(Block {
                            callout: Some(CalloutBlock {
                                model,
                                children,
                                ordinal,
                            }),
                            ..Default::default()
                        });
                    } else {
                        plain_quotes -= 1;
                    }
                    quote -= 1;
                }
                Event::Start(Tag::Paragraph) => {
                    block.quote = plain_quotes > 0;
                    block.list_depth = list.len();
                    block.loose_item = !list.is_empty();
                }
                Event::Start(Tag::List(start)) => {
                    flush(&mut block, &mut parsed.blocks);
                    list.push(start);
                }
                Event::End(TagEnd::List(_)) => {
                    list.pop();
                }
                Event::Start(Tag::Item) => {
                    block.list_depth = list.len();
                    block.quote = plain_quotes > 0;
                    let marker = match list.last_mut() {
                        Some(Some(number)) => {
                            let marker = format!("{number}. ");
                            *number += 1;
                            marker
                        }
                        _ => "•  ".into(),
                    };
                    block.parts.push(Inline {
                        text: format!("{}{}", "  ".repeat(list.len().saturating_sub(1)), marker),
                        ..Default::default()
                    });
                }
                Event::Start(Tag::Strong) => style.bold = true,
                Event::End(TagEnd::Strong) => style.bold = false,
                Event::Start(Tag::Emphasis) => style.italic = true,
                Event::End(TagEnd::Emphasis) => style.italic = false,
                Event::Start(Tag::Strikethrough) => style.strike = true,
                Event::End(TagEnd::Strikethrough) => style.strike = false,
                Event::Start(Tag::Link { dest_url, .. }) => {
                    style.link = Some(dest_url.into_string())
                }
                Event::End(TagEnd::Link) => style.link = None,
                Event::Text(text) => {
                    if let Some(code) = &mut block.code {
                        code.body.push_str(&text);
                    } else {
                        block.parts.push(Inline {
                            text: text.into_string(),
                            ..style.clone()
                        });
                    }
                }
                Event::InlineMath(_) | Event::DisplayMath(_) => {
                    let part = Inline {
                        text: source[range.clone()].into(),
                        math: math.iter().find(|math| math.range == range).cloned(),
                        ..style.clone()
                    };
                    if part.math.as_ref().is_some_and(|math| math.display) && block.table.is_none()
                    {
                        let quote = block.quote;
                        let list_depth = block.list_depth;
                        flush(&mut block, &mut parsed.blocks);
                        parsed.blocks.push(Block {
                            math: Some(part),
                            quote,
                            list_depth,
                            ..Default::default()
                        });
                        block.quote = quote;
                        block.list_depth = list_depth;
                    } else {
                        block.parts.push(part);
                    }
                }
                Event::Code(text) => block.parts.push(Inline {
                    text: text.into_string(),
                    code: true,
                    ..style.clone()
                }),
                Event::SoftBreak => block.parts.push(Inline {
                    text: " ".into(),
                    ..style.clone()
                }),
                Event::HardBreak => block.parts.push(Inline {
                    text: "\n".into(),
                    ..style.clone()
                }),
                Event::End(TagEnd::CodeBlock) => {
                    if let Some(body) = original_code.take()
                        && let Some(code) = &mut block.code
                    {
                        code.body = body;
                    }
                    flush(&mut block, &mut parsed.blocks);
                }
                Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item) => {
                    flush(&mut block, &mut parsed.blocks)
                }
                Event::Rule => {
                    flush(&mut block, &mut parsed.blocks);
                    parsed.blocks.push(Block {
                        rule: true,
                        ..Default::default()
                    });
                }
                Event::Start(Tag::Table(alignments)) => {
                    flush(&mut block, &mut parsed.blocks);
                    block.ordinal = table_ordinal;
                    table_ordinal += 1;
                    block.table = Some(Table {
                        alignments,
                        rows: Vec::new(),
                    });
                    table_row.clear();
                }
                Event::End(TagEnd::TableCell) => {
                    table_row.push(std::mem::take(&mut block.parts));
                }
                Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                    if let Some(table) = &mut block.table {
                        table.rows.push(std::mem::take(&mut table_row));
                    }
                }
                Event::End(TagEnd::Table) => flush(&mut block, &mut parsed.blocks),
                Event::InlineHtml(tag)
                    if rustidian::highlight::NativeColor::from_tag(&tag).is_some() =>
                {
                    highlights.push((
                        rustidian::inline_style::Element::Mark,
                        style.html,
                        style.highlight,
                        style.native_highlight,
                    ));
                    style.highlight = None;
                    style.native_highlight = rustidian::highlight::NativeColor::from_tag(&tag);
                }
                Event::InlineHtml(tag) => match rustidian::inline_style::parse(&tag) {
                    Some(rustidian::inline_style::Tag::Open(element, html)) => {
                        highlights.push((
                            element,
                            style.html,
                            style.highlight,
                            style.native_highlight,
                        ));
                        style.html.overlay(html);
                        if let Some(background) = html.background {
                            style.highlight = Some(background);
                            style.native_highlight = None;
                        }
                    }
                    Some(rustidian::inline_style::Tag::Close(element))
                        if highlights
                            .last()
                            .is_some_and(|(open, _, _, _)| *open == element) =>
                    {
                        let (_, html, highlight, native) = highlights.pop().unwrap();
                        style.html = html;
                        style.highlight = highlight;
                        style.native_highlight = native;
                    }
                    _ => parsed.fallback = true,
                },
                Event::Html(_) => parsed.fallback = true,
                _ => {}
            }
        }
        flush(&mut block, &mut parsed.blocks);
        parsed
    }
}

/// Translate wiki links to intercepted native actions, retaining fenced code verbatim.
pub fn prepare(text: &str) -> (String, Vec<(String, String)>) {
    let mut output = String::new();
    let mut links = Vec::new();
    let mut previous = 0;
    let math = rustidian::math::spans(text);
    for (span, target) in rustidian::vault::wiki_spans(text) {
        if math.iter().any(|math| ranges_overlap(&span, &math.range)) {
            continue;
        }
        let destination = format!("rust-note-{}", links.len());
        let label = target
            .split_once('|')
            .map(|(_, label)| label)
            .unwrap_or(&target);
        output.push_str(&text[previous..span.start]);
        output.push_str(&format!("[{label}]({destination})"));
        previous = span.end;
        links.push((destination, target));
    }
    output.push_str(&text[previous..]);
    (output, links)
}

fn ranges_overlap(first: &std::ops::Range<usize>, second: &std::ops::Range<usize>) -> bool {
    first.start < second.end && second.start < first.end
}

fn prepare_images(markdown: &str, resolve: impl Fn(&str) -> Option<String>) -> String {
    let mut prepared = String::new();
    let mut previous = 0;
    for math in rustidian::math::spans(markdown) {
        prepared.push_str(&rustidian::assets::prepare_images(
            &markdown[previous..math.range.start],
            &resolve,
        ));
        prepared.push_str(&markdown[math.range.clone()]);
        previous = math.range.end;
    }
    prepared.push_str(&rustidian::assets::prepare_images(
        &markdown[previous..],
        resolve,
    ));
    prepared
}

pub fn show(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    tab: &mut rustidian::tab::Tab,
    anchor: Option<&str>,
    vault: Option<&rustidian::vault::Vault>,
) -> Option<String> {
    // Navigation reuses the tab; its editing path identifies the document and
    // remains stable when that document moves.
    let document_id = egui::Id::unique((tab.id(), tab.editor_path()));
    let text = &tab.document.text;
    let from = &tab.document.path;
    let body = rustidian::metadata::frontmatter(text)
        .ok()
        .flatten()
        .map_or(0, |properties| properties.range.end);
    let text = &text[body..];
    let (markdown, links) = prepare(text);
    let markdown = prepare_images(&markdown, |destination| {
        let target = links
            .iter()
            .find(|(id, _)| id == destination)
            .map_or(destination, |(_, target)| target);
        rustidian::assets::image_uri(vault, from, target)
    });
    let id = ui.scope_id().with("reading-blocks");
    let parsed = ui.ctx().data_mut(|data| {
        let previous = data.get_temp::<Arc<Parsed>>(id);
        if let Some(previous) = previous
            && previous.source == markdown
        {
            return previous;
        }
        let parsed = Arc::new(Parsed::new(&markdown));
        data.insert_temp(id, parsed.clone());
        parsed
    });
    if !parsed.fallback {
        let mut task = None;
        let select_all_id = document_id.with("select-all");
        let mut select_all = ui
            .ctx()
            .data(|data| data.get_temp::<bool>(select_all_id))
            .unwrap_or(false);
        // Select all lasts until Escape or a press on the note; Ctrl+A starts it.
        if ui.input(|input| input.key_pressed(egui::Key::Escape))
            || ui.input(|input| input.pointer.any_pressed())
                && ui
                    .ctx()
                    .pointer_interact_pos()
                    .is_some_and(|pos| ui.ctx().layer_id_at(pos) == Some(ui.layer_id()))
        {
            select_all = false;
        }
        if !ui.ctx().egui_wants_keyboard_input() && take_select_all_key(ui) {
            select_all = true;
            clear_label_selection(ui.ctx());
        }
        // Pressing the menu's Copy ends egui's text selection when this pass ends,
        // and egui gathers selected text only while a copy is pending, so ask for
        // the copy before this pass draws the text.
        let menu = document_id.with("reading-menu");
        if !select_all
            && egui::Popup::is_id_open(ui.ctx(), menu)
            && ui
                .ctx()
                .data(|data| data.get_temp::<egui::Rect>(menu.with("copy")))
                .is_some_and(|copy| {
                    // A click within one frame leaves no press origin; use its position.
                    ui.input(|input| {
                        input.pointer.primary_pressed()
                            && input
                                .pointer
                                .interact_pos()
                                .is_some_and(|pos| copy.contains(pos))
                    })
                })
        {
            ui.ctx()
                .input_mut(|input| input.events.push(egui::Event::Copy));
        }
        let top = ui.cursor().top();
        let follow = show_blocks(
            ui,
            &parsed.blocks,
            &links,
            anchor,
            &mut task,
            from,
            document_id,
        );
        let area = egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().top());
        reading_menu(ui, area, menu, &parsed.blocks, &mut select_all);
        if select_all {
            ui.painter()
                .rect_filled(area, 0.0, ui.visuals().selection.bg_fill);
            if ui.input(|input| {
                input
                    .events
                    .iter()
                    .any(|event| matches!(event, egui::Event::Copy))
            }) {
                ui.ctx().copy_text(plain_text(&parsed.blocks));
            }
        }
        ui.ctx()
            .data_mut(|data| data.insert_temp(select_all_id, select_all));
        if let Some(ordinal) = task
            && rustidian::editor::toggle_task(&mut tab.document.text, ordinal)
        {
            tab.edited();
        }
        return follow;
    }
    cache.link_hooks_clear();
    for (destination, _) in &links {
        cache.add_link_hook(destination);
    }
    CommonMarkViewer::new()
        .max_image_width(Some(ui.available_width() as usize))
        .show(ui, cache, &markdown);
    links.into_iter().find_map(|(destination, target)| {
        if cache.get_link_hook(&destination) == Some(true) {
            Some(target)
        } else {
            None
        }
    })
}

/// Obsidian's reading-view menu: Copy and Select all.
fn reading_menu(
    ui: &mut egui::Ui,
    area: egui::Rect,
    menu: egui::Id,
    blocks: &[Block],
    select_all: &mut bool,
) {
    use crate::editor_ui::{MenuArea, menu_target, take_menu_click};
    menu_target(ui, area, MenuArea::Reading);
    let opened = take_menu_click(ui.ctx(), MenuArea::Reading).is_some();
    let selected = *select_all
        || ui
            .ctx()
            .plugin_opt::<egui::text_selection::LabelSelectionState>()
            .is_some_and(|plugin| plugin.lock().has_selection());
    let shown = egui::Popup::new(
        menu,
        ui.ctx().clone(),
        egui::PopupAnchor::PointerFixed,
        ui.layer_id(),
    )
    .kind(egui::PopupKind::Menu)
    .layout(egui::Layout::top_down_justified(egui::Align::Min))
    .style(egui::menu::menu_style)
    .open_memory(opened.then_some(egui::SetOpenCommand::Bool(true)))
    .show(|ui| {
        crate::theme::menu_style(ui);
        ui.set_min_width(240.0);
        let copy = ui.add_enabled(selected, egui::Button::new("Copy").shortcut_text("Ctrl+C"));
        crate::ui_probe::record(ui.ctx(), "reading-menu:Copy", copy.rect);
        ui.ctx()
            .data_mut(|data| data.insert_temp(menu.with("copy"), copy.rect));
        // Text selected by dragging was copied when the press began (see `show`).
        if copy.is_pointer_button_down_on() || copy.clicked() {
            if *select_all {
                ui.ctx().copy_text(plain_text(blocks));
            }
            ui.close();
        }
        let all = ui.add(egui::Button::new("Select all").shortcut_text("Ctrl+A"));
        crate::ui_probe::record(ui.ctx(), "reading-menu:Select all", all.rect);
        if all.clicked() {
            *select_all = true;
            clear_label_selection(ui.ctx());
            ui.close();
        }
    });
    if let Some(shown) = shown {
        crate::ui_probe::record(ui.ctx(), "reading-menu", shown.response.rect);
    }
}

/// Consume exactly Ctrl+A, leaving Ctrl+Shift+A to the agent shortcut.
fn take_select_all_key(ui: &egui::Ui) -> bool {
    ui.input_mut(|input| {
        let index = input.events.iter().position(|event| {
            matches!(
                event,
                egui::Event::Key { key: egui::Key::A, pressed: true, modifiers, .. }
                    if modifiers.matches_exact(egui::Modifiers::COMMAND)
            )
        });
        index.map(|index| input.events.remove(index)).is_some()
    })
}

fn clear_label_selection(ctx: &egui::Context) {
    if let Some(plugin) = ctx.plugin_opt::<egui::text_selection::LabelSelectionState>() {
        plugin.lock().clear_selection();
    }
}

/// The note's reading text, one block per line, as Select all copies it.
fn plain_text(blocks: &[Block]) -> String {
    fn collect(blocks: &[Block], lines: &mut Vec<String>) {
        let text =
            |parts: &[Inline]| -> String { parts.iter().map(|part| part.text.as_str()).collect() };
        for block in blocks {
            if let Some(callout) = &block.callout {
                if !callout.model.title.is_empty() {
                    lines.push(callout.model.title.clone());
                }
                collect(&callout.children, lines);
            } else if let Some(math) = &block.math {
                lines.push(math.text.clone());
            } else if let Some(table) = &block.table {
                for row in &table.rows {
                    let cells: Vec<String> = row.iter().map(|cell| text(cell.as_slice())).collect();
                    lines.push(cells.join("\t"));
                }
            } else if let Some(code) = &block.code {
                lines.push(code.body.trim_end_matches('\n').to_owned());
            } else if block.image.is_none() && !block.rule {
                lines.push(text(block.parts.as_slice()));
            }
        }
    }
    let mut lines = Vec::new();
    collect(blocks, &mut lines);
    lines.join("\n")
}

fn matches_anchor(block: &Block, anchor: &str) -> bool {
    if let Some(callout) = &block.callout {
        return callout
            .children
            .iter()
            .any(|block| matches_anchor(block, anchor));
    }
    let content: String = block.parts.iter().map(|part| part.text.as_str()).collect();
    (block.heading > 0 && content.to_lowercase() == anchor.to_lowercase())
        || (anchor.starts_with('^') && content.split_whitespace().last() == Some(anchor))
}

fn show_blocks(
    ui: &mut egui::Ui,
    blocks: &[Block],
    native_links: &[(String, String)],
    anchor: Option<&str>,
    task: &mut Option<usize>,
    from: &std::path::Path,
    document_id: egui::Id,
) -> Option<String> {
    let mut clicked = None;
    ui.style_mut().spacing.item_spacing.y = 0.0;
    let mut occurrences = std::collections::HashMap::new();
    let headings: Vec<_> = blocks
        .iter()
        .map(|block| {
            if block.heading == 0 {
                return None;
            }
            let title: String = block.parts.iter().map(|part| part.text.as_str()).collect();
            let occurrence = occurrences.entry(title.clone()).or_insert(0usize);
            let id = ui.make_persistent_id(("reading-fold", &title, *occurrence));
            *occurrence += 1;
            Some(id)
        })
        .collect();
    if let Some(anchor) = anchor {
        let mut ancestors: Vec<(u8, usize)> = Vec::new();
        for (index, block) in blocks.iter().enumerate() {
            if block.heading > 0 {
                while ancestors
                    .last()
                    .is_some_and(|&(level, _)| level >= block.heading)
                {
                    ancestors.pop();
                }
                ancestors.push((block.heading, index));
            }
            if matches_anchor(block, anchor) {
                ui.ctx().data_mut(|data| {
                    for &(_, index) in &ancestors {
                        data.insert_temp(headings[index].unwrap(), false);
                    }
                });
                break;
            }
        }
    }
    let mut folded_level = None;
    for (index, block) in blocks.iter().enumerate() {
        let ordinal = block.ordinal;
        let table_index = block.ordinal;
        if let Some(level) = folded_level {
            if block.heading == 0 || block.heading > level {
                continue;
            }
            folded_level = None;
        }
        if let Some(callout) = &block.callout {
            let id = ui.make_persistent_id(("reading-callout", document_id, callout.ordinal));
            let mut folded = ui
                .ctx()
                .data(|data| data.get_temp::<bool>(id))
                .unwrap_or(callout.model.folded.unwrap_or(false));
            if anchor.is_some_and(|anchor| matches_anchor(block, anchor)) {
                folded = false;
            }
            ui.add_space(8.0);
            egui::Frame::NONE
                .fill(crate::callout_ui::background(ui, callout.model.kind))
                .inner_margin(egui::Margin {
                    left: 24,
                    right: 12,
                    top: 12,
                    bottom: 12,
                })
                .corner_radius(7)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.push_id(("callout", callout.ordinal), |ui| {
                        let (header, link) =
                            crate::callout_ui::header(ui, &callout.model, callout.ordinal, folded);
                        if let Some(link) = link {
                            clicked = follow_destination(&link, native_links);
                        } else if header.clicked() && callout.model.folded.is_some() {
                            folded = !folded;
                        }
                        if !folded && !callout.children.is_empty() {
                            ui.add_space(10.0);
                            let content = ui.scope(|ui| {
                                show_blocks(
                                    ui,
                                    &callout.children,
                                    native_links,
                                    anchor,
                                    task,
                                    from,
                                    document_id,
                                )
                            });
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("callout-body:{}", callout.ordinal),
                                content.response.rect,
                            );
                            if let Some(link) = content.inner {
                                clicked = Some(link);
                            }
                        }
                    });
                });
            ui.ctx().data_mut(|data| data.insert_temp(id, folded));
            ui.add_space(8.0);
            continue;
        }
        if let Some(part) = &block.math {
            let formula = part.math.as_ref().unwrap();
            ui.add_space(12.0);
            let response = ui
                .vertical(|ui| {
                    match crate::math_ui::image(
                        ui,
                        &formula.formula,
                        true,
                        crate::theme::document_size(ui, 18.0),
                        ui.visuals().text_color(),
                    ) {
                        Ok(Some(mut rendered)) => {
                            rendered.size *=
                                (ui.available_width().max(1.0) / rendered.size.x).min(1.0);
                            let response = ui.add(
                                egui::Image::new(&rendered.uri).fit_to_exact_size(rendered.size),
                            );
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("math-display:{index}"),
                                response.rect,
                            );
                            crate::editor_ui::menu_target(
                                ui,
                                response.rect,
                                crate::editor_ui::MenuArea::Native,
                            );
                            response.context_menu(|ui| {
                                crate::theme::menu_style(ui);
                                if ui.button("Copy LaTeX").clicked() {
                                    ui.ctx().copy_text(part.text.clone());
                                    ui.close();
                                }
                            });
                        }
                        Ok(None) => {
                            ui.label(egui::RichText::new(&part.text).monospace());
                        }
                        Err(error) => {
                            let response = ui
                                .label(egui::RichText::new(&part.text).monospace())
                                .on_hover_text(error);
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("math-error:{index}"),
                                response.rect,
                            );
                        }
                    }
                })
                .response;
            crate::ui_probe::record(ui.ctx(), format!("reading-block:{index}"), response.rect);
            ui.add_space(12.0);
            continue;
        }
        if let Some(table) = &block.table {
            ui.add_space(16.0);
            if let Some(link) = show_table(ui, table, native_links, table_index, from) {
                clicked = Some(link);
            }
            ui.add_space(16.0);
            continue;
        }
        if let Some(code) = &block.code {
            if index == 0 || blocks[index - 1].heading > 0 {
                ui.add_space(2.0);
            }
            let response = egui::Frame::NONE
                .inner_margin(egui::Margin {
                    left: if block.quote { 26 } else { 0 }
                        + if block.list_depth > 0 {
                            (20.0 + 31.5 * block.list_depth.saturating_sub(1) as f32) as i8
                        } else {
                            0
                        },
                    ..Default::default()
                })
                .show(ui, |ui| {
                    if code.language.eq_ignore_ascii_case("mermaid") {
                        crate::diagrams::show(ui, &code.body, ordinal)
                    } else {
                        crate::code_ui::reading(ui, &code.body, &code.language, ordinal)
                    }
                })
                .inner;
            crate::ui_probe::record(ui.ctx(), format!("reading-block:{index}"), response.rect);
            if block.quote {
                ui.painter().line_segment(
                    [
                        response.rect.left_top() - egui::vec2(25.0, 0.0),
                        response.rect.left_bottom() - egui::vec2(25.0, 0.0),
                    ],
                    egui::Stroke::new(2.0, crate::theme::interactive(ui)),
                );
            }
            ui.add_space(18.0);
            continue;
        }
        if let Some(uri) = &block.image {
            let alt: String = block.parts.iter().map(|part| part.text.as_str()).collect();
            if let Some(target) = uri.strip_prefix("rust-missing-image:") {
                ui.weak(format!("Image unavailable: {target}"));
            } else {
                let width = alt
                    .rsplit('|')
                    .next()
                    .and_then(|hint| hint.split('x').next())
                    .and_then(|width| width.parse::<f32>().ok())
                    .filter(|width| *width > 0.0)
                    .unwrap_or(ui.available_width())
                    .min(ui.available_width());
                ui.add(
                    egui::Image::new(uri)
                        .fit_to_original_size(1.0)
                        .max_width(width)
                        .alt_text(&alt),
                );
            }
            ui.add_space(16.0);
            continue;
        }
        if block.rule {
            ui.separator();
            ui.add_space(20.0);
            continue;
        }
        if block.heading > 0 && index > 0 {
            ui.add_space(if block.heading == 2 { 40.0 } else { 24.0 });
        }
        let mut job = egui::text::LayoutJob::default();
        let task_indent = if block.task.is_some() {
            13.5 + 40.5 * block.list_depth.saturating_sub(1) as f32
        } else {
            0.0
        };
        job.wrap.max_width = (ui.available_width()
            - if block.quote { 26.0 } else { 0.0 }
            - if block.task.is_some() {
                20.5 + task_indent
            } else {
                0.0
            })
        .max(1.0);
        let mut destinations = Vec::new();
        let mut math = Vec::new();
        let mut character = 0;
        for (part_index, part) in block.parts.iter().enumerate() {
            let mut format = inline_format(ui, part, block.heading, None);
            if block.task.is_some_and(|(_, checked)| checked) {
                format.color = ui.visuals().weak_text_color();
                format.strikethrough = egui::Stroke::new(1.0, format.color);
            }
            let leading = if part_index == 0 && block.list_depth > 0 && block.task.is_none() {
                20.0 + 31.5 * block.list_depth.saturating_sub(1) as f32
            } else {
                0.0
            };
            if let Some(formula) = &part.math {
                if let Some(placement) = crate::math_inline::append(
                    ui,
                    &mut job,
                    &part.text,
                    &formula.formula,
                    formula.display,
                    format,
                    leading,
                ) {
                    math.push(placement);
                }
            } else {
                job.append(&part.text, leading, format);
            }
            let end = character + part.text.chars().count();
            if let Some(link) = &part.link {
                destinations.push((character..end, link));
            }
            character = end;
        }
        let heading_line_height = (block.heading > 0).then(|| {
            job.sections
                .iter()
                .filter_map(|section| section.format.line_height)
                .fold(0.0, f32::max)
        });
        let mut galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
        if let Some(height) = heading_line_height {
            crate::theme::set_line_height(&mut galley, height);
        }
        let response = egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: if block.quote { 26 } else { 0 },
                ..Default::default()
            })
            .show(ui, |ui| {
                let label = egui::Label::new(galley.clone())
                    .sense(egui::Sense::click())
                    .selectable(true);
                if let Some((ordinal, mut checked)) = block.task {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 9.0;
                        ui.add_space(task_indent);
                        let line_height = galley.rows.first().map_or(29.16, |row| row.height());
                        let size = crate::theme::document_size(ui, 18.0);
                        let checkbox = ui
                            .allocate_ui_with_layout(
                                egui::vec2(size, line_height),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.add_space(((line_height - size) / 2.0).max(0.0));
                                    crate::theme::checkbox(ui, &mut checked)
                                },
                            )
                            .inner;
                        crate::ui_probe::record(ui.ctx(), format!("task:{ordinal}"), checkbox.rect);
                        if checkbox.clicked() {
                            *task = Some(ordinal);
                        }
                        ui.add(label)
                    })
                    .inner
                } else {
                    ui.add(label)
                }
            })
            .inner;
        for placement in &math {
            let rect = crate::math_inline::paint(ui, &galley, response.rect.min, placement);
            crate::ui_probe::record(
                ui.ctx(),
                format!("math-inline:{}", placement.character),
                rect,
            );
        }
        crate::ui_probe::record(ui.ctx(), format!("reading-block:{index}"), response.rect);
        if let Some(id) = headings[index] {
            let mut collapsed = ui
                .ctx()
                .data(|data| data.get_temp::<bool>(id).unwrap_or(false));
            let rect = egui::Rect::from_center_size(
                egui::pos2(
                    response.rect.left() - 16.0,
                    response.rect.top()
                        + galley.rows.first().map_or(18.0, |row| row.height()) / 2.0,
                ),
                egui::vec2(16.0, 18.0),
            );
            let toggle = ui.interact(rect, id, egui::Sense::click());
            let title: String = block.parts.iter().map(|part| part.text.as_str()).collect();
            toggle.widget_info(|| {
                egui::WidgetInfo::labeled(
                    egui::Role::Button,
                    true,
                    format!("{} {title}", if collapsed { "Expand" } else { "Collapse" }),
                )
            });
            crate::ui_probe::record(ui.ctx(), format!("fold:{index}"), rect);
            if toggle.clicked() {
                collapsed = !collapsed;
                ui.ctx().data_mut(|data| data.insert_temp(id, collapsed));
                ui.ctx().request_repaint();
            }
            if collapsed || response.hovered() || toggle.hovered() || toggle.has_focus() {
                crate::theme::disclosure(ui, if collapsed { 0.0 } else { 1.0 }, &toggle);
            }
            toggle.on_hover_text(if collapsed {
                "Expand section"
            } else {
                "Collapse section"
            });
            if collapsed {
                folded_level = Some(block.heading);
            }
        }
        if anchor.is_some_and(|anchor| matches_anchor(block, anchor)) {
            ui.scroll_to_rect(response.rect, Some(egui::Align::Min));
        }
        if block.quote {
            ui.painter().line_segment(
                [
                    response.rect.left_top() - egui::vec2(25.0, 0.0),
                    response.rect.left_bottom() - egui::vec2(25.0, 0.0),
                ],
                egui::Stroke::new(2.0, crate::theme::interactive(ui)),
            );
        }
        if block.heading == 2 {
            ui.add_space(8.6508);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
            ui.painter().hline(
                rect.x_range(),
                rect.center().y,
                ui.visuals().widgets.noninteractive.bg_stroke,
            );
        }
        if let Some(pointer) = response.hover_pos() {
            let cursor = galley.cursor_from_pos(pointer - response.rect.min).index.0;
            if let Some((range, destination)) = destinations
                .iter()
                .find(|(range, _)| range.contains(&cursor))
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                if let Some(target) = follow_destination(destination, native_links) {
                    let first = galley.pos_from_cursor(egui::text::CCursor::new(range.start));
                    let last = galley.pos_from_cursor(egui::text::CCursor::new(range.end));
                    crate::page_preview::offer(
                        ui,
                        from,
                        &target,
                        first.union(last).translate(response.rect.min.to_vec2()),
                    );
                }
                if crate::theme::link_clicked(&response) {
                    clicked = follow_destination(destination, native_links);
                }
            }
        }
        ui.add_space(if block.list_depth > 0 && !block.loose_item {
            3.0
        } else {
            16.0
        });
    }
    clicked
}

fn inline_format(
    ui: &egui::Ui,
    part: &Inline,
    heading: u8,
    table_header: Option<bool>,
) -> egui::TextFormat {
    let size = if heading > 0 || !part.code {
        crate::theme::prose_size(ui, heading)
    } else {
        crate::theme::document_size(
            ui,
            if table_header.is_some() {
                crate::code_ui::FONT_SIZE
            } else {
                16.0
            },
        )
    };
    let family = if part.code {
        egui::FontFamily::Monospace
    } else if heading == 1 || part.html.bold == Some(true) {
        crate::theme::fonts::text("bold")
    } else if part
        .html
        .bold
        .unwrap_or(part.bold || heading > 0 || table_header == Some(true))
    {
        crate::theme::fonts::text("semibold")
    } else if part.html.italic.unwrap_or(part.italic) {
        crate::theme::fonts::text("italic")
    } else {
        crate::theme::fonts::text("regular")
    };
    let color = if let Some([red, green, blue, alpha]) = part.html.color {
        egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha)
    } else if part.link.is_some() {
        crate::theme::accent(ui)
    } else if heading > 0 {
        crate::theme::heading(ui, heading)
    } else if part.code && table_header.is_some() {
        ui.visuals().weak_text_color()
    } else {
        ui.visuals().text_color()
    };
    egui::TextFormat {
        font_id: egui::FontId::new(size, family),
        color,
        italics: part.html.italic.unwrap_or(part.italic),
        extra_letter_spacing: if heading > 0 { -0.025 * size } else { 0.0 },
        line_height: Some(
            size * if table_header.is_some() {
                1.3
            } else if heading > 0 {
                1.2
            } else {
                1.62
            },
        ),
        background: if let Some([red, green, blue, alpha]) = part
            .native_highlight
            .map(|color| color.rgba(ui.visuals().dark_mode))
            .or(part.highlight)
        {
            egui::Color32::from_rgba_unmultiplied(red, green, blue, alpha)
        } else if part.code && table_header.is_none() {
            ui.visuals().code_bg_color
        } else {
            egui::Color32::TRANSPARENT
        },
        strikethrough: if part.html.strike.unwrap_or(part.strike) {
            egui::Stroke::new(1.0, color)
        } else {
            egui::Stroke::NONE
        },
        underline: if part
            .html
            .underline
            .unwrap_or(part.link.is_some() && table_header.is_some())
        {
            egui::Stroke::new(1.0, color)
        } else {
            egui::Stroke::NONE
        },
        ..Default::default()
    }
}

fn follow_destination(destination: &str, native_links: &[(String, String)]) -> Option<String> {
    if let Some((_, target)) = native_links.iter().find(|(hook, _)| hook == destination) {
        Some(target.clone())
    } else {
        Some(destination.to_owned())
    }
}

const INLINE_PADDING: f32 = 3.9375;

fn table_job(
    ui: &egui::Ui,
    parts: &[Inline],
    header: bool,
    width: f32,
) -> (egui::text::LayoutJob, Vec<crate::math_inline::Placement>) {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    job.first_row_min_height = 23.4;
    let mut math = Vec::new();
    let mut previous_code = false;
    for part in parts {
        let leading = INLINE_PADDING * (usize::from(part.code) + usize::from(previous_code)) as f32;
        let format = inline_format(ui, part, 0, Some(header));
        if let Some(formula) = &part.math {
            if let Some(placement) = crate::math_inline::append(
                ui,
                &mut job,
                &part.text,
                &formula.formula,
                formula.display,
                format,
                leading,
            ) {
                math.push(placement);
            }
        } else {
            job.append(&part.text, leading, format);
        }
        previous_code = part.code;
    }
    if parts.is_empty() {
        job.append(
            "",
            0.0,
            inline_format(ui, &Inline::default(), 0, Some(header)),
        );
    }
    (job, math)
}

fn table_line_height(galley: &egui::Galley, has_code: bool, has_math: bool) -> f32 {
    let height: f32 = if has_code { 23.625 } else { 23.4 };
    if has_math {
        galley
            .rows
            .iter()
            .fold(height, |height, row| height.max(row.height()))
    } else {
        height
    }
}

fn show_table(
    ui: &mut egui::Ui,
    table: &Table,
    native_links: &[(String, String)],
    ordinal: usize,
    from: &std::path::Path,
) -> Option<String> {
    use egui::text::CCursor;
    let columns = table.alignments.len();
    let mut widths = vec![0.0_f32; columns];
    let mut heights = Vec::new();
    let mut cells = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut shaped_row = Vec::new();
        let mut height = 23.4_f32;
        for (column, parts) in row.iter().enumerate() {
            let (job, math) = table_job(ui, parts, row_index == 0, f32::INFINITY);
            let mut character = 0;
            let mut links = Vec::new();
            let mut code_ranges = Vec::new();
            let mut previous_code = false;
            for part in parts {
                let end = character + part.text.chars().count();
                if let Some(destination) = &part.link {
                    links.push((character..end, destination));
                }
                if part.code {
                    code_ranges.push(character..end);
                }
                character = end;
                previous_code = part.code;
            }
            let mut galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            let line_height = table_line_height(&galley, !code_ranges.is_empty(), !math.is_empty());
            crate::theme::set_line_height(&mut galley, line_height);
            let content_width = galley.size().x + if previous_code { INLINE_PADDING } else { 0.0 };
            if let Some(width) = widths.get_mut(column) {
                *width = width.max(content_width + 17.0);
            }
            height = height.max(galley.size().y);
            shaped_row.push((galley, content_width, links, code_ranges, math));
        }
        heights.push(height + 9.0);
        cells.push(shaped_row);
    }
    let available = ui.available_width();
    let resized = crate::table_layout::widths(ui.ctx(), from, ordinal, &widths, available);
    if resized != widths {
        widths = resized;
        for (row, cells) in cells.iter_mut().enumerate() {
            let mut height = 23.4_f32;
            for (column, (galley, content_width, _, code_ranges, math)) in
                cells.iter_mut().enumerate()
            {
                let (job, placements) = table_job(
                    ui,
                    &table.rows[row][column],
                    row == 0,
                    (widths[column] - 17.0).max(1.0),
                );
                *math = placements;
                *galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                let line_height =
                    table_line_height(galley, !code_ranges.is_empty(), !math.is_empty());
                crate::theme::set_line_height(galley, line_height);
                *content_width = galley.size().x;
                height = height.max(galley.size().y);
            }
            heights[row] = height + 9.0;
        }
    }
    let border = ui.visuals().widgets.noninteractive.bg_stroke;
    let mut follow = None;
    egui::ScrollArea::horizontal()
        .id_salt(("reading-table", ordinal))
        .auto_shrink([true, true])
        .show(ui, |ui| {
            let size = egui::vec2(
                widths.iter().sum::<f32>() + 1.0,
                heights.iter().sum::<f32>() + 1.0,
            );
            let (outer, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            crate::ui_probe::record(
                ui.ctx(),
                format!("table:{ordinal}"),
                outer.intersect(ui.clip_rect()),
            );
            let start = outer.min + egui::vec2(0.5, 0.5);
            let mut y = start.y;
            for (row_index, row) in cells.iter().enumerate() {
                let mut x = start.x;
                for (column, (galley, width, links, codes, math)) in row.iter().enumerate() {
                    let Some(&cell_width) = widths.get(column) else {
                        continue;
                    };
                    let cell = egui::Rect::from_min_size(
                        egui::pos2(x, y),
                        egui::vec2(cell_width, heights[row_index]),
                    );
                    if !cell.intersects(ui.clip_rect()) {
                        x += cell_width;
                        continue;
                    }
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("table-cell:{ordinal}:{row_index}:{column}"),
                        cell.intersect(ui.clip_rect()),
                    );
                    let text_x = match table.alignments[column] {
                        pulldown_cmark::Alignment::Center => cell.center().x - width / 2.0,
                        pulldown_cmark::Alignment::Right => cell.right() - 8.5 - width,
                        _ => cell.left() + 8.5,
                    };
                    let position = egui::pos2(text_x, cell.top() + 4.5);
                    for range in codes {
                        let first = galley.pos_from_cursor(CCursor::new(range.start));
                        let last = galley.pos_from_cursor(CCursor::new(range.end));
                        let rect = egui::Rect::from_min_max(
                            position + egui::vec2(first.left() - INLINE_PADDING, 0.0),
                            position + egui::vec2(last.left() + INLINE_PADDING, 23.625),
                        );
                        ui.painter()
                            .rect_filled(rect, 3, ui.visuals().code_bg_color);
                    }
                    let text_rect = egui::Rect::from_min_size(position, galley.size());
                    ui.put(text_rect, egui::Label::new(galley.clone()).selectable(true));
                    for placement in math {
                        let rect = crate::math_inline::paint(ui, galley, position, placement);
                        crate::ui_probe::record(
                            ui.ctx(),
                            format!(
                                "math-table:{ordinal}:{row_index}:{column}:{}",
                                placement.character
                            ),
                            rect.intersect(ui.clip_rect()),
                        );
                    }
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("table-text:{ordinal}:{row_index}:{column}"),
                        text_rect.intersect(ui.clip_rect()),
                    );
                    for (link_index, (range, destination)) in links.iter().enumerate() {
                        let first = galley.pos_from_cursor(CCursor::new(range.start));
                        let last = galley.pos_from_cursor(CCursor::new(range.end));
                        let rect = egui::Rect::from_min_max(
                            position + first.min.to_vec2(),
                            position + last.max.to_vec2(),
                        );
                        let response = ui
                            .interact(
                                rect.intersect(ui.clip_rect()),
                                ui.make_persistent_id((
                                    "table-link",
                                    ordinal,
                                    row_index,
                                    column,
                                    link_index,
                                )),
                                egui::Sense::click(),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        crate::ui_probe::record(
                            ui.ctx(),
                            format!("table-link:{ordinal}:{row_index}:{column}:{link_index}"),
                            rect,
                        );
                        if crate::theme::link_clicked(&response) {
                            follow = follow_destination(destination, native_links);
                        }
                        if response.hovered()
                            && let Some(target) = follow_destination(destination, native_links)
                        {
                            crate::page_preview::offer(ui, from, &target, rect);
                        }
                    }
                    x += cell_width;
                }
                y += heights[row_index];
            }
            let stroke = border;
            let mut x = start.x;
            for width in widths.iter().chain(std::iter::once(&0.0)) {
                ui.painter()
                    .line_segment([egui::pos2(x, start.y), egui::pos2(x, y)], stroke);
                x += width;
            }
            let mut y = start.y;
            for height in heights.iter().chain(std::iter::once(&0.0)) {
                ui.painter()
                    .line_segment([egui::pos2(start.x, y), egui::pos2(x, y)], stroke);
                y += height;
            }
            let in_hover_card = ui.ctx().data(|data| {
                data.get_temp::<bool>(egui::Id::unique("inside-page-preview"))
                    .unwrap_or(false)
            });
            if !in_hover_card {
                crate::table_layout::handles(ui, from, ordinal, outer, &widths, available);
            }
        });
    follow
}

#[cfg(test)]
#[path = "preview_callout_tests.rs"]
mod callout_tests;

#[cfg(test)]
mod tests {
    use super::{Parsed, TableCell, prepare};
    use eframe::egui;

    #[test]
    fn preview_transforms_do_not_rewrite_math_as_links_highlights_or_images() {
        let raw_math = "$[[x]] ==y== ![z](missing.png)$";
        let source = format!("[[Home]] {raw_math} ==outside==\n\n![Outside](outside.png)");
        let (prepared, links) = prepare(&source);
        assert_eq!(links, [("rust-note-0".into(), "Home".into())]);
        let prepared = super::prepare_images(&prepared, |_| Some("file:///resolved.png".into()));
        assert!(prepared.contains(raw_math));
        assert!(prepared.contains("![Outside](<file:///resolved.png>)"));
        let parsed = Parsed::new(&prepared);
        assert!(!parsed.fallback);
        let parts: Vec<_> = parsed
            .blocks
            .iter()
            .flat_map(|block| &block.parts)
            .collect();
        let math = parts.iter().find(|part| part.math.is_some()).unwrap();
        assert_eq!(math.text, raw_math);
        assert_eq!(
            math.math.as_ref().unwrap().formula,
            "[[x]] ==y== ![z](missing.png)"
        );
        assert!(
            parts
                .iter()
                .any(|part| part.text == "outside" && part.highlight.is_some())
        );
    }

    #[test]
    fn reading_math_keeps_surrounding_prose_nested_blocks_and_literal_prices() {
        let source = "Before $x$ after\n\n$$\\frac{a}{b}$$\n\n> [!note]\n> Inline $y$\n>\n> $$z^2$$\n>\n> | Formula |\n> | --- |\n> | $w$ |\n\nAfter $5 and$10.";
        let parsed = Parsed::new(source);
        assert!(!parsed.fallback);
        assert_eq!(
            parsed.blocks[0]
                .parts
                .iter()
                .map(|part| part.text.as_str())
                .collect::<String>(),
            "Before $x$ after"
        );
        assert_eq!(
            parsed.blocks[0]
                .parts
                .iter()
                .filter_map(|part| part.math.as_ref())
                .map(|math| math.formula.as_str())
                .collect::<Vec<_>>(),
            ["x"]
        );
        let display = parsed.blocks[1].math.as_ref().unwrap();
        assert_eq!(display.text, "$$\\frac{a}{b}$$");
        assert_eq!(display.math.as_ref().unwrap().formula, "\\frac{a}{b}");
        let callout = parsed
            .blocks
            .iter()
            .find_map(|block| block.callout.as_ref())
            .unwrap();
        assert!(callout.children.iter().any(|block| {
            block
                .math
                .as_ref()
                .is_some_and(|part| part.math.as_ref().is_some_and(|math| math.formula == "z^2"))
        }));
        let table = callout
            .children
            .iter()
            .find_map(|block| block.table.as_ref())
            .unwrap();
        assert_eq!(table.rows[1][0][0].text, "$w$");
        assert_eq!(table.rows[1][0][0].math.as_ref().unwrap().formula, "w");
        let last = parsed.blocks.last().unwrap();
        assert_eq!(
            last.parts
                .iter()
                .map(|part| part.text.as_str())
                .collect::<String>(),
            "After $5 and$10."
        );
        assert!(last.parts.iter().all(|part| part.math.is_none()));
    }

    #[test]
    fn an_empty_native_mark_is_invisible_and_keeps_the_next_color() {
        let parsed = Parsed::new("Before ==🔵== ==🟣EXISTING== after");
        assert!(!parsed.fallback);
        let parts: Vec<_> = parsed
            .blocks
            .iter()
            .flat_map(|block| &block.parts)
            .collect();
        let visible: String = parts.iter().map(|part| part.text.as_str()).collect();
        assert_eq!(visible, "Before  EXISTING after");
        assert!(parts.iter().any(|part| part.text == "EXISTING"
            && part.native_highlight == Some(rustidian::highlight::NativeColor::Purple)));
    }

    #[test]
    fn reader_resolves_native_color_markers_adjacent_spans_and_literal_code() {
        use rustidian::highlight::NativeColor;
        let source = "==🔵TA====🟣RGE====🔵T== and ==🟡[TARGET](Other.md)== plus `==🔴literal==`";
        let parsed = Parsed::new(source);
        assert!(!parsed.fallback);
        let parts: Vec<_> = parsed
            .blocks
            .iter()
            .flat_map(|block| &block.parts)
            .collect();
        for (text, color) in [
            ("TA", NativeColor::Blue),
            ("RGE", NativeColor::Purple),
            ("T", NativeColor::Blue),
            ("TARGET", NativeColor::Yellow),
        ] {
            assert!(
                parts
                    .iter()
                    .any(|part| part.text == text && part.native_highlight == Some(color)),
                "{text}"
            );
        }
        assert!(parts.iter().any(|part| part.code
            && part.text == "==🔴literal=="
            && part.native_highlight.is_none()));
        assert!(
            parts
                .iter()
                .any(|part| part.text == "TARGET" && part.link.as_deref() == Some("Other.md"))
        );
    }

    #[test]
    fn reader_preserves_code_body_bytes_empty_blocks_and_literal_examples() {
        let source = "Before\r\n\r\n~~~rust\r\nfn main() {\r\n    let s = \"[[No link]] π\";\r\n}\r\n~~~\r\n\r\n~~~text\r\n~~~\r\n\r\nAfter";
        let parsed = Parsed::new(source);
        let code: Vec<_> = parsed
            .blocks
            .iter()
            .filter_map(|block| block.code.as_ref())
            .collect();
        assert_eq!(code.len(), 2);
        assert_eq!(code[0].language, "rust");
        assert_eq!(
            code[0].body,
            "fn main() {\r\n    let s = \"[[No link]] π\";\r\n}\r\n"
        );
        assert_eq!(code[1].body, "");
        assert_eq!(parsed.blocks.first().unwrap().parts[0].text, "Before");
        assert_eq!(parsed.blocks.last().unwrap().parts[0].text, "After");
        let nested = Parsed::new(
            "> ~~~rust\n> fn quoted() {}\n> ~~~\n\n- Item\n\n  ~~~text\n  [[Literal]]\n  ~~~\n",
        );
        let code: Vec<_> = nested
            .blocks
            .iter()
            .filter_map(|block| block.code.as_ref())
            .collect();
        assert_eq!(
            code.iter()
                .map(|code| code.body.as_str())
                .collect::<Vec<_>>(),
            ["fn quoted() {}\n", "[[Literal]]\n"]
        );
        assert!(nested.blocks[0].quote);
        assert_eq!(nested.blocks.last().unwrap().list_depth, 1);
    }

    #[test]
    fn reader_tracks_nested_list_indentation_and_task_content() {
        let parsed = Parsed::new("- Root\n  - Nested\n- [ ] Task\n\nOutside");
        assert!(!parsed.fallback);
        assert_eq!(
            parsed
                .blocks
                .iter()
                .map(|block| block.list_depth)
                .collect::<Vec<_>>(),
            [1, 2, 1, 0]
        );
        assert_eq!(
            parsed.blocks[2]
                .parts
                .iter()
                .map(|part| part.text.as_str())
                .collect::<String>(),
            "Task"
        );
        assert!(parsed.blocks[3].task.is_none());
    }

    #[test]
    fn native_images_preserve_caption_and_neighboring_text() {
        let parsed = Parsed::new("Before\n\n![Caption|180](Assets/image.png)\n\nAfter");
        assert!(!parsed.fallback);
        assert_eq!(parsed.blocks[1].image.as_deref(), Some("Assets/image.png"));
        assert_eq!(parsed.blocks[1].parts[0].text, "Caption|180");
        assert_eq!(parsed.blocks[2].parts[0].text, "After");
    }

    #[test]
    fn reader_applies_weight_and_link_styles_without_leaking_to_the_rest() {
        let parsed = Parsed::new(
            "## Heading\n\n**Bold** plain [link](https://example.com) `code`.\n\n> Quoted\n",
        );
        assert!(!parsed.fallback);
        assert_eq!(parsed.blocks[0].heading, 2);
        let body = &parsed.blocks[1].parts;
        assert!(body[0].bold);
        assert!(!body[1].bold);
        assert!(
            body.iter()
                .any(|part| part.link.as_deref() == Some("https://example.com"))
        );
        assert!(body.iter().any(|part| part.code && part.text == "code"));
        assert!(parsed.blocks[2].quote);
    }

    #[test]
    fn reader_keeps_the_existing_html_renderer_for_complex_blocks() {
        assert!(Parsed::new("<table><tr><td>HTML</td></tr></table>\n").fallback);
        assert!(!Parsed::new("| A | B |\n|---|---|\n| 1 | 2 |\n").fallback);
        assert!(!Parsed::new("![Image](image.png)").fallback);
        assert!(!Parsed::new("```\n**literal**\n```\n").fallback);
    }

    #[test]
    fn compound_inline_styles_keep_the_whole_document_on_the_native_renderer() {
        let parsed = Parsed::new(include_str!("../tests/fixtures/inline-styles.md"));
        assert!(!parsed.fallback);
        let table = parsed
            .blocks
            .iter()
            .find_map(|block| block.table.as_ref())
            .unwrap();
        let header = &table.rows[0][1];
        assert_eq!(
            header
                .iter()
                .map(|part| part.text.as_str())
                .collect::<String>(),
            "DW"
        );
        assert!(header.iter().all(
            |part| part.html.color == Some([43, 122, 75, 255]) && part.html.bold == Some(true)
        ));
        assert!(
            table.rows[1][1]
                .iter()
                .any(|part| part.text == "11h" && part.highlight.is_some())
        );
        assert!(
            table.rows[1][2]
                .iter()
                .any(|part| part.text == "8h" && part.highlight == Some([46, 204, 113, 102]))
        );
        let callout = parsed
            .blocks
            .iter()
            .find_map(|block| block.callout.as_ref())
            .unwrap();
        assert!(
            callout
                .children
                .iter()
                .flat_map(|block| &block.parts)
                .any(|part| part.text == "DW" && part.html.color == Some([43, 122, 75, 255]))
        );
        let nested = callout
            .children
            .iter()
            .find_map(|block| block.table.as_ref())
            .unwrap();
        assert!(
            nested.rows[1][1]
                .iter()
                .any(|part| part.text == "49h" && part.highlight == Some([46, 204, 113, 102]))
        );
        assert!(
            parsed
                .blocks
                .iter()
                .flat_map(|block| &block.parts)
                .any(|part| part.code && part.text.contains("<mark style="))
        );
    }

    #[test]
    fn actual_cell_layout_paints_shared_styles_and_restores_following_text() {
        let cell = TableCell::new(
            "<span style='color:#2b7a4b;font-weight:700'><mark style='background-color:rgba(46,204,113,.4);padding:0.1em 0.2em'>DW</mark></span> plain ==11h==",
        );
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::theme::apply(&ctx, true);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let galley = cell.layout(ui, false, 900.0);
            assert_eq!(galley.job.text, "DW plain 11h");
            let styled = &galley.job.sections[0].format;
            assert_eq!(styled.color, egui::Color32::from_rgb(43, 122, 75));
            assert_eq!(
                styled.background,
                egui::Color32::from_rgba_unmultiplied(46, 204, 113, 102)
            );
            assert_eq!(styled.font_id.family, crate::theme::fonts::text("bold"));
            let plain = galley
                .job
                .sections
                .iter()
                .find(|section| {
                    galley.job.text[section.byte_range.start.0..section.byte_range.end.0]
                        .contains("plain")
                })
                .unwrap();
            assert_eq!(plain.format.color, ui.visuals().text_color());
            assert_eq!(plain.format.background, egui::Color32::TRANSPARENT);
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn native_tables_preserve_alignments_styles_links_empty_cells_and_neighboring_code() {
        let (source, links) = prepare(include_str!("../tests/fixtures/table.md"));
        let parsed = Parsed::new(&source);
        assert!(!parsed.fallback);
        assert_eq!(parsed.blocks.len(), 4);
        let table = parsed.blocks[1].table.as_ref().unwrap();
        assert_eq!(
            table.alignments,
            [
                pulldown_cmark::Alignment::Left,
                pulldown_cmark::Alignment::Center,
                pulldown_cmark::Alignment::Right
            ]
        );
        assert_eq!(table.rows.len(), 4);
        assert!(table.rows.iter().all(|row| row.len() == 3));
        assert_eq!(table.rows[0][0][0].text, "Item");
        assert!(table.rows[1][0][0].bold);
        assert!(table.rows[1][1][0].italic);
        assert_eq!(table.rows[2][0][0].text, "Home note");
        assert_eq!(table.rows[2][0][0].link.as_deref(), Some("rust-note-0"));
        assert!(table.rows[2][1][0].code);
        assert_eq!(table.rows[2][2][0].text, "π");
        assert!(table.rows[3][1].is_empty());
        assert_eq!(links[0].1, "Home|Home note");
        assert_eq!(parsed.blocks[2].heading, 2);
        assert_eq!(
            parsed.blocks[3].code.as_ref().unwrap().body,
            "fn main() {\n    let answer: u32 = 42;\n}\n"
        );
        assert!(Parsed::new("| Image |\n| --- |\n| ![Caption](image.png) |\n").fallback);
    }

    #[test]
    fn translates_wiki_links_but_preserves_code_and_unclosed_links() {
        let (text, links) = prepare("[[Folder/Note|Read this]]\n```\n[[Code]]\n```\n[[Unclosed");
        assert!(text.starts_with("[Read this](rust-note-0)"));
        assert!(text.contains("[[Code]]"));
        assert!(text.ends_with("[[Unclosed"));
        assert_eq!(
            links,
            vec![("rust-note-0".into(), "Folder/Note|Read this".into())]
        );
    }

    #[test]
    fn retains_inline_code_examples_verbatim() {
        let (text, links) = prepare("Write `[[Home]]`, then open [[Home]].");
        assert_eq!(text, "Write `[[Home]]`, then open [Home](rust-note-0).");
        assert_eq!(links.len(), 1);
    }
}
