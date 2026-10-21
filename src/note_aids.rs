//! Daily notes and source-preserving template variables; no work on idle frames.
use crate::vault::{Document, Result, Vault};
use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, Timelike};
use serde::{Deserialize, Serialize};
use std::{
    path::{Component, Path, PathBuf},
    sync::OnceLock,
};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub templates: Templates,
    pub daily: Daily,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Templates {
    pub folder: String,
    pub date_format: String,
    pub time_format: String,
}

impl Default for Templates {
    fn default() -> Self {
        Self {
            folder: String::new(),
            date_format: "YYYY-MM-DD".into(),
            time_format: "HH:mm".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Daily {
    pub folder: String,
    pub format: String,
    pub template: String,
}

impl Default for Daily {
    fn default() -> Self {
        Self {
            folder: String::new(),
            format: "YYYY-MM-DD".into(),
            template: String::new(),
        }
    }
}

fn relative(value: &str, empty: bool) -> Result<PathBuf> {
    let path = Path::new(value.trim());
    if (!empty && path.as_os_str().is_empty())
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || crate::vault::is_ignored_path(path)
    {
        return Err("Choose a visible relative path inside this vault.".into());
    }
    Ok(path.to_owned())
}

impl Config {
    /// Import only these two small settings files, without writing .obsidian.
    pub fn from_obsidian(root: &Path) -> Result<Self> {
        let folder = root.join(".obsidian");
        match std::fs::symlink_metadata(&folder) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
            _ => {
                return Err(
                    "Cannot import note settings from a linked or unreadable .obsidian folder."
                        .into(),
                );
            }
        }
        fn read<T: serde::de::DeserializeOwned + Default>(path: &Path) -> Result<T> {
            let meta = match std::fs::symlink_metadata(path) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return Ok(T::default());
                }
                Err(error) => return Err(format!("Cannot read note settings: {error}")),
                Ok(meta) => meta,
            };
            if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 65536 {
                return Err("Note settings must be small ordinary JSON files.".into());
            }
            let bytes = std::fs::read(path)
                .map_err(|error| format!("Cannot read note settings: {error}"))?;
            serde_json::from_slice(&bytes)
                .map_err(|error| format!("Cannot import note settings: {error}"))
        }
        Ok(Self {
            templates: read(&folder.join("templates.json"))?,
            daily: read(&folder.join("daily-notes.json"))?,
        })
    }

    pub fn template_paths(&self, vault: &Vault) -> Result<Vec<PathBuf>> {
        if self.templates.folder.trim().is_empty() {
            return Err("Set the template folder in Settings → Templates first.".into());
        }
        let folder = relative(&self.templates.folder, false)?;
        if !vault.folders.contains(&folder) {
            return Err("The template folder does not exist in this vault.".into());
        }
        Ok(vault
            .notes
            .iter()
            .filter(|note| note.path.starts_with(&folder) && crate::vault::is_markdown(&note.path))
            .map(|note| note.path.clone())
            .collect())
    }

    pub fn render_template(
        &self,
        vault: &Vault,
        path: &Path,
        title: &str,
        now: DateTime<FixedOffset>,
    ) -> Result<String> {
        let document = vault.open_document(path)?;
        Ok(self.expand(&document.text, title, now))
    }

