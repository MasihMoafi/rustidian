//! Text for read aloud: whole sentences, speakable chunks, and a selection rounded out to
//! the sentences it touches.
use std::ops::Range;

/// Words whose final period does not end a sentence ("Dr. Smith", "e.g. this").
const ABBREVIATIONS: &[&str] = &[
    "mr", "mrs", "ms", "dr", "prof", "sr", "jr", "st", "vs", "etc", "e.g", "i.e", "fig", "no",
    "vol", "approx", "inc", "ltd", "co", "u.s", "u.k", "al",
];

/// The longest chunk handed to the voice at once; longer sentences split at a comma or space.
const LONGEST: usize = 320;

/// Collapse runs of spaces and tabs into one space; keep one newline between lines (blocks).
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let (mut space, mut newline) = (false, false);
    for character in text.chars() {
        if character == '\n' || character == '\r' {
            newline = true;
        } else if character.is_whitespace() {
            space = true;
        } else {
            if !out.is_empty() {
                if newline {
                    out.push('\n');
                } else if space {
                    out.push(' ');
                }
            }
            (space, newline) = (false, false);
            out.push(character);
        }
    }
    out
}

fn abbreviation(before: &str) -> bool {
    let word = before
        .rsplit(|character: char| character.is_whitespace() || character == '(')
        .next()
        .unwrap_or("");
    let initial = word.chars().count() == 1 && word.chars().all(char::is_uppercase);
    initial || ABBREVIATIONS.contains(&word.to_lowercase().as_str())
}

fn push_trimmed(ranges: &mut Vec<Range<usize>>, text: &str, start: usize, end: usize) {
    let slice = &text[start..end];
    let first = start + (slice.len() - slice.trim_start().len());
    let last = end - (slice.len() - slice.trim_end().len());
    if first < last {
        ranges.push(first..last);
    }
}

/// Byte ranges of the sentences in normalized `text`. A line break always ends one.
pub fn sentence_ranges(text: &str) -> Vec<Range<usize>> {
    let characters: Vec<(usize, char)> = text.char_indices().collect();
    let mut ranges = Vec::new();
    let (mut start, mut index) = (0, 0);
    while index < characters.len() {
        let (at, character) = characters[index];
        if character == '\n' {
            push_trimmed(&mut ranges, text, start, at);
            start = at + 1;
            index += 1;
            continue;
        }
        if !matches!(character, '.' | '!' | '?' | '…') {
            index += 1;
            continue;
        }
        // Take the whole run of terminators and closing quotes or brackets.
        let mut next = index + 1;
        while characters.get(next).is_some_and(|(_, character)| {
            matches!(
                character,
                '.' | '!' | '?' | '…' | '"' | '\'' | '”' | '’' | ')' | ']'
            )
        }) {
            next += 1;
        }
        let followed_by_start = characters.get(next).is_some_and(|(_, c)| *c == ' ')
            && characters.get(next + 1).is_some_and(|(_, c)| {
                c.is_uppercase()
                    || c.is_ascii_digit()
                    || matches!(c, '"' | '“' | '\'' | '‘' | '(' | '[')
            });
        if followed_by_start && !(character == '.' && abbreviation(&text[start..at])) {
            let end = characters[next].0;
            push_trimmed(&mut ranges, text, start, end);
            start = end + 1;
            index = next + 1;
        } else {
            index = next;
        }
    }
    push_trimmed(&mut ranges, text, start, text.len());
    ranges
}

