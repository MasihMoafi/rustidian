use pulldown_cmark::{Event, Parser, Tag, TagEnd};

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

pub fn find_anchor(text: &str, fragment: &str) -> Option<usize> {
    if let Some(block) = fragment.strip_prefix('^') {
        let mut start = 0;
        let mut in_code = false;
        for (event, range) in Parser::new(text).into_offset_iter() {
            match event {
                Event::Start(Tag::CodeBlock(_)) => in_code = true,
                Event::End(TagEnd::CodeBlock) => in_code = false,
                Event::Start(Tag::Paragraph | Tag::Heading { .. } | Tag::Item) => {
                    start = range.start
                }
                Event::Text(value)
                    if !in_code
                        && value
                            .split_whitespace()
                            .last()
                            .is_some_and(|word| word == format!("^{block}")) =>
                {
                    return Some(text[..start].chars().count());
                }
                _ => {}
            }
        }
        None
    } else {
        let target = fragment.trim().to_lowercase();
        outline(text)
            .into_iter()
            .find(|heading| heading.title.to_lowercase() == target)
            .map(|heading| heading.character)
    }
}