    pub fn expand(&self, source: &str, title: &str, now: DateTime<FixedOffset>) -> String {
        static VARIABLES: OnceLock<regex::Regex> = OnceLock::new();
        VARIABLES
            .get_or_init(|| regex::Regex::new(r"\{\{(title|date|time)(?::([^}]*))?\}\}").unwrap())
            .replace_all(source, |captures: &regex::Captures<'_>| {
                let kind = &captures[1];
                if kind == "title" {
                    return if captures.get(2).is_none() {
                        title.to_owned()
                    } else {
                        captures[0].to_owned()
                    };
                }
                let default = if kind == "date" {
                    &self.templates.date_format
                } else {
                    &self.templates.time_format
                };
                let fallback = if kind == "date" {
                    "YYYY-MM-DD"
                } else {
                    "HH:mm"
                };
                moment_format(
                    now,
                    captures.get(2).map_or(default.as_str(), |m| m.as_str()),
                    fallback,
                )
            })
            .into_owned()
    }

    pub fn create_daily(&self, vault: &mut Vault, now: DateTime<FixedOffset>) -> Result<PathBuf> {
        let name = moment_format(now, self.daily.format.trim(), "YYYY-MM-DD");
        let path = relative(&self.daily.folder, true)?.join(relative(&name, false)?);
        let path = crate::paths::markdown_name(&path);
        let absolute = vault.safe_path(&path, false)?;
        if absolute.exists() {
            vault.open_document(&path)?;
            return Ok(path);
        }
        let mut document = Document::new(path.clone());
        if !self.daily.template.trim().is_empty() {
            let template = crate::paths::markdown_name(&relative(&self.daily.template, false)?);
            document.text = self.render_template(vault, &template, &document.title(), now)?;
        }
        vault.save(&mut document)?;
        Ok(path)
    }
}

