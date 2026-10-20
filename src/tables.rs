//! Markdown table edits retain source outside the requested cell or table.
use pulldown_cmark::Alignment;
use std::ops::Range;

#[derive(Clone, Debug)]
pub struct Row {
    pub cells: Vec<Range<usize>>,
    fields: Vec<Range<usize>>,
    line: Range<usize>,
    prefix: String,
    ending: String,
}

#[derive(Clone, Debug)]
pub struct Table {
    pub range: Range<usize>,
    pub alignments: Vec<Alignment>,
    pub rows: Vec<Row>,
    snapshot: String,
    separator: Row,
}

/// Retain typed padding until the field closes; Markdown trims it when reparsed.
#[derive(Clone, Debug)]
pub struct CellDraft {
    pub text: String,
    row: usize,
    column: usize,
    range: Range<usize>,
    encoded: String,
}

impl CellDraft {
    pub fn is_current(&self, source: &str) -> bool {
        source.get(self.range.clone()) == Some(self.encoded.as_str())
    }
}

#[derive(Clone, Debug)]
pub enum Action {
    Cell {
        row: usize,
        column: usize,
        text: String,
    },
    InsertRow(usize),
    RemoveRow(usize),
    MoveRow {
        from: usize,
        to: usize,
    },
    InsertColumn(usize),
    RemoveColumn(usize),
    MoveColumn {
        from: usize,
        to: usize,
    },
    Align {
        column: usize,
        alignment: Alignment,
    },
    Sort {
        column: usize,
        descending: bool,
    },
}

impl Table {
    pub fn is_current(&self, source: &str) -> bool {
        source.get(self.range.clone()) == Some(self.snapshot.as_str())
    }
    /// Ranges come from the existing Markdown parser, never a pipe-line heuristic.
    pub fn from_ranges(
        source: &str,
        range: Range<usize>,
        alignments: Vec<Alignment>,
        row_ranges: Vec<Range<usize>>,
    ) -> Option<Self> {
        if alignments.is_empty() || row_ranges.is_empty() {
            return None;
        }
        let rows: Vec<_> = row_ranges
            .into_iter()
            .map(|range| Row::parse(source, range.start))
            .collect::<Option<_>>()?;
        let separator_start = rows[0].line.end;
        let separator_end = source[separator_start..]
            .find('\n')
            .map_or(source.len(), |end| separator_start + end + 1);
        if separator_start >= range.end || separator_end > range.end {
            return None;
        }
        let marker = source[separator_start..separator_end].find(['|', '-', ':'])?;
        let separator = Row::parse(source, separator_start + marker)?;
        if separator.cells.len() != alignments.len() {
            return None;
        }
        let range = rows[0].line.start..rows.last()?.line.end.max(separator_end);
        Some(Self {
            snapshot: source.get(range.clone())?.into(),
            range,
            alignments,
            rows,
            separator,
        })
    }

