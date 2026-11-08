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
    /// An HTML `<img>` tag rather than Markdown.
    pub html: bool,
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
    let maths = crate::math::spans(source);
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
            html: false,
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
                        html: false,
                    });
                }
            }
            _ => {}
        }
    }
    for tag in crate::html_layout::scan(source).images {
        let Some(target) = tag.attribute("src").filter(|src| !src.trim().is_empty()) else {
            continue;
        };
        images.push(EmbeddedImage {
            target: target.trim().into(),
            alt: tag.attribute("alt").unwrap_or_default().into(),
            width: tag.pixels("width"),
            height: tag.pixels("height"),
            range: tag.range,
            html: true,
        });
    }
    images.retain(|image| {
        !maths
            .iter()
            .any(|math| math.range.start < image.range.end && math.range.end > image.range.start)
    });
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
        if image.html {
            let attribute = |text: &str| text.replace('&', "&amp;").replace('"', "&quot;");
            let mut tag = format!(
                "<img src=\"{}\" alt=\"{}\"",
                attribute(&destination),
                attribute(&image.alt)
            );
            let original = crate::html_layout::tokens(&markdown[image.range.clone()])
                .into_iter()
                .find_map(|token| match token {
                    crate::html_layout::Token::Tag(tag) => Some(tag),
                    _ => None,
                });
            for name in ["width", "height"] {
                if let Some(value) = original.as_ref().and_then(|tag| tag.attribute(name)) {
                    tag.push_str(&format!(" {name}=\"{}\"", attribute(value)));
                }
            }
            tag.push('>');
            display.replace_range(image.range, &tag);
            continue;
        }
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

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp4" | "webm" | "mov"
            )
        })
}

pub fn is_media(path: &Path) -> bool {
    is_image(path) || is_video(path)
}

pub fn video_uri(vault: Option<&Vault>, from: &Path, target: &str) -> Option<String> {
    if target.starts_with("https://") || target.starts_with("http://") {
        return Some(target.into());
    }
    let path = local_media(vault?, from, target)?;
    is_video(&path).then(|| {
        format!(
            "file://{}",
            paths::markdown_destination(Path::new(""), &path)
        )
    })
}

pub fn image_uri(vault: Option<&Vault>, from: &Path, target: &str) -> Option<String> {
    if target.starts_with("https://") || target.starts_with("http://") {
        return Some(target.into());
    }
    if target.starts_with("data:") {
        return data_image_uri(target);
    }
    if target.starts_with(DATA_IMAGE_SCHEME) {
        return data_image(target).map(|_| target.into());
    }
    local_media(vault?, from, target)
        .filter(|path| is_image(path))
        .map(|path| {
            format!(
                "file://{}",
                paths::markdown_destination(Path::new(""), &path)
            )
        })
}

fn local_media(vault: &Vault, from: &Path, target: &str) -> Option<PathBuf> {
    if let Some(path) = target.strip_prefix("file://") {
        let path = PathBuf::from(paths::decode(path)?).canonicalize().ok()?;
        let guest = if from.is_absolute() && !from.starts_with(&vault.root) {
            from.parent().and_then(|parent| parent.canonicalize().ok())
        } else {
            None
        };
        return (path.is_file()
            && is_media(&path)
            && (path
                .strip_prefix(&vault.root)
                .ok()
                .is_some_and(|relative| vault.attachments.contains(&relative.to_owned()))
                || guest.is_some_and(|parent| path.starts_with(parent))))
        .then_some(path);
    }
    vault.resolve_attachment(from, target)
}

/// URI scheme under which decoded inline `data:` images are served.
pub const DATA_IMAGE_SCHEME: &str = "data-image://";

type DataImages = std::collections::HashMap<String, (std::sync::Arc<[u8]>, &'static str)>;

fn data_images() -> &'static std::sync::Mutex<DataImages> {
    static IMAGES: std::sync::OnceLock<std::sync::Mutex<DataImages>> = std::sync::OnceLock::new();
    IMAGES.get_or_init(Default::default)
}

