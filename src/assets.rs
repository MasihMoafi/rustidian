//! Resolve indexed image attachments without granting access outside a vault.
use crate::{paths, vault::Vault};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct EmbeddedImage {
    pub range: std::ops::Range<usize>,
    pub target: String,
    pub alt: String,
    pub width: Option<f32>,
    pub height: Option<f32>,
}

fn dimensions(label: &str) -> (Option<f32>, Option<f32>) {
    let hint = label.rsplit('|').next().unwrap_or(label);
    let (width, height) = hint
        .split_once('x')
        .map_or((hint, None), |(width, height)| (width, Some(height)));
    let parse = |value: &str| {
        value
            .parse::<f32>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
    };
    (parse(width), height.and_then(parse))
}

pub fn embedded_images(source: &str) -> Vec<EmbeddedImage> {
    use pulldown_cmark::{Event, Parser, Tag, TagEnd};
    let body = crate::metadata::frontmatter(source)
        .ok()
        .flatten()
        .map_or(0, |properties| properties.range.end);
    let mut images = Vec::new();
    for (range, target) in crate::vault::wiki_spans(source) {
        if range.start <= body || source.as_bytes()[range.start - 1] != b'!' {
            continue;
        }
        let start = range.start - 1;
        if source[..start]
            .bytes()
            .rev()
            .take_while(|byte| *byte == b'\\')
            .count()
            % 2
            == 1
        {
            continue;
        }
        let (target, alt) = target
            .split_once('|')
            .map_or((target.as_str(), target.as_str()), |(target, alt)| {
                (target, alt)
            });
        let (width, height) = dimensions(alt);
        images.push(EmbeddedImage {
            range: start..range.end,
            target: target.into(),
            alt: alt.into(),
            width,
            height,
        });
    }
    let mut image = None;
    for (event, range) in Parser::new(&source[body..]).into_offset_iter() {
        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                image = Some((
                    body + range.start..body + range.end,
                    dest_url.into_string(),
                    String::new(),
                ))
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, _, alt)) = &mut image {
                    alt.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, _, alt)) = &mut image {
                    alt.push(' ');
                }
            }
            Event::End(TagEnd::Image) => {
                if let Some((range, target, alt)) = image.take() {
                    let (width, height) = dimensions(&alt);
                    images.push(EmbeddedImage {
                        range,
                        target,
                        alt,
                        width,
                        height,
                    });
                }
            }
            _ => {}
        }
    }
    images.sort_by_key(|image| (image.range.start, std::cmp::Reverse(image.range.end)));
    let mut previous_end = 0;
    images.retain(|image| {
        if image.range.start < previous_end {
            return false;
        }
        previous_end = image.range.end;
        true
    });
    images
}

/// Resolve display image destinations without altering a note's source.
pub fn prepare_images(markdown: &str, resolve: impl Fn(&str) -> Option<String>) -> String {
    let mut display = markdown.to_owned();
    for image in embedded_images(markdown).into_iter().rev() {
        let destination = resolve(&image.target).unwrap_or_else(|| {
            if image.target.starts_with("https://") || image.target.starts_with("http://") {
                image.target
            } else {
                format!("rust-missing-image:{}", image.target)
            }
        });
        let alt = image
            .alt
            .replace('\\', "\\\\")
            .replace('[', "\\[")
            .replace(']', "\\]");
        display.replace_range(image.range, &format!("![{alt}](<{destination}>)"));
    }
    display
}

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp"
            )
        })
}

pub fn image_uri(vault: Option<&Vault>, from: &Path, target: &str) -> Option<String> {
    if target.starts_with("https://") || target.starts_with("http://") {
        return Some(target.into());
    }
    vault?
        .resolve_attachment(from, target)
        .map(|path| format!("file://{}", path.display()))
}

impl Vault {
    pub fn resolve_destination(&self, from: &Path, target: &str) -> Option<PathBuf> {
        self.resolve_attachment(from, target)
            .and_then(|path| path.strip_prefix(&self.root).ok().map(Path::to_owned))
            .or_else(|| self.resolve(from, target))
    }

    pub fn resolve_attachment(&self, from: &Path, target: &str) -> Option<PathBuf> {
        let target = target.split('|').next()?.split('#').next()?.trim();
        let decoded = paths::decode(target)?;
        let target = Path::new(&decoded);
        if target.is_absolute() || !is_image(target) {
            return None;
        }
        let sibling = paths::normalise(&from.parent().unwrap_or(Path::new("")).join(target));
        let root = paths::normalise(target);
        let indexed = sibling
            .as_ref()
            .filter(|path| self.attachments.contains(path))
            .or_else(|| root.as_ref().filter(|path| self.attachments.contains(path)));
        let relative = if let Some(path) = indexed {
            path
        } else {
            if target.components().count() != 1 {
                return None;
            }
            let mut matches = self
                .attachments
                .iter()
                .filter(|path| path.file_name() == target.file_name());
            let first = matches.next()?;
            if matches.next().is_some() {
                return None;
            }
            first
        };
        let absolute = self.root.join(relative).canonicalize().ok()?;
        (absolute.starts_with(&self.root) && absolute.is_file()).then_some(absolute)
    }
}
