use percent_encoding::{AsciiSet, CONTROLS, percent_decode_str, utf8_percent_encode};
use std::path::{Component, Path, PathBuf};

/// Read one small, ordinary settings file; never follow a linked profile.
pub fn read_obsidian<T: serde::de::DeserializeOwned + Default>(
    root: &Path,
    name: &str,
) -> Result<T, String> {
    let folder = root.join(".obsidian");
    match std::fs::symlink_metadata(&folder) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Ok(meta) if meta.is_dir() && !meta.file_type().is_symlink() => {}
        _ => {
            return Err(
                "Cannot import note settings from a linked or unreadable .obsidian folder.".into(),
            );
        }
    }
    let path = folder.join(name);
    let meta = match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(error) => return Err(format!("Cannot read note settings: {error}")),
        Ok(meta) => meta,
    };
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 65536 {
        return Err("Note settings must be small ordinary JSON files.".into());
    }
    let bytes =
        std::fs::read(path).map_err(|error| format!("Cannot read note settings: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("Cannot import note settings: {error}"))
}

pub fn state_file(root: &Path, group: &str, extension: &str) -> Option<PathBuf> {
    use std::{
        collections::hash_map::DefaultHasher,
        hash::{Hash, Hasher},
    };
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    let mut hash = DefaultHasher::new();
    root.hash(&mut hash);
    Some(
        app_directory(&base)
            .join(group)
            .join(format!("{:016x}.{extension}", hash.finish())),
    )
}

/// Keep settings and recovery when upgrading, including windows still running.
pub fn app_directory(base: &Path) -> PathBuf {
    let current = base.join("rustidian");
    let previous = base.join("rust-notes");
    if current.exists() || !previous.is_dir() {
        return current;
    }
    if let Err(error) = std::fs::rename(&previous, &current) {
        eprintln!(
            "Cannot move existing Rustidian settings: {error}. Using their previous location."
        );
        return previous;
    }
    #[cfg(unix)]
    if let Err(error) = std::os::unix::fs::symlink("rustidian", &previous) {
        eprintln!("Cannot keep the settings link for an older running window: {error}");
    }
    current
}

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
