use crate::editor::Markdown;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

#[derive(Clone, Debug)]
pub struct Section {
    pub key: String,
    pub level: u8,
    pub heading: Range<usize>,
    pub body: Range<usize>,
    pub heading_chars: Range<usize>,
    pub body_chars: Range<usize>,
}

#[derive(Default, Clone)]
pub struct Sections(pub Vec<Section>);

impl Sections {
    pub fn new(source: &str, parsed: &Markdown) -> Self {
        let mut sections = Vec::new();
        let mut occurrences = HashMap::new();
        // Headings come in text order, so one forward walk converts their bytes to characters.
        let mut counted = (0, 0);
        let mut chars_at = |byte: usize| {
            if byte < counted.0 {
                counted = (0, 0);
            }
            counted = (byte, counted.1 + source[counted.0..byte].chars().count());
            counted.1
        };
        let total_chars = source.chars().count();
        for &start in &parsed.heading_starts {
            let first_end = source[start..]
                .find('\n')
                .map_or(source.len(), |i| start + i + 1);
            let line = source[start..first_end].trim_end();
            let content = line.trim_start_matches(' ');
            if line.len() - content.len() > 3
                || content.starts_with(['>', '\t'])
                || content.starts_with("- ")
                || content.starts_with("* ")
                || content.starts_with("+ ")
            {
                continue;
            }
            let content_start = start + line.len() - content.len();
            let span = parsed
                .spans
                .partition_point(|span| span.range.end <= content_start);
            let Some(level) = parsed
                .spans
                .get(span)
                .map(|span| span.style.heading)
                .filter(|&level| level > 0)
            else {
                continue;
            };
            let end = parsed
                .setext_markers
                .iter()
                .find(|marker| marker.start == first_end)
                .map_or(first_end, |marker| {
                    source[marker.end..]
                        .find('\n')
                        .map_or(source.len(), |i| marker.end + i + 1)
                });
            let title = content.trim_start_matches('#').trim();
            let occurrence = occurrences.entry((level, title)).or_insert(0);
            let key = format!("{level}:{title}:{occurrence}");
            *occurrence += 1;
            let char_start = chars_at(start);
            let char_end = chars_at(end);
            sections.push(Section {
                key,
                level,
                heading: start..end,
                body: end..source.len(),
                heading_chars: char_start..char_end,
                body_chars: char_end..total_chars,
            });
        }
        for index in 0..sections.len() {
            if let Some(next) = sections[index + 1..]
                .iter()
                .find(|next| next.level <= sections[index].level)
            {
                let (end, chars) = (next.heading.start, next.heading_chars.start);
                sections[index].body.end = end;
                sections[index].body_chars.end = chars;
            }
        }
        Self(sections)
    }

    pub fn hidden(&self, collapsed: &HashSet<String>) -> Vec<Range<usize>> {
        if collapsed.is_empty() {
            return Vec::new();
        }
        let mut ranges: Vec<Range<usize>> = Vec::new();
        for section in &self.0 {
            if collapsed.contains(&section.key)
                && !section.body_chars.is_empty()
                && !ranges
                    .last()
                    .is_some_and(|range| range.contains(&section.heading_chars.start))
            {
                ranges.push(section.body_chars.clone());
            }
        }
        ranges
    }

    pub fn expand_at(&self, character: usize, collapsed: &mut HashSet<String>) {
        let eof = self.0.last().map_or(0, |section| section.body_chars.end);
        for section in &self.0 {
            if section.body_chars.contains(&character)
                || !section.body_chars.is_empty()
                    && character == eof
                    && section.body_chars.end == eof
            {
                collapsed.remove(&section.key);
            }
        }
    }

    pub fn reveal(&self, character: usize, collapsed: &mut HashSet<String>) {
        self.expand_at(character, collapsed);
        for section in &self.0 {
            if section.heading_chars.contains(&character) {
                collapsed.remove(&section.key);
            }
        }
    }
}