/// Decode a `data:image/...` URI (base64 or percent-encoded) once and return a
/// `data-image://<hash>.<ext>` URI; image loaders pick a decoder by extension.
pub fn data_image_uri(target: &str) -> Option<String> {
    use std::hash::{Hash, Hasher};
    let (header, payload) = target.strip_prefix("data:")?.split_once(',')?;
    let mut fields = header.split(';');
    let mime = fields.next()?.trim().to_ascii_lowercase();
    let (extension, mime) = match mime.as_str() {
        "image/svg+xml" => ("svg", "image/svg+xml"),
        "image/png" => ("png", "image/png"),
        "image/jpeg" | "image/jpg" => ("jpg", "image/jpeg"),
        "image/gif" => ("gif", "image/gif"),
        "image/webp" => ("webp", "image/webp"),
        "image/bmp" => ("bmp", "image/bmp"),
        _ => return None,
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    target.hash(&mut hasher);
    let uri = format!("{DATA_IMAGE_SCHEME}{:016x}.{extension}", hasher.finish());
    let mut images = data_images()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if !images.contains_key(&uri) {
        let bytes: Vec<u8> = if fields.any(|field| field.trim().eq_ignore_ascii_case("base64")) {
            use base64::Engine;
            let compact: String = percent_encoding::percent_decode_str(payload)
                .decode_utf8()
                .ok()?
                .chars()
                .filter(|character| !character.is_whitespace())
                .collect();
            base64::engine::general_purpose::STANDARD_NO_PAD
                .decode(compact.trim_end_matches('='))
                .ok()?
        } else {
            percent_encoding::percent_decode_str(payload).collect()
        };
        if bytes.is_empty() {
            return None;
        }
        images.insert(uri.clone(), (bytes.into(), mime));
    }
    Some(uri)
}

/// The decoded bytes and mime type behind a `data-image://` URI.
pub fn data_image(uri: &str) -> Option<(std::sync::Arc<[u8]>, &'static str)> {
    data_images()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(uri)
        .cloned()
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
        if target.is_absolute() || !is_media(target) {
            return None;
        }
        if from.is_absolute() && !from.starts_with(&self.root) {
            let parent = from.parent()?.canonicalize().ok()?;
            let root = parent
                .ancestors()
                .find(|path| path.join(".obsidian").is_dir())
                .unwrap_or(&parent);
            let absolute = parent.join(target).canonicalize().ok();
            if let Some(absolute) = absolute.filter(|path| {
                path.starts_with(root)
                    && path.is_file()
                    && path
                        .strip_prefix(root)
                        .is_ok_and(|relative| !crate::vault::is_ignored_path(relative))
            }) {
                return Some(absolute);
            }
        }
        let from = from.strip_prefix(&self.root).unwrap_or(from);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_data_images_resolve_to_decoded_bytes() {
        let svg = "data:image/svg+xml,%3Csvg%20xmlns%3D%22http%3A%2F%2Fwww.w3.org%2F2000%2Fsvg%22%20viewBox%3D%220%200%201%201%22%3E%3Crect%20width%3D%221%22%20height%3D%221%22%20fill%3D%22rgb(243%2C243%2C243)%22%2F%3E%3C%2Fsvg%3E";
        let uri = image_uri(None, Path::new("note.md"), svg).unwrap();
        assert!(uri.starts_with(DATA_IMAGE_SCHEME) && uri.ends_with(".svg"));
        let (bytes, mime) = data_image(&uri).unwrap();
        assert_eq!(mime, "image/svg+xml");
        assert!(
            std::str::from_utf8(&bytes)
                .unwrap()
                .starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\"")
        );

        let png = image_uri(
            None,
            Path::new("note.md"),
            "data:image/png;base64,iVBORw0KGgo=",
        )
        .unwrap();
        assert_eq!(&*data_image(&png).unwrap().0, b"\x89PNG\r\n\x1a\n");

        let display = prepare_images(&format!("![]({svg})"), |target| {
            image_uri(None, Path::new("note.md"), target)
        });
        assert!(!display.contains("rust-missing-image:"), "{display}");
        assert!(image_uri(None, Path::new("note.md"), "data:text/html,<b>x</b>").is_none());
    }
}
