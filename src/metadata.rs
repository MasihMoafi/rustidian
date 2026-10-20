use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use regex::Regex;
use serde_yaml_ng::{Mapping, Value};
use std::{collections::HashSet, ops::Range, sync::OnceLock};

pub struct Frontmatter {
    pub range: Range<usize>,
    pub values: Mapping,
}
#[derive(Clone, Debug, Default)]
pub struct Metadata {
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
}

pub fn frontmatter(text: &str) -> Result<Option<Frontmatter>, String> {
    let mut lines = text.split_inclusive('\n');
    if lines
        .next()
        .is_none_or(|line| line.trim_end_matches(['\r', '\n']) != "---")
    {
        return Ok(None);
    }
    let first_end = text.find('\n').map_or(text.len(), |position| position + 1);
    let mut offset = first_end;
    for line in lines {
        if matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            let yaml = &text[first_end..offset];
            let mut options = serde_saphyr::Options::default();
            options.strict_booleans = true;
            let value: Value = serde_saphyr::from_str_with_options(yaml, options)
                .map_err(|error| format!("Invalid YAML properties: {error}"))?;
            let values = match value {
                Value::Mapping(values) => values,
                Value::Null => Mapping::new(),
                _ => return Err("Properties must be a YAML mapping.".into()),
            };
            return Ok(Some(Frontmatter {
                range: 0..offset + line.len(),
                values,
            }));
        }
        offset += line.len();
    }
    Err("The opening properties block has no closing --- line.".into())
}

pub fn set_property(text: &mut String, name: &str, value: Value) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['\r', '\n']) {
        return Err("Use a nonempty property name on one line.".into());
    }
    let existing = frontmatter(text)?;
    let (range, mut values) = existing.map_or((0..0, Mapping::new()), |properties| {
        (properties.range, properties.values)
    });
    values.insert(Value::String(name.into()), value);
    write_properties(text, range, values)
}

pub fn remove_property(text: &mut String, name: &str) -> Result<(), String> {
    let Some(mut properties) = frontmatter(text)? else {
        return Ok(());
    };
    properties.values.remove(Value::String(name.into()));
    write_properties(text, properties.range, properties.values)
}

fn write_properties(text: &mut String, range: Range<usize>, values: Mapping) -> Result<(), String> {
    let yaml = if values.is_empty() {
        String::new()
    } else {
        serde_yaml_ng::to_string(&values).map_err(|error| error.to_string())?
    };
    let header = format!("---\n{yaml}---\n");
    text.replace_range(range, &header);
    Ok(())
}

fn strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Sequence(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

pub fn analyze(text: &str) -> Metadata {
    let properties = frontmatter(text).ok().flatten();
    let mut metadata = Metadata::default();
    let mut body = text;
    if let Some(properties) = &properties {
        metadata.aliases = strings(properties.values.get(Value::String("aliases".into())));
        metadata.tags = strings(properties.values.get(Value::String("tags".into())))
            .into_iter()
            .map(|tag| tag.trim_start_matches('#').to_owned())
            .collect();
        body = &text[properties.range.end..];
    }
    static TAG: OnceLock<Regex> = OnceLock::new();
    let tag = TAG.get_or_init(|| {
        Regex::new(r"(?:^|[^\p{L}\p{N}\p{M}_/:-])#([\p{L}\p{N}\p{M}\p{S}_/-]+)").unwrap()
    });
    // Most notes have no hashtags; avoid parsing all Markdown just to establish that.
    if tag.is_match(body) {
        let mut in_code = false;
        for event in Parser::new(body) {
            match &event {
                Event::Start(Tag::CodeBlock(_)) => in_code = true,
                Event::End(TagEnd::CodeBlock) => in_code = false,
                _ => {}
            }
            if let Event::Text(value) = event
                && !in_code
            {
                for captures in tag.captures_iter(&value) {
                    let value = captures[1].to_owned();
                    if value.chars().any(|character| !character.is_numeric()) {
                        metadata.tags.push(value);
                    }
                }
            }
        }
    }
    let mut seen = HashSet::new();
    metadata.tags.retain(|tag| seen.insert(tag.to_lowercase()));
    metadata
}