    pub fn cell<'a>(&self, source: &'a str, row: usize, column: usize) -> &'a str {
        self.rows
            .get(row)
            .and_then(|row| row.cells.get(column))
            .and_then(|range| source.get(range.clone()))
            .unwrap_or("")
    }

    /// A field edits literal pipes; only the table's delimiter escape is decoded.
    pub fn cell_text(&self, source: &str, row: usize, column: usize) -> String {
        decode_pipes(self.cell(source, row, column))
    }

    pub fn edit_cell(
        &self,
        source: &mut String,
        row: usize,
        column: usize,
        text: String,
        draft: Option<&CellDraft>,
    ) -> Result<CellDraft, &'static str> {
        if !self.is_current(source) {
            return Err("The table changed. Reopen the cell before editing it.");
        }
        if column >= self.alignments.len() || row >= self.rows.len() || text.contains(['\n', '\r'])
        {
            return Err("A table cell must be one Markdown line in an existing column.");
        }
        let encoded = encode_pipes(&text);
        let target = &self.rows[row];
        let tracked = draft.filter(|draft| {
            draft.row == row
                && draft.column == column
                && draft.is_current(source)
                && target.fields.get(column).is_some_and(|field| {
                    field.start <= draft.range.start && field.end >= draft.range.end
                })
        });
        let start = if let Some(range) = tracked
            .map(|draft| &draft.range)
            .or_else(|| target.cells.get(column))
        {
            let start = range.start;
            source.replace_range(range.clone(), &encoded);
            start
        } else {
            // GFM permits short rows. Materialize only this row's missing cells.
            let mut replacement = format!("{}|", target.prefix);
            let mut start = 0;
            for index in 0..self.alignments.len() {
                replacement.push(' ');
                if index == column {
                    start = target.line.start + replacement.len();
                    replacement.push_str(&encoded);
                } else {
                    replacement.push_str(self.cell(source, row, index));
                }
                replacement.push_str(" |");
            }
            replacement.push_str(&target.ending);
            source.replace_range(target.line.clone(), &replacement);
            start
        };
        Ok(CellDraft {
            row,
            column,
            range: start..start + encoded.len(),
            text,
            encoded,
        })
    }

    pub fn apply(&self, source: &mut String, action: Action) -> Result<(), &'static str> {
        if let Action::Cell { row, column, text } = action {
            return self.edit_cell(source, row, column, text, None).map(|_| ());
        }
        if !self.is_current(source) {
            return Err("The table changed. Reopen the cell before editing it.");
        }
        let columns = self.alignments.len();
        let mut rows: Vec<Vec<String>> = self
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                (0..row.cells.len().max(columns))
                    .map(|column| self.cell(source, index, column).to_owned())
                    .collect()
            })
            .collect();
        let mut alignments = self.alignments.clone();
        match action {
            Action::InsertRow(index) if index > 0 && index <= rows.len() => {
                rows.insert(index, vec![String::new(); columns]);
            }
            Action::RemoveRow(index) if index > 0 && index < rows.len() => {
                rows.remove(index);
            }
            Action::MoveRow { from, to }
                if from > 0 && to > 0 && from < rows.len() && to < rows.len() =>
            {
                let row = rows.remove(from);
                rows.insert(to, row);
            }
            Action::InsertColumn(index) if index <= columns => {
                for row in &mut rows {
                    row.insert(index, String::new());
                }
                alignments.insert(index, Alignment::None);
            }
            Action::RemoveColumn(index) if columns > 1 && index < columns => {
                for row in &mut rows {
                    row.remove(index);
                }
                alignments.remove(index);
            }
            Action::MoveColumn { from, to } if from < columns && to < columns => {
                for row in &mut rows {
                    let cell = row.remove(from);
                    row.insert(to, cell);
                }
                let alignment = alignments.remove(from);
                alignments.insert(to, alignment);
            }
            Action::Align { column, alignment } if column < columns => {
                // Alignment changes need not reformat any content row.
                let marker = match alignment {
                    Alignment::Left => ":---",
                    Alignment::Center => ":---:",
                    Alignment::Right => "---:",
                    Alignment::None => "---",
                };
                source.replace_range(self.separator.cells[column].clone(), marker);
                return Ok(());
            }
            Action::Sort { column, descending } if column < columns => {
                let mut entries: Vec<_> = rows
                    .drain(1..)
                    .map(|row| {
                        let key = sort_text(&row[column]);
                        let number = key
                            .trim()
                            .parse::<f64>()
                            .ok()
                            .filter(|number| number.is_finite());
                        (row, key.to_lowercase(), number)
                    })
                    .collect();
                let numeric = entries.iter().all(|(_, _, number)| number.is_some());
                entries.sort_by(|a, b| {
                    let order = if numeric {
                        a.2.unwrap().total_cmp(&b.2.unwrap())
                    } else {
                        a.1.cmp(&b.1)
                    };
                    if descending { order.reverse() } else { order }
                });
                rows.extend(entries.into_iter().map(|(row, _, _)| row));
            }
            _ => {
                return Err(
                    "That table operation would remove its header or last column, or uses a missing cell.",
                );
            }
        }
        let newline = if self.separator.ending.is_empty() {
            "\n"
        } else {
            &self.separator.ending
        };
        let trailing = source[self.range.clone()].ends_with('\n');
        let continuation = &self.separator.prefix;
        let mut replacement = String::new();
        for (index, row) in rows.iter().enumerate() {
            let prefix = if index == 0 {
                &self.rows[0].prefix
            } else {
                continuation
            };
            write_row(&mut replacement, prefix, row.iter().map(String::as_str));
            replacement.push_str(newline);
            if index == 0 {
                write_row(
                    &mut replacement,
                    continuation,
                    alignments.iter().map(|a| match a {
                        Alignment::Left => ":---",
                        Alignment::Center => ":---:",
                        Alignment::Right => "---:",
                        Alignment::None => "---",
                    }),
                );
                replacement.push_str(newline);
            }
        }
        if !trailing {
            replacement.truncate(replacement.len() - newline.len());
        }
        source.replace_range(self.range.clone(), &replacement);
        Ok(())
    }
}

