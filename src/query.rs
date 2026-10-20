use crate::vault::Note;
use regex::{Regex, RegexBuilder};
use serde_yaml_ng::Value;

enum Expression {
    And(Vec<Self>),
    Or(Vec<Self>),
    Not(Box<Self>),
    Line(Box<Self>),
    Term(Term),
}
enum Term {
    Text(String),
    Content(String),
    Path(String),
    File(String),
    Tag(String),
    Regex(Regex, String),
    Property(String, Option<String>),
}
#[derive(Clone, Debug, PartialEq)]
enum Token {
    Word(String),
    Open,
    Close,
    Or,
}
pub struct Query {
    source: String,
    expression: Expression,
}

#[derive(Default)]
pub struct Cache {
    key: Option<String>,
    pub paths: Vec<std::path::PathBuf>,
    pub error: Option<String>,
}
impl Cache {
    pub fn invalidate(&mut self) {
        self.key = None;
    }
    pub fn update(&mut self, query: &str, vault: &crate::vault::Vault) {
        if self.key.as_deref() == Some(query) {
            return;
        }
        self.key = Some(query.into());
        match vault.query(query) {
            Ok(notes) => {
                self.paths = notes.iter().map(|note| note.path.clone()).collect();
                self.error = None;
            }
            Err(error) => {
                self.paths.clear();
                self.error = Some(error);
            }
        }
    }
}

