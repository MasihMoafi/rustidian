use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::sync::Arc;

#[derive(Clone, Default)]
struct Inline {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    strike: bool,
    link: Option<String>,
}
#[derive(Clone, Default)]
struct Block {
    parts: Vec<Inline>,
    heading: u8,
    quote: bool,
    code: Option<Code>,
    table: Option<Table>,
    rule: bool,
    image: Option<String>,
    task: Option<(usize, bool)>,
    list_depth: usize,
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
}

impl TableCell {
    pub fn new(source: &str) -> Self {
        let (prepared, _) = prepare(source);
        let parsed = Parsed::new(&prepared);
        Self {
            parts: parsed
                .blocks
                .into_iter()
                .flat_map(|block| block.parts)
                .collect(),
        }
    }

    pub fn layout(&self, ui: &egui::Ui, header: bool, width: f32) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = width;
        job.first_row_min_height = 23.4;
        for part in &self.parts {
            job.append(&part.text, 0.0, inline_format(ui, part, 0, Some(header)));
        }
        if job.sections.is_empty() {
            job.append(
                "",
                0.0,
                inline_format(ui, &Inline::default(), 0, Some(header)),
            );
        }
        ui.fonts_mut(|fonts| fonts.layout_job(job))
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
        let mut block = Block::default();
        let mut style = Inline::default();
        let mut list: Vec<Option<u64>> = Vec::new();
        let mut task_count = 0;
        let mut quote = 0;
        let mut original_code = None;
        let mut table_row = Vec::new();
        let flush = |block: &mut Block, blocks: &mut Vec<Block>| {
            if !block.parts.is_empty()
                || block.rule
                || block.image.is_some()
                || block.task.is_some()
                || block.code.is_some()
                || block.table.is_some()
            {
                blocks.push(std::mem::take(block));
            }
        };
        for (event, range) in Parser::new_ext(
            source,
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
        )
        .into_offset_iter()
        {
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
                    let language = match kind {
                        CodeBlockKind::Fenced(language) => {
                            if quote == 0 && list.is_empty() {
                                original_code = rust_notes::editor::CodeBlock::fenced(
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
                    block.quote = quote > 0;
                    block.list_depth = list.len();
                    block.code = Some(Code {
                        language,
                        body: String::new(),
                    });
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    quote += 1;
                    block.quote = true;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    flush(&mut block, &mut parsed.blocks);
                    quote -= 1;
                }
                Event::Start(Tag::Paragraph) => {
                    block.quote = quote > 0;
                    block.list_depth = list.len();
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
                    block.quote = quote > 0;
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
                Event::Html(_) | Event::InlineHtml(_) => parsed.fallback = true,
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
    for (span, target) in rust_notes::vault::wiki_spans(text) {
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

pub fn show(
    ui: &mut egui::Ui,
    cache: &mut CommonMarkCache,
    tab: &mut rust_notes::tab::Tab,
    anchor: Option<&str>,
    vault: Option<&rust_notes::vault::Vault>,
) -> Option<String> {
    let text = &tab.document.text;
    let from = &tab.document.path;
    let body = rust_notes::metadata::frontmatter(text)
        .ok()
        .flatten()
        .map_or(0, |properties| properties.range.end);
    let text = &text[body..];
    let (markdown, links) = prepare(text);
    let markdown = rust_notes::assets::prepare_images(&markdown, |destination| {
        let target = links
            .iter()
            .find(|(id, _)| id == destination)
            .map_or(destination, |(_, target)| target);
        rust_notes::assets::image_uri(vault, from, target)
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
        let follow = show_blocks(ui, &parsed.blocks, &links, anchor, &mut task);
        if let Some(ordinal) = task
            && rust_notes::editor::toggle_task(&mut tab.document.text, ordinal)
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

fn matches_anchor(block: &Block, anchor: &str) -> bool {
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
    let mut code_ordinal = 0;
    let mut table_ordinal = 0;
    for (index, block) in blocks.iter().enumerate() {
        let ordinal = code_ordinal;
        code_ordinal += usize::from(block.code.is_some());
        let table_index = table_ordinal;
        table_ordinal += usize::from(block.table.is_some());
        if let Some(level) = folded_level {
            if block.heading == 0 || block.heading > level {
                continue;
            }
            folded_level = None;
        }
        if let Some(table) = &block.table {
            ui.add_space(16.0);
            if let Some(link) = show_table(ui, table, native_links, table_index) {
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
                    crate::code_ui::reading(ui, &code.body, &code.language, ordinal)
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
            20.0 + 40.5 * block.list_depth.saturating_sub(1) as f32
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
        let mut character = 0;
        for (part_index, part) in block.parts.iter().enumerate() {
            job.append(
                &part.text,
                if part_index == 0 && block.list_depth > 0 && block.task.is_none() {
                    20.0 + 31.5 * block.list_depth.saturating_sub(1) as f32
                } else {
                    0.0
                },
                inline_format(ui, part, block.heading, None),
            );
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
                        ui.spacing_mut().item_spacing.x = 2.5;
                        ui.add_space(task_indent);
                        let checkbox = crate::theme::checkbox(ui, &mut checked);
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
            if let Some((_, destination)) = destinations
                .iter()
                .find(|(range, _)| range.contains(&cursor))
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                if response.clicked() {
                    clicked = follow_destination(ui, destination, native_links);
                }
            }
        }
        ui.add_space(
            if block
                .parts
                .first()
                .is_some_and(|p| p.text.starts_with("•  "))
            {
                3.0
            } else {
                16.0
            },
        );
    }
    clicked
}

fn inline_format(
    ui: &egui::Ui,
    part: &Inline,
    heading: u8,
    table_header: Option<bool>,
) -> egui::TextFormat {
    let size = match heading {
        1 => 32.436,
        2 => 28.836,
        3 => 25.632,
        4.. => 22.788,
        _ if part.code && table_header.is_some() => crate::code_ui::FONT_SIZE,
        _ if part.code => 16.0,
        _ => 18.0,
    };
    let family = if part.code {
        egui::FontFamily::Monospace
    } else if heading == 1 {
        egui::FontFamily::Name("bold".into())
    } else if part.bold || heading > 0 || table_header == Some(true) {
        egui::FontFamily::Name("semibold".into())
    } else if part.italic {
        egui::FontFamily::Name("italic".into())
    } else {
        egui::FontFamily::Proportional
    };
    let color = if part.link.is_some() {
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
        italics: part.italic,
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
        background: if part.code && table_header.is_none() {
            ui.visuals().code_bg_color
        } else {
            egui::Color32::TRANSPARENT
        },
        strikethrough: if part.strike {
            egui::Stroke::new(1.0, color)
        } else {
            egui::Stroke::NONE
        },
        underline: if part.link.is_some() && table_header.is_some() {
            egui::Stroke::new(1.0, color)
        } else {
            egui::Stroke::NONE
        },
        ..Default::default()
    }
}

fn follow_destination(
    ui: &egui::Ui,
    destination: &str,
    native_links: &[(String, String)],
) -> Option<String> {
    if let Some((_, target)) = native_links.iter().find(|(hook, _)| hook == destination) {
        Some(target.clone())
    } else if destination.ends_with(".md") {
        Some(destination.to_owned())
    } else {
        ui.ctx().open_url(egui::OpenUrl::new_tab(destination));
        None
    }
}

fn show_table(
    ui: &mut egui::Ui,
    table: &Table,
    native_links: &[(String, String)],
    ordinal: usize,
) -> Option<String> {
    use egui::text::{CCursor, LayoutJob};
    const INLINE_PADDING: f32 = 3.9375;
    let columns = table.alignments.len();
    let mut widths = vec![0.0_f32; columns];
    let mut heights = Vec::new();
    let mut cells = Vec::new();
    for (row_index, row) in table.rows.iter().enumerate() {
        let mut shaped_row = Vec::new();
        let mut height = 23.4_f32;
        for (column, parts) in row.iter().enumerate() {
            let mut job = LayoutJob::default();
            job.wrap.max_width = f32::INFINITY;
            job.first_row_min_height = 23.4;
            let mut character = 0;
            let mut links = Vec::new();
            let mut code_ranges = Vec::new();
            let mut previous_code = false;
            for part in parts {
                let leading =
                    INLINE_PADDING * (usize::from(part.code) + usize::from(previous_code)) as f32;
                job.append(
                    &part.text,
                    leading,
                    inline_format(ui, part, 0, Some(row_index == 0)),
                );
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
            if parts.is_empty() {
                job.append(
                    "",
                    0.0,
                    inline_format(ui, &Inline::default(), 0, Some(row_index == 0)),
                );
            }
            let mut galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
            let line_height = if code_ranges.is_empty() { 23.4 } else { 23.625 };
            crate::theme::set_line_height(&mut galley, line_height);
            let content_width = galley.size().x + if previous_code { INLINE_PADDING } else { 0.0 };
            if let Some(width) = widths.get_mut(column) {
                *width = width.max(content_width + 17.0);
            }
            height = height.max(galley.size().y);
            shaped_row.push((galley, content_width, links, code_ranges));
        }
        heights.push(height + 9.0);
        cells.push(shaped_row);
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
                for (column, (galley, width, links, codes)) in row.iter().enumerate() {
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
                        if response.clicked() {
                            follow = follow_destination(ui, destination, native_links);
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
        });
    follow
}

#[cfg(test)]
mod tests {
    use super::{Parsed, prepare};

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
