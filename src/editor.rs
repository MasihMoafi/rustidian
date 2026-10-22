//! Source-preserving Markdown decorations and character-indexed editing commands.
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

pub fn link_at(text: &str, caret: usize) -> Option<String> {
    let byte = text
        .char_indices()
        .nth(caret)
        .map_or(text.len(), |(i, _)| i);
    if let Some((_, target)) = crate::vault::wiki_spans(text)
        .into_iter()
        .find(|(range, _)| range.contains(&byte) || range.end == byte)
    {
        return Some(target);
    }
    Parser::new(text)
        .into_offset_iter()
        .find_map(|(event, range)| {
            if let Event::Start(Tag::Link { dest_url, .. }) = event
                && (range.contains(&byte) || range.end == byte)
            {
                Some(dest_url.into_string())
            } else {
                None
            }
        })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Style {
    pub heading: u8,
    pub strong: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
    pub hidden: bool,
    pub quote: bool,
    pub bullet: bool,
    pub task_marker: bool,
    pub task_checked: bool,
    pub highlight: Option<crate::highlight::Rgba>,
    pub native_highlight: Option<crate::highlight::NativeColor>,
    pub html: crate::inline_style::Style,
    pub property: bool,
    pub image: bool,
    pub marker: bool,
    pub quote_marker: bool,
    pub block_code: bool,
    pub fence: bool,
    pub callout_header: bool,
}

#[derive(Clone, Debug)]
pub struct Span {
    pub range: Range<usize>,
    pub style: Style,
}

#[derive(Default)]
pub struct Markdown {
    pub spans: Vec<Span>,
    pub heading_starts: Vec<usize>,
    pub item_starts: Vec<usize>,
    pub quotes: Vec<QuotePrefix>,
    pub setext_markers: Vec<Range<usize>>,
    pub code_blocks: Vec<CodeBlock>,
    pub tables: Vec<crate::tables::Table>,
    pub callouts: Vec<crate::callouts::Callout>,
    pub tasks: Vec<Task>,
    pub highlights: Vec<Range<usize>>,
}

#[derive(Clone, Debug)]
pub struct Task {
    pub prefix: Range<usize>,
    pub character: usize,
    pub checked: bool,
}

#[derive(Clone, Debug)]
pub struct CodeBlock {
    pub range: Range<usize>,
    pub opening: Range<usize>,
    pub closing: Option<Range<usize>>,
    pub language: String,
}

impl CodeBlock {
    /// The caller supplies a fenced-block range from the Markdown parser.
    pub fn fenced(text: &str, range: Range<usize>, language: String) -> Self {
        let opening_end = text[range.clone()]
            .find('\n')
            .map_or(range.end, |end| range.start + end);
        let opening = range.start..opening_end;
        let fence = text.as_bytes()[range.start];
        let fence_len = text[opening.clone()]
            .bytes()
            .take_while(|byte| *byte == fence)
            .count();
        let trimmed = text[range.clone()].trim_end_matches(['\r', '\n']);
        let closing = trimmed.rfind('\n').and_then(|line| {
            let last = &trimmed[line + 1..];
            let marker = last.trim_start_matches(' ');
            let count = marker.bytes().take_while(|byte| *byte == fence).count();
            (last.len() - marker.len() <= 3
                && count >= fence_len
                && marker[count..].trim_matches([' ', '\t']).is_empty())
            .then_some(range.start + line + 1..range.start + trimmed.len())
        });
        Self {
            range,
            opening,
            closing,
            language,
        }
    }

    /// Copy the original code bytes, excluding only its Markdown fences.
    pub fn content<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.content_range(source)?)
    }

    pub fn content_range(&self, source: &str) -> Option<Range<usize>> {
        let start =
            self.opening.end + usize::from(source.as_bytes().get(self.opening.end) == Some(&b'\n'));
        let end = self
            .closing
            .as_ref()
            .map_or(self.range.end, |closing| closing.start);
        source.get(start..end)?;
        Some(start..end)
    }
}