/// Moment-style English tokens, including literal brackets and escaped letters.
/// This is used only for explicit commands, never for rendering or indexing.
pub fn moment_format(now: DateTime<FixedOffset>, pattern: &str, fallback: &str) -> String {
    const TOKENS: &[(&str, &str)] = &[
        ("YYYYYY", "extended-year"),
        ("YYYY", "%Y"),
        ("YY", "%y"),
        ("Y", "%Y"),
        ("MMMM", "%B"),
        ("MMM", "%b"),
        ("Mo", "ordinal-month"),
        ("MM", "%m"),
        ("M", "%-m"),
        ("DDDD", "%j"),
        ("DDDo", "ordinal-year"),
        ("DDD", "day-year"),
        ("Do", "ordinal"),
        ("DD", "%d"),
        ("D", "%-d"),
        ("dddd", "%A"),
        ("ddd", "%a"),
        ("dd", "weekday-short"),
        ("do", "ordinal-weekday"),
        ("d", "%w"),
        ("e", "%w"),
        ("HH", "%H"),
        ("H", "%-H"),
        ("hh", "%I"),
        ("h", "%-I"),
        ("kk", "hour-one-padded"),
        ("k", "hour-one"),
        ("mm", "%M"),
        ("m", "%-M"),
        ("ss", "%S"),
        ("s", "%-S"),
        ("ZZ", "%z"),
        ("Z", "%:z"),
        ("zz", "zone-long"),
        ("z", "zone"),
        ("A", "%p"),
        ("a", "%P"),
        ("GGGG", "%G"),
        ("GG", "%g"),
        ("WW", "%V"),
        ("Wo", "ordinal-iso-week"),
        ("W", "iso-week"),
        ("E", "%u"),
        ("gggg", "week-year"),
        ("gg", "short-week-year"),
        ("ww", "week-padded"),
        ("wo", "ordinal-week"),
        ("w", "week"),
        ("Qo", "ordinal-quarter"),
        ("Q", "quarter"),
        ("X", "unix"),
        ("x", "unix-millis"),
        ("LTS", "long-time"),
        ("LT", "short-time"),
        ("LLLL", "long-weekday"),
        ("LLL", "long-datetime"),
        ("LL", "long-date"),
        ("L", "numeric-date"),
        ("llll", "short-weekday"),
        ("lll", "short-datetime"),
        ("ll", "short-date"),
        ("l", "short-numeric-date"),
    ];
    fn ordinal(value: u32) -> String {
        let ending = if (11..=13).contains(&(value % 100)) {
            "th"
        } else {
            match value % 10 {
                1 => "st",
                2 => "nd",
                3 => "rd",
                _ => "th",
            }
        };
        format!("{value}{ending}")
    }
    let mut remaining = if pattern.is_empty() {
        fallback
    } else {
        pattern
    };
    let mut result = String::new();
    let date = now.date_naive();
    let week_start = |year| {
        NaiveDate::from_ymd_opt(year, 1, 1)
            .map(|first| first - Duration::days(first.weekday().num_days_from_sunday().into()))
    };
    let week_year = if week_start(now.year() + 1).is_some_and(|first| date >= first) {
        now.year() + 1
    } else {
        now.year()
    };
    let week = week_start(week_year).map_or(1, |first| (date - first).num_days() as u32 / 7 + 1);
    while !remaining.is_empty() {
        if remaining.starts_with('[')
            && let Some(end) = remaining.find(']')
        {
            result.push_str(&remaining[1..end]);
            remaining = &remaining[end + 1..];
            continue;
        }
        if remaining.starts_with('S') {
            let count = remaining
                .bytes()
                .take_while(|byte| *byte == b'S')
                .count()
                .min(9);
            let fraction = format!("{:03}000000", now.timestamp_subsec_millis());
            result.push_str(&fraction[..count]);
            remaining = &remaining[count..];
            continue;
        }
        if let Some(escaped) = remaining.strip_prefix('\\')
            && let Some(character) = escaped.chars().next()
        {
            result.push(character);
            remaining = &escaped[character.len_utf8()..];
            continue;
        }
        if let Some((token, format)) = TOKENS
            .iter()
            .find(|(token, _)| remaining.starts_with(token))
        {
            let value = match *format {
                "extended-year" => format!("{:+07}", now.year()),
                "ordinal-month" => ordinal(now.month()),
                "ordinal" => ordinal(now.day()),
                "ordinal-weekday" => ordinal(now.weekday().num_days_from_sunday()),
                "ordinal-year" => ordinal(now.ordinal()),
                "day-year" => now.ordinal().to_string(),
                "weekday-short" => now.format("%a").to_string()[..2].to_owned(),
                "hour-one" => if now.hour() == 0 { 24 } else { now.hour() }.to_string(),
                "hour-one-padded" => {
                    format!("{:02}", if now.hour() == 0 { 24 } else { now.hour() })
                }
                "iso-week" => now.iso_week().week().to_string(),
                "ordinal-iso-week" => ordinal(now.iso_week().week()),
                "week-year" => week_year.to_string(),
                "short-week-year" => format!("{:02}", week_year.rem_euclid(100)),
                "week" => week.to_string(),
                "week-padded" => format!("{week:02}"),
                "ordinal-week" => ordinal(week),
                "ordinal-quarter" => ordinal(now.month0() / 3 + 1),
                "zone" | "zone-long" => String::new(),
                "quarter" => (now.month0() / 3 + 1).to_string(),
                "unix" => now.timestamp().to_string(),
                "unix-millis" => now.timestamp_millis().to_string(),
                "long-time" => moment_format(now, "h:mm:ss A", ""),
                "short-time" => moment_format(now, "h:mm A", ""),
                "long-weekday" => moment_format(now, "dddd, MMMM D, YYYY h:mm A", ""),
                "long-datetime" => moment_format(now, "MMMM D, YYYY h:mm A", ""),
                "long-date" => moment_format(now, "MMMM D, YYYY", ""),
                "numeric-date" => moment_format(now, "MM/DD/YYYY", ""),
                "short-weekday" => moment_format(now, "ddd, MMM D, YYYY h:mm A", ""),
                "short-datetime" => moment_format(now, "MMM D, YYYY h:mm A", ""),
                "short-date" => moment_format(now, "MMM D, YYYY", ""),
                "short-numeric-date" => moment_format(now, "M/D/YYYY", ""),
                format => now.format(format).to_string(),
            };
            result.push_str(&value);
            remaining = &remaining[token.len()..];
        } else {
            let character = remaining.chars().next().unwrap();
            result.push(character);
            remaining = &remaining[character.len_utf8()..];
        }
    }
    result
}