impl Query {
    pub fn parse(source: &str) -> Result<Self, String> {
        let tokens = tokenize(source)?;
        let mut parser = Parse {
            tokens,
            position: 0,
        };
        let expression = parser.or(None)?;
        if parser.position != parser.tokens.len() {
            return Err("An unexpected closing parenthesis remains.".into());
        }
        Ok(Self {
            source: source.into(),
            expression,
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn matches(&self, note: &Note) -> bool {
        self.expression
            .matches(note, &note.text, note.folded_text())
    }
    pub fn count(&self, notes: &[Note]) -> usize {
        notes.iter().filter(|note| self.matches(note)).count()
    }
}

impl Expression {
    fn matches(&self, note: &Note, text: &str, folded: &str) -> bool {
        match self {
            Self::And(terms) => terms.iter().all(|term| term.matches(note, text, folded)),
            Self::Or(terms) => terms.iter().any(|term| term.matches(note, text, folded)),
            Self::Not(term) => !term.matches(note, text, folded),
            Self::Line(term) => text
                .lines()
                .any(|line| term.matches(note, line, &line.to_lowercase())),
            Self::Term(term) => term.matches(note, text, folded),
        }
    }
}

impl Term {
    fn new(value: &str, default: Option<&str>) -> Result<Self, String> {
        if value.starts_with('[') && value.ends_with(']') {
            let property = &value[1..value.len() - 1];
            let (key, value) = property
                .split_once(':')
                .map_or((property, None), |(key, value)| {
                    (key, Some(value.trim().trim_matches('"').to_owned()))
                });
            return Ok(Self::Property(key.trim().trim_matches('"').into(), value));
        }
        let (field, value) = match value.split_once(':') {
            Some((field @ ("file" | "path" | "content" | "tag" | "line"), value)) => {
                (Some(field), value)
            }
            _ => (default, value),
        };
        if value.starts_with('/') && value.ends_with('/') && value.len() >= 2 {
            return RegexBuilder::new(&value[1..value.len() - 1])
                .case_insensitive(true)
                .build()
                .map(|regex| Self::Regex(regex, field.unwrap_or("content").into()))
                .map_err(|error| format!("Invalid regular expression: {error}"));
        }
        let value = value.to_lowercase();
        Ok(match field {
            Some("file") => Self::File(value),
            Some("path") => Self::Path(value),
            Some("content" | "line") => Self::Content(value),
            Some("tag") => Self::Tag(value.trim_start_matches('#').to_owned()),
            _ => Self::Text(value),
        })
    }
    fn matches(&self, note: &Note, text: &str, folded: &str) -> bool {
        match self {
            Self::Text(value) => {
                folded.contains(value)
                    || note.path.to_string_lossy().to_lowercase().contains(value)
                    || note
                        .aliases
                        .iter()
                        .any(|alias| alias.to_lowercase().contains(value))
            }
            Self::Content(value) => folded.contains(value),
            Self::Path(value) => note.path.to_string_lossy().to_lowercase().contains(value),
            Self::File(value) => note.title().to_lowercase().contains(value),
            Self::Tag(value) => note.tags.iter().any(|tag| {
                let tag = tag.to_lowercase();
                tag == *value || tag.starts_with(&format!("{value}/"))
            }),
            Self::Regex(regex, field) => match field.as_str() {
                "file" => regex.is_match(&note.title()),
                "path" => regex.is_match(&note.path.to_string_lossy()),
                "tag" => note.tags.iter().any(|tag| regex.is_match(tag)),
                _ => regex.is_match(text),
            },
            Self::Property(key, expected) => crate::metadata::frontmatter(&note.text)
                .ok()
                .flatten()
                .is_some_and(|properties| {
                    properties.values.iter().any(|(name, value)| {
                        name.as_str()
                            .is_some_and(|name| name.eq_ignore_ascii_case(key))
                            && expected
                                .as_ref()
                                .is_none_or(|expected| property_matches(value, expected))
                    })
                }),
        }
    }
}

fn property_matches(value: &Value, expected: &str) -> bool {
    if let Some(number) = value.as_f64() {
        for operator in [">=", "<=", ">", "<", "!=", "="] {
            if let Some(expected) = expected.strip_prefix(operator)
                && let Ok(expected) = expected.trim().parse::<f64>()
            {
                return match operator {
                    ">=" => number >= expected,
                    "<=" => number <= expected,
                    ">" => number > expected,
                    "<" => number < expected,
                    "!=" => number != expected,
                    _ => number == expected,
                };
            }
        }
    }
    match value {
        Value::Sequence(values) => values.iter().any(|value| property_matches(value, expected)),
        Value::String(value) => value.to_lowercase() == expected.to_lowercase(),
        Value::Null => expected.is_empty(),
        _ => serde_yaml_ng::to_string(value)
            .is_ok_and(|value| value.trim().eq_ignore_ascii_case(expected)),
    }
}

struct Parse {
    tokens: Vec<Token>,
    position: usize,
}
impl Parse {
    fn or(&mut self, default: Option<&str>) -> Result<Expression, String> {
        if self.tokens.is_empty() {
            return Ok(Expression::And(Vec::new()));
        }
        let mut groups = vec![self.and(default)?];
        while self.tokens.get(self.position) == Some(&Token::Or) {
            self.position += 1;
            groups.push(self.and(default)?);
        }
        Ok(if groups.len() == 1 {
            groups.remove(0)
        } else {
            Expression::Or(groups)
        })
    }
    fn and(&mut self, default: Option<&str>) -> Result<Expression, String> {
        let mut terms = Vec::new();
        while let Some(token) = self.tokens.get(self.position).cloned() {
            if matches!(token, Token::Close | Token::Or) {
                break;
            }
            self.position += 1;
            match token {
                Token::Open => terms.push(self.group(default)?),
                Token::Word(word) => {
                    let (negative, word) = word
                        .strip_prefix('-')
                        .map_or((false, word.as_str()), |word| (true, word));
                    let term = if word.is_empty()
                        && negative
                        && self.tokens.get(self.position) == Some(&Token::Open)
                    {
                        self.position += 1;
                        self.group(default)?
                    } else if word.ends_with(':')
                        && self.tokens.get(self.position) == Some(&Token::Open)
                    {
                        self.position += 1;
                        let field = &word[..word.len() - 1];
                        if field == "line" {
                            Expression::Line(Box::new(self.group(Some("content"))?))
                        } else {
                            self.group(Some(field))?
                        }
                    } else {
                        Expression::Term(Term::new(word, default)?)
                    };
                    terms.push(if negative {
                        Expression::Not(Box::new(term))
                    } else {
                        term
                    });
                }
                _ => unreachable!(),
            }
        }
        if terms.is_empty() {
            return Err("Enter a search term on each side of OR and inside groups.".into());
        }
        Ok(Expression::And(terms))
    }
    fn group(&mut self, default: Option<&str>) -> Result<Expression, String> {
        let group = self.or(default)?;
        if self.tokens.get(self.position) != Some(&Token::Close) {
            return Err("Close the search group with a parenthesis.".into());
        }
        self.position += 1;
        Ok(group)
    }
}

fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let mut characters = source.chars().peekable();
    while let Some(character) = characters.next() {
        if character.is_whitespace() {
            continue;
        }
        if character == '(' {
            tokens.push(Token::Open);
            continue;
        }
        if character == ')' {
            tokens.push(Token::Close);
            continue;
        }
        let mut word = String::new();
        let mut quote = false;
        let mut quoted = false;
        let mut bracket = false;
        let mut regex = false;
        let mut next = Some(character);
        while let Some(character) = next {
            if character == '"' && !regex {
                quote = !quote;
                quoted = true;
            } else if character == '[' && !quote && word.is_empty() {
                bracket = true;
                word.push(character);
            } else if character == ']' && bracket && !quote {
                bracket = false;
                word.push(character);
            } else if character == '/'
                && !quote
                && !bracket
                && (regex || word.is_empty() || word.ends_with(':') || word == "-")
            {
                regex = !regex;
                word.push(character);
            } else if character == '\\' && regex {
                if let Some(next) = characters.next() {
                    if next != '/' {
                        word.push('\\');
                    }
                    word.push(next);
                }
            } else if character == '\\' && quote {
                if let Some(character) = characters.next() {
                    word.push(character);
                }
            } else {
                word.push(character);
            }
            if !quote
                && !bracket
                && !regex
                && characters.peek().is_none_or(|character| {
                    character.is_whitespace() || matches!(character, '(' | ')')
                })
            {
                break;
            }
            next = characters.next();
        }
        if quote {
            return Err("Close the quoted search phrase.".into());
        }
        if bracket {
            return Err("Close the property search with ].".into());
        }
        if regex {
            return Err("Close the regular expression with /.".into());
        }
        if word == "OR" && !quoted {
            tokens.push(Token::Or);
        } else {
            tokens.push(Token::Word(word));
        }
    }
    Ok(tokens)
}