/// Text the voice can say: no web addresses, normalized spacing.
fn speakable(text: &str) -> String {
    let lines: Vec<String> = text
        .lines()
        .map(|line| {
            line.split(' ')
                .filter(|word| !word.starts_with("http://") && !word.starts_with("https://"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    normalize(&lines.join("\n"))
}

/// Split one long sentence at a comma, semicolon or space, each piece at most `LONGEST`.
fn split_long(sentence: &str, out: &mut Vec<String>) {
    let mut rest = sentence.trim();
    while rest.len() > LONGEST {
        let mut cut = LONGEST;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        let window = &rest[..cut];
        let at = window
            .rfind([',', ';', ':'])
            .filter(|at| *at > LONGEST / 3)
            .map(|at| at + 1)
            .or_else(|| window.rfind(' '))
            .unwrap_or(cut);
        out.push(rest[..at].trim().to_owned());
        rest = rest[at..].trim_start();
    }
    if !rest.is_empty() {
        out.push(rest.to_owned());
    }
}

/// The chunks read aloud in order: one whole sentence each, long ones split.
pub fn chunks(text: &str) -> Vec<String> {
    let text = speakable(text);
    let mut out = Vec::new();
    for range in sentence_ranges(&text) {
        split_long(&text[range], &mut out);
    }
    out.retain(|chunk| chunk.chars().any(char::is_alphanumeric));
    out
}

/// `selected`, widened to the whole sentences of `full` it touches. Text not found in
/// `full` comes back normalized but unwidened.
pub fn snap(full: &str, selected: &str) -> String {
    let (full, selected) = (normalize(full), normalize(selected));
    let Some(start) = (!selected.is_empty())
        .then(|| full.find(&selected))
        .flatten()
    else {
        return selected;
    };
    let end = start + selected.len();
    let ranges = sentence_ranges(&full);
    let first = ranges
        .iter()
        .find(|range| range.end > start)
        .map_or(start, |range| range.start.min(start));
    let last = ranges
        .iter()
        .rev()
        .find(|range| range.start < end)
        .map_or(end, |range| range.end.max(end));
    full[first..last].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sentences(text: &str) -> Vec<&str> {
        sentence_ranges(text)
            .into_iter()
            .map(|range| &text[range])
            .collect()
    }

    #[test]
    fn sentences_end_at_terminators_but_not_abbreviations_or_initials() {
        assert_eq!(
            sentences(
                "Dr. Smith met J. Doe at 5 p.m. today. Was it late? \"Yes,\" she said. Fine!"
            ),
            [
                "Dr. Smith met J. Doe at 5 p.m. today.",
                "Was it late?",
                "\"Yes,\" she said.",
                "Fine!"
            ]
        );
        assert_eq!(
            sentences("Use tools, e.g. rg. Then stop."),
            ["Use tools, e.g. rg.", "Then stop."]
        );
        assert_eq!(
            sentences("A heading\nIts first line. Second."),
            ["A heading", "Its first line.", "Second."]
        );
        assert_eq!(
            sentences("version 1.5 shipped. 2 bugs remain."),
            ["version 1.5 shipped.", "2 bugs remain."]
        );
    }

    #[test]
    fn chunks_are_whole_sentences_without_links_and_long_ones_split() {
        assert_eq!(
            chunks("See https://example.com for more.  It   works.\n\n\nDone"),
            ["See for more.", "It works.", "Done"]
        );
        let long = format!("{}, and then it ends.", "word ".repeat(90).trim());
        let pieces = chunks(&long);
        assert!(
            pieces.len() > 1 && pieces.iter().all(|piece| piece.len() <= LONGEST),
            "{pieces:?}"
        );
        assert_eq!(pieces.join(" "), normalize(&long));
        assert!(chunks("--- • ---").is_empty());
    }

    #[test]
    fn a_selection_widens_to_the_sentences_it_touches() {
        let full = "First one here. The second sentence is long.\nA new paragraph starts. It ends.";
        assert_eq!(
            snap(full, "cond sentence is"),
            "The second sentence is long."
        );
        assert_eq!(
            snap(full, "is long.\nA new"),
            "The second sentence is long.\nA new paragraph starts."
        );
        assert_eq!(snap(full, "  It   ends. "), "It ends.");
        assert_eq!(snap(full, "not in the note"), "not in the note");
    }
}
