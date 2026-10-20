use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use std::path::{Component, Path, PathBuf};

pub fn wiki_destination(path: &Path) -> String {
    if crate::vault::is_markdown(path) {
        let stem = path.with_extension("");
        if crate::assets::is_image(&stem) {
            path.display().to_string()
        } else {
            stem.display().to_string()
        }
    } else {
        path.display()
            .to_string()
            .replace('%', "%25")
            .replace('#', "%23")
            .replace('|', "%7C")
            .replace('\\', "%5C")
            .replace('\n', "%0A")
            .replace('\r', "%0D")
            .replace("]]", "%5D%5D")
            .replace("mailto:", "mailto%3A")
    }
}

pub fn display_path(path: &Path) -> String {
    if crate::vault::is_markdown(path) {
        path.with_extension("").display().to_string()
    } else {
        path.display().to_string()
    }
}

pub fn wiki_path_supported(path: &Path) -> bool {
    let Some(raw) = path.to_str() else {
        return false;
    };
    if !crate::vault::is_markdown(path) {
        return true;
    }
    let name = &raw[..raw.len() - 3];
    !name.is_empty()
        && name.trim() == name
        && !name.contains("]]")
        && !name
            .chars()
            .any(|character| matches!(character, '#' | '|' | '\\' | '\n' | '\r'))
}

pub fn moved_path(path: &Path, from: &Path, to: &Path, folder: bool) -> PathBuf {
    if path == from {
        to.to_owned()
    } else if folder && let Ok(suffix) = path.strip_prefix(from) {
        to.join(suffix)
    } else {
        path.to_owned()
    }
}

pub fn normalise(path: &Path) -> Option<PathBuf> {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => result.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !result.pop() {
                    return None;
                }
            }
            _ => return None,
        }
    }
    Some(result)
}

pub fn markdown_name(path: &Path) -> PathBuf {
    if crate::vault::is_markdown(path) {
        return path.to_owned();
    }
    let mut name = path.as_os_str().to_os_string();
    name.push(".md");
    PathBuf::from(name)
}

pub fn decode(value: &str) -> Option<String> {
    percent_decode_str(value)
        .decode_utf8()
        .ok()
        .map(|value| value.into_owned())
}

pub fn markdown_destination(from: &Path, to: &Path) -> String {
    let parent: Vec<_> = from
        .parent()
        .unwrap_or(Path::new(""))
        .components()
        .collect();
    let destination: Vec<_> = to.components().collect();
    let common = parent
        .iter()
        .zip(&destination)
        .take_while(|(a, b)| a == b)
        .count();
    let mut relative = PathBuf::new();
    for _ in common..parent.len() {
        relative.push("..");
    }
    for component in &destination[common..] {
        relative.push(component.as_os_str());
    }
    const RESERVED: &AsciiSet = &CONTROLS
        .add(b' ')
        .add(b'(')
        .add(b')')
        .add(b'#')
        .add(b'%')
        .add(b'?')
        .add(b'[')
        .add(b']')
        .add(b'\\');
    utf8_percent_encode(&relative.to_string_lossy().replace('\\', "/"), RESERVED).to_string()
}