#[derive(Clone, Debug)]
pub struct QuotePrefix {
    pub range: Range<usize>,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub struct LinkCompletion {
    pub query: String,
    pub range: Range<usize>,
}

pub fn link_completion(text: &str, caret: usize) -> Option<LinkCompletion> {
    let caret = text
        .char_indices()
        .nth(caret)
        .map_or(text.len(), |(i, _)| i);
    let line_start = text[..caret].rfind('\n').map_or(0, |i| i + 1);
    let line = &text[line_start..caret];
    let start = line_start + line.rfind("[[")?;
    let query = &text[start + 2..caret];
    if query.contains("]]")
        || query.contains('|')
        || text[..start]
            .chars()
            .rev()
            .take_while(|c| *c == '\\')
            .count()
            % 2
            == 1
    {
        return None;
    }
    if line[..start - line_start]
        .bytes()
        .filter(|b| *b == b'`')
        .count()
        % 2
        == 1
    {
        return None;
    }
    if Markdown::parse(text)
        .spans
        .iter()
        .any(|span| span.range.contains(&start) && span.style.code)
    {
        return None;
    }
    let end = caret
        + if text[caret..].starts_with("]]") {
            2
        } else {
            0
        };
    Some(LinkCompletion {
        query: query.into(),
        range: start..end,
    })
}

pub fn complete_link(text: &mut String, completion: &LinkCompletion, target: &str) -> usize {
    if text
        .get(completion.range.clone())
        .is_none_or(|source| !source.starts_with("[["))
    {
        return text.chars().count();
    }
    let prefix = text[..completion.range.start].chars().count();
    let replacement = format!("[[{target}]]");
    let caret = prefix + replacement.chars().count();
    text.replace_range(completion.range.clone(), &replacement);
    caret
}

impl Markdown {
    pub fn parse(text: &str) -> Self {
        let mut styles = vec![Style::default(); text.len()];
        let mut heading_starts = Vec::new();
        let mut item_starts = Vec::new();
        let mut setext_markers = Vec::new();
        let mut code_blocks = Vec::new();
        let mut tables = Vec::new();
        let mut callouts = Vec::new();
        let mut tasks = Vec::new();
        let mut mark_stack = Vec::new();
        let mut marks = Vec::new();
        let mut current_item = 0;
        let mut task_positions = text.char_indices().enumerate().peekable();
        let mut table = None;
        for (event, range) in Parser::new_ext(
            text,
            Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS,
        )
        .into_offset_iter()
        {
            match event {
                Event::Start(Tag::Table(alignments)) => {
                    table = Some((range, alignments, Vec::new()));
                }
                Event::Start(Tag::TableHead | Tag::TableRow) => {
                    if let Some((_, _, rows)) = &mut table {
                        rows.push(range);
                    }
                }
                Event::End(TagEnd::Table) => {
                    if let Some((range, alignments, rows)) = table.take()
                        && let Some(table) =
                            crate::tables::Table::from_ranges(text, range, alignments, rows)
                    {
                        tables.push(table);
                    }
                }
                Event::Start(Tag::Heading { level, .. }) => {
                    heading_starts.push(text[..range.start].rfind('\n').map_or(0, |i| i + 1));
                    patch(&mut styles, range.clone(), |s| {
                        s.heading = level as u8;
                        s.strong = true;
                    });
                    let heading = &text[range.clone()];
                    if heading.starts_with('#') {
                        let prefix = heading.bytes().take_while(|b| *b == b'#').count();
                        let space = usize::from(heading.as_bytes().get(prefix) == Some(&b' '));
                        let markers = range.start..range.start + prefix + space;
                        hide(&mut styles, markers.clone());
                        patch(&mut styles, markers, |style| style.marker = true);
                    } else {
                        let trimmed = heading.trim_end();
                        if let Some(underline) = trimmed.rfind('\n') {
                            let markers = range.start + underline + 1..range.start + trimmed.len();
                            setext_markers.push(markers.clone());
                            patch(&mut styles, markers, |style| {
                                *style = Style {
                                    marker: true,
                                    ..Default::default()
                                };
                            });
                        }
                    }
                }
                Event::Start(Tag::Strong) => {
                    patch(&mut styles, range.clone(), |s| s.strong = true);
                    delimit(&mut styles, range, 2);
                }
                Event::Start(Tag::Emphasis) => {
                    patch(&mut styles, range.clone(), |s| s.italic = true);
                    delimit(&mut styles, range, 1);
                }
                Event::Start(Tag::Strikethrough) => {
                    patch(&mut styles, range.clone(), |s| s.strike = true);
                    delimit(&mut styles, range, 2);
                }
                Event::Start(Tag::Link { .. }) => {
                    patch(&mut styles, range.clone(), |s| s.link = true);
                    if let Some(end_label) = text[range.clone()].find("](") {
                        hide(&mut styles, range.start..range.start + 1);
                        hide(&mut styles, range.start + end_label..range.end);
                    }
                }
                Event::Code(_) => {
                    patch(&mut styles, range.clone(), |s| s.code = true);
                    let ticks = text[range.clone()]
                        .bytes()
                        .take_while(|b| *b == b'`')
                        .count();
                    delimit(&mut styles, range, ticks);
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    let line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
                    let root = text[line_start..range.start].trim().is_empty()
                        && !styles[range.start].quote;
                    patch(&mut styles, range.clone(), |s| {
                        *s = Style {
                            code: true,
                            block_code: true,
                            quote: s.quote,
                            ..Style::default()
                        };
                    });
                    if let CodeBlockKind::Fenced(language) = kind
                        && root
                    {
                        let block = CodeBlock::fenced(text, range, language.into_string());
                        patch(&mut styles, block.opening.clone(), |style| {
                            style.fence = true
                        });
                        if let Some(closing) = &block.closing {
                            patch(&mut styles, closing.clone(), |style| style.fence = true);
                        }
                        code_blocks.push(block);
                    }
                }
                Event::Start(Tag::BlockQuote(_)) => {
                    if let Some(callout) = crate::callouts::Callout::from_range(text, range.clone())
                    {
                        callouts.push(callout);
                    }
                    patch(&mut styles, range, |s| s.quote = true);
                }
                Event::TaskListMarker(checked) => {
                    let start = current_item;
                    let prefix = start..range.end;
                    patch(&mut styles, prefix.clone(), |style| {
                        style.task_marker = true
                    });
                    if checked {
                        let end = text[range.end..]
                            .find('\n')
                            .map_or(text.len(), |offset| range.end + offset);
                        patch(&mut styles, range.end..end, |style| {
                            style.task_checked = true
                        });
                    }
                    while task_positions
                        .peek()
                        .is_some_and(|(_, (byte, _))| *byte < start)
                    {
                        task_positions.next();
                    }
                    let character = task_positions.peek().unwrap().0;
                    tasks.push(Task {
                        prefix,
                        character,
                        checked,
                    });
                }
                Event::Start(Tag::Item) => {
                    current_item = range.start;
                    item_starts.push(text[..range.start].rfind('\n').map_or(0, |i| i + 1));
                    let item = &text[range.clone()];
                    if item.starts_with("- ") || item.starts_with("* ") || item.starts_with("+ ") {
                        patch(&mut styles, range.start..range.start + 1, |s| {
                            s.bullet = true;
                            s.marker = true;
                        });
                    }
                }
                Event::InlineHtml(tag) => match crate::inline_style::parse(&tag) {
                    Some(crate::inline_style::Tag::Open(element, style)) => {
                        mark_stack.push((element, range, style));
                    }
                    Some(crate::inline_style::Tag::Close(element))
                        if mark_stack
                            .last()
                            .is_some_and(|(open, _, _)| *open == element) =>
                    {
                        let (_, opening, style) = mark_stack.pop().unwrap();
                        marks.push((opening, range, style));
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        if let Ok(Some(properties)) = crate::metadata::frontmatter(text) {
            heading_starts.retain(|start| !properties.range.contains(start));
            item_starts.retain(|start| !properties.range.contains(start));
            setext_markers.retain(|range| !properties.range.contains(&range.start));
            code_blocks.retain(|block| !properties.range.contains(&block.range.start));
            tables.retain(|table| !properties.range.contains(&table.range.start));
            callouts.retain(|callout| !properties.range.contains(&callout.range.start));
            tasks.retain(|task| !properties.range.contains(&task.prefix.start));
            patch(&mut styles, properties.range, |style| {
                *style = Style {
                    property: true,
                    ..Default::default()
                }
            });
        }
        let mut highlights = Vec::new();
        let mut opening = None;
        let mut next = 0;
        while let Some(found) = text[next..].find("==") {
            let run = next + found;
            let count = text.as_bytes()[run..]
                .iter()
                .take_while(|byte| **byte == b'=')
                .count();
            next = run + count;
            let escaped = text.as_bytes()[..run]
                .iter()
                .rev()
                .take_while(|byte| **byte == b'\\')
                .count()
                % 2
                == 1;
            if escaped || !matches!(count, 2 | 4) || (count == 4 && opening.is_none()) {
                continue;
            }
            for index in (run..next).step_by(2) {
                if styles[index].code || styles[index].hidden || styles[index].property {
                    continue;
                }
                if let Some(start) = opening.take() {
                    let content = &text[start + 2..index];
                    let native = crate::highlight::NativeColor::prefix(content);
                    let marker_end = start + 2 + native.map_or(0, |(_, size)| size);
                    let content = &text[marker_end..index];
                    if (native.is_some() || !content.trim().is_empty())
                        && !content.contains("\n\n")
                        && !content.contains("\r\n\r\n")
                    {
                        patch(&mut styles, marker_end..index, |style| {
                            style.native_highlight = native.map(|(color, _)| color);
                            style.highlight = native
                                .is_none()
                                .then_some(crate::highlight::Color::Yellow.rgba());
                        });
                        for marker in [start..marker_end, index..index + 2] {
                            hide(&mut styles, marker.clone());
                            patch(&mut styles, marker, |style| style.marker = true);
                        }
                        highlights.push(start..index + 2);
                    } else {
                        opening = Some(index);
                    }
                } else {
                    opening = Some(index);
                }
            }
        }
        marks.sort_by_key(|(opening, _, _)| opening.start);
        for (opening, closing, html) in marks {
            if styles[opening.start].property || styles[closing.start].property {
                continue;
            }
            patch(&mut styles, opening.end..closing.start, |style| {
                style.html.overlay(html);
                if let Some(background) = html.background {
                    style.highlight = Some(background);
                }
            });
            for marker in [opening, closing] {
                hide(&mut styles, marker.clone());
                patch(&mut styles, marker, |style| style.marker = true);
            }
        }
        for callout in &callouts {
            patch(&mut styles, callout.header.clone(), |style| {
                style.callout_header = true;
            });
        }
        for (range, _) in crate::vault::wiki_spans(text) {
            patch(&mut styles, range.clone(), |s| s.link = true);
            let prefix = text[range.start + 2..range.end - 2]
                .find('|')
                .map(|i| i + 3)
                .unwrap_or(2);
            hide(&mut styles, range.start..range.start + prefix);
            hide(&mut styles, range.end - 2..range.end);
            patch(&mut styles, range.start..range.start + 2, |style| {
                style.marker = true
            });
            patch(&mut styles, range.end - 2..range.end, |style| {
                style.marker = true
            });
        }
        if text.contains("![") {
            for image in crate::assets::embedded_images(text) {
                patch(&mut styles, image.range, |style| style.image = true);
            }
        }
        let mut quotes = Vec::new();
        let mut offset = 0;
        for line in text.split_inclusive('\n') {
            let bytes = line.as_bytes();
            let mut cursor = 0;
            while bytes
                .get(cursor)
                .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
            {
                cursor += 1;
            }
            let start = cursor;
            let mut depth = 0;
            while bytes.get(cursor) == Some(&b'>')
                && styles.get(offset + cursor).is_some_and(|style| style.quote)
            {
                depth += 1;
                cursor += 1;
                if bytes
                    .get(cursor)
                    .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
                {
                    cursor += 1;
                }
                let mut nested = cursor;
                while bytes.get(nested) == Some(&b' ') {
                    nested += 1;
                }
                if bytes.get(nested) == Some(&b'>')
                    && styles.get(offset + nested).is_some_and(|style| !style.code)
                {
                    cursor = nested;
                } else {
                    break;
                }
            }
            if depth > 0 {
                let range = offset + start..offset + cursor;
                patch(&mut styles, range.clone(), |style| {
                    style.quote_marker = true;
                    style.marker = true;
                    style.code = false;
                });
                quotes.push(QuotePrefix { range, depth });
            }
            offset += line.len();
        }
        heading_starts.dedup();
        item_starts.dedup();
        let mut spans = Vec::new();
        let mut start = 0;
        for (index, _) in text.char_indices().skip(1) {
            if styles[index] != styles[start] {
                spans.push(Span {
                    range: start..index,
                    style: styles[start],
                });
                start = index;
            }
        }
        if start < text.len() {
            spans.push(Span {
                range: start..text.len(),
                style: styles[start],
            });
        }
        Self {
            spans,
            heading_starts,
            item_starts,
            quotes,
            setext_markers,
            code_blocks,
            tables,
            callouts,
            tasks,
            highlights,
        }
    }
}

fn patch(styles: &mut [Style], range: Range<usize>, mut f: impl FnMut(&mut Style)) {
    for style in &mut styles[range] {
        f(style);
    }
}
fn hide(styles: &mut [Style], range: Range<usize>) {
    patch(styles, range, |s| s.hidden = true);
}
fn delimit(styles: &mut [Style], range: Range<usize>, size: usize) {
    if range.len() >= size * 2 {
        hide(styles, range.start..range.start + size);
        hide(styles, range.end - size..range.end);
    }
}

/// Both input and output selections use Unicode scalar indices, as egui does.
pub fn surround(text: &mut String, selection: Range<usize>, marker: &str) -> Range<usize> {
    wrap(text, selection, marker, marker)
}

pub fn wrap(
    text: &mut String,
    selection: Range<usize>,
    opening: &str,
    closing: &str,
) -> Range<usize> {
    let offsets: Vec<_> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    let Some((&start, &end)) = offsets.get(selection.start).zip(offsets.get(selection.end)) else {
        return selection;
    };
    let size = opening.chars().count();
    if text[..start].ends_with(opening) && text[end..].starts_with(closing) {
        text.replace_range(end..end + closing.len(), "");
        text.replace_range(start - opening.len()..start, "");
        selection.start - size..selection.end - size
    } else {
        text.insert_str(end, closing);
        text.insert_str(start, opening);
        selection.start + size..selection.end + size
    }
}

/// Toggle one parsed task marker while preserving the rest of a note.
pub fn toggle_task(text: &mut String, ordinal: usize) -> bool {
    let body = crate::metadata::frontmatter(text)
        .ok()
        .flatten()
        .map_or(0, |properties| properties.range.end);
    let task = Parser::new_ext(&text[body..], Options::ENABLE_TASKLISTS)
        .into_offset_iter()
        .filter_map(|(event, range)| {
            if let Event::TaskListMarker(checked) = event {
                Some((range, checked))
            } else {
                None
            }
        })
        .nth(ordinal);
    let Some((range, checked)) = task else {
        return false;
    };
    let position = body + range.start + 1;
    text.replace_range(position..position + 1, if checked { " " } else { "x" });
    true
}

/// Continue Markdown structure on Enter. Character ranges match the native editor.
pub fn markdown_enter(text: &mut String, selection: Range<usize>) -> Option<Range<usize>> {
    let start = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain([text.len()])
        .nth(selection.start)?;
    let end = text
        .char_indices()
        .map(|(byte, _)| byte)
        .chain([text.len()])
        .nth(selection.end)?;
    if start > end || text[start..end].contains('\n') {
        return None;
    }
    let line_start = text[..start].rfind('\n').map_or(0, |byte| byte + 1);
    let line_end = text[end..].find('\n').map_or(text.len(), |byte| end + byte);
    if Markdown::parse(text)
        .spans
        .iter()
        .any(|span| span.style.code && span.range.contains(&start.saturating_sub(1)))
    {
        return None;
    }
    static PREFIX: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let prefix = PREFIX.get_or_init(|| {
        regex::Regex::new(
            r"^([ \t]*)((?:> ?)*)(?:(?:([-+*])|([0-9]+)([.)]))[ \t]+)?(\[[ xX]\][ \t]+)?",
        )
        .unwrap()
    });
    let line = &text[line_start..line_end];
    let captures = prefix.captures(line)?;
    let prefix_end = captures.get(0)?.end();
    let indent = &captures[1];
    let quote = &captures[2];
    let marker = captures.get(3).or_else(|| captures.get(4));
    if (marker.is_none() && quote.is_empty()) || start < line_start + prefix_end {
        return None;
    }
    if line[prefix_end..].trim().is_empty() {
        let remaining = if marker.is_some() {
            format!("{indent}{quote}")
        } else {
            format!(
                "{indent}{}",
                quote
                    .strip_suffix("> ")
                    .or_else(|| quote.strip_suffix('>'))
                    .unwrap_or("")
            )
        };
        let cursor = text[..line_start].chars().count() + remaining.chars().count();
        text.replace_range(line_start..line_end, &remaining);
        return Some(cursor..cursor);
    }
    let mut continuation = format!("\n{indent}{quote}");
    if let Some(number) = captures.get(4) {
        let number: u64 = number.as_str().parse().ok()?;
        continuation.push_str(&format!("{}{} ", number.checked_add(1)?, &captures[5]));
    } else if let Some(bullet) = captures.get(3) {
        continuation.push_str(bullet.as_str());
        continuation.push(' ');
    }
    if marker.is_some() && captures.get(6).is_some() {
        continuation.push_str("[ ] ");
    }
    let cursor = selection.start + continuation.chars().count();
    text.replace_range(start..end, &continuation);
    Some(cursor..cursor)
}

pub fn find_matches(text: &str, needle: &str) -> Vec<Range<usize>> {
    if needle.is_empty() {
        return Vec::new();
    }
    // Scalar-to-byte mapping avoids Unicode lowercase expansion corrupting selections.
    let haystack: Vec<char> = text.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    if needle.len() > haystack.len() {
        return Vec::new();
    }
    haystack
        .windows(needle.len())
        .enumerate()
        .filter_map(|(i, window)| {
            window
                .iter()
                .zip(&needle)
                .all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
                .then_some(i..i + needle.len())
        })
        .collect()
}