impl Row {
    fn parse(source: &str, content_start: usize) -> Option<Self> {
        let start = source
            .get(..content_start)?
            .rfind('\n')
            .map_or(0, |i| i + 1);
        let end = source[content_start..]
            .find('\n')
            .map_or(source.len(), |i| content_start + i + 1);
        let content_end = source[..end].trim_end_matches(['\n', '\r']).len();
        let content = source.get(content_start..content_end)?;
        let trimmed = content.trim_matches([' ', '\t']);
        let mut first =
            content_start + content.len() - content.trim_start_matches([' ', '\t']).len();
        let mut last = first + trimmed.len();
        if source.as_bytes().get(first) == Some(&b'|') {
            first += 1;
        }
        if last > first && source.as_bytes()[last - 1] == b'|' && !escaped(source, last - 1) {
            last -= 1;
        }
        let mut fields = Vec::new();
        let mut cell_start = first;
        for (offset, character) in source[first..last].char_indices() {
            let byte = first + offset;
            if character == '|' && !escaped(source, byte) {
                fields.push(cell_start..byte);
                cell_start = byte + 1;
            }
        }
        fields.push(cell_start..last);
        Some(Self {
            cells: fields
                .iter()
                .map(|range| trim_range(source, range.clone()))
                .collect(),
            fields,
            line: start..end,
            prefix: source[start..content_start].into(),
            ending: source[content_end..end].into(),
        })
    }
}

fn trim_range(source: &str, range: Range<usize>) -> Range<usize> {
    let text = &source[range.clone()];
    if text.trim_matches([' ', '\t']).is_empty() {
        let middle = range.start + text.len() / 2;
        return middle..middle;
    }
    let start = range.start + text.len() - text.trim_start_matches([' ', '\t']).len();
    start..start + text.trim_matches([' ', '\t']).len()
}

fn escaped(source: &str, byte: usize) -> bool {
    source[..byte]
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'\\')
        .count()
        % 2
        == 1
}

fn encode_pipes(text: &str) -> String {
    let mut result = String::new();
    for (index, character) in text.char_indices() {
        if character == '|' && !escaped(text, index) {
            result.push('\\');
        }
        result.push(character);
    }
    result
}

fn decode_pipes(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\\' && chars.peek() == Some(&'|') {
            continue;
        }
        result.push(character);
        if character == '\\' && chars.peek() == Some(&'\\') {
            result.push(chars.next().unwrap());
        }
    }
    result
}

fn write_row<'a>(output: &mut String, prefix: &str, cells: impl Iterator<Item = &'a str>) {
    output.push_str(prefix);
    output.push('|');
    for cell in cells {
        output.push(' ');
        output.push_str(cell);
        output.push_str(" |");
    }
}

fn sort_text(source: &str) -> String {
    let mut display = decode_pipes(source);
    for (range, target) in crate::vault::wiki_spans(&display).into_iter().rev() {
        display.replace_range(
            range,
            target
                .split_once('|')
                .map_or(target.as_str(), |(_, label)| label),
        );
    }
    pulldown_cmark::Parser::new_ext(&display, pulldown_cmark::Options::ENABLE_STRIKETHROUGH)
        .filter_map(|event| match event {
            pulldown_cmark::Event::Text(text) | pulldown_cmark::Event::Code(text) => {
                Some(text.into_string())
            }
            pulldown_cmark::Event::SoftBreak | pulldown_cmark::Event::HardBreak => Some(" ".into()),
            _ => None,
        })
        .collect()
}
