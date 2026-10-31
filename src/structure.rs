use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heading {
    pub level: usize,
    pub title: String,
    pub character: usize,
}

pub fn outline(text: &str) -> Vec<Heading> {
    let base = crate::metadata::frontmatter(text)
        .ok()
        .flatten()
        .map_or(0, |properties| properties.range.end);
    let source = text;
    let text = &source[base..];
    let base_character = source[..base].chars().count();
    let mut headings = Vec::new();
    let mut current: Option<Heading> = None;
    for (event, range) in Parser::new(text).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current = Some(Heading {
                    level: level as usize,
                    title: String::new(),
                    character: base_character + text[..range.start].chars().count(),
                })
            }
            Event::Text(value) | Event::Code(value) => {
                if let Some(current) = &mut current {
                    current.title.push_str(&value);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some(current) = &mut current {
                    current.title.push(' ');
                }
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(current) = current.take() {
                    headings.push(current);
                }
            }
            _ => {}
        }
    }
    headings
}

#[derive(Clone, Debug)]
pub struct Block {
    pub range: Range<usize>,
    pub text: String,
    pub label: String,
    pub id: Option<String>,
    insert: usize,
    inline: bool,
}

impl Block {
    pub fn id_edit(&self, id: &str) -> (usize, String) {
        (
            self.insert,
            if self.inline {
                format!(" ^{id}")
            } else {
                format!("\n\n^{id}")
            },
        )
    }
}

fn marker(text: &str) -> Option<&str> {
    let raw = text.trim_end();
    let word = raw.split_whitespace().last()?;
    let id = word.strip_prefix('^')?;
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return None;
    }
    Some(id)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    Paragraph,
    Heading,
    Item,
    List,
    Quote,
    Code,
    Table,
    Html,
}

/// Referenceable blocks, with standalone IDs attached to the preceding structured block.
pub fn blocks(text: &str) -> Vec<Block> {
    let base = crate::metadata::frontmatter(text)
        .ok()
        .flatten()
        .map_or(0, |properties| properties.range.end);
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let mut stack: Vec<(BlockKind, Range<usize>)> = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    for (event, range) in Parser::new_ext(&text[base..], options).into_offset_iter() {
        let kind = match &event {
            Event::Start(Tag::Paragraph) => Some(BlockKind::Paragraph),
            Event::Start(Tag::Heading { .. }) => Some(BlockKind::Heading),
            Event::Start(Tag::Item) => Some(BlockKind::Item),
            Event::Start(Tag::List(_)) => Some(BlockKind::List),
            Event::Start(Tag::BlockQuote(_)) => Some(BlockKind::Quote),
            Event::Start(Tag::CodeBlock(_)) => Some(BlockKind::Code),
            Event::Start(Tag::Table(_)) => Some(BlockKind::Table),
            Event::Start(Tag::HtmlBlock) => Some(BlockKind::Html),
            _ => None,
        };
        if let Some(kind) = kind {
            stack.push((kind, base + range.start..base + range.end));
            continue;
        }
        if !matches!(
            event,
            Event::End(
                TagEnd::Paragraph
                    | TagEnd::Heading(_)
                    | TagEnd::Item
                    | TagEnd::List(_)
                    | TagEnd::BlockQuote(_)
                    | TagEnd::CodeBlock
                    | TagEnd::Table
                    | TagEnd::HtmlBlock
            )
        ) {
            continue;
        }
        let Some((kind, range)) = stack.pop() else {
            continue;
        };
        if !stack.is_empty()
            && !(kind == BlockKind::Item
                && !stack.iter().any(|(kind, _)| *kind == BlockKind::Quote))
        {
            continue;
        }
        let raw = &text[range.clone()];
        if kind == BlockKind::Paragraph
            && let Some(id) = marker(raw)
            && raw.trim() == format!("^{id}")
        {
            if let Some(previous) = blocks
                .iter_mut()
                .filter(|block| {
                    block.range.end <= range.start
                        && text[block.range.end..range.start].trim().is_empty()
                })
                .max_by_key(|block| (block.range.end, std::cmp::Reverse(block.range.start)))
            {
                previous.id = Some(id.to_owned());
            }
            continue;
        }
        if blocks.iter().any(|block| block.range == range) {
            continue;
        }
        let inline = matches!(
            kind,
            BlockKind::Paragraph | BlockKind::Heading | BlockKind::Item
        );
        let line = if kind == BlockKind::Item {
            raw.lines().next().unwrap_or_default()
        } else {
            raw
        };
        let id = if inline {
            marker(line).map(str::to_owned)
        } else {
            None
        };
        let insert = range.start + line.trim_end().len();
        let label = Parser::new_ext(raw, options)
            .filter_map(|event| match event {
                Event::Text(value) | Event::Code(value) => Some(value.into_string()),
                Event::SoftBreak
                | Event::HardBreak
                | Event::End(TagEnd::Paragraph | TagEnd::Item | TagEnd::TableRow) => {
                    Some(" ".into())
                }
                _ => None,
            })
            .collect::<String>();
        let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
        blocks.push(Block {
            range,
            text: raw.into(),
            label,
            id,
            insert,
            inline,
        });
    }
    blocks.sort_by_key(|block| (block.range.start, block.range.end));
    blocks
}

pub fn block_anchors(text: &str) -> Vec<(String, usize)> {
    blocks(text)
        .into_iter()
        .filter_map(|block| {
            block
                .id
                .map(|id| (id, text[..block.range.start].chars().count()))
        })
        .collect()
}

pub fn find_anchor(text: &str, fragment: &str) -> Option<usize> {
    if let Some(block) = fragment.strip_prefix('^') {
        block_anchors(text)
            .into_iter()
            .find(|(id, _)| id == block)
            .map(|(_, character)| character)
    } else {
        let target = fragment.trim().to_lowercase();
        outline(text)
            .into_iter()
            .find(|heading| heading.title.to_lowercase() == target)
            .map(|heading| heading.character)
    }
}
