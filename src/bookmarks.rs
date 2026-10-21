use crate::vault::Vault;
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmark {
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl Bookmark {
    pub fn label(&self) -> String {
        self.title
            .clone()
            .unwrap_or_else(|| crate::vault::file_title(&self.path))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bookmarks {
    pub items: Vec<Bookmark>,
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|part| {
            matches!(part, Component::Normal(name) if !name.to_string_lossy().starts_with('.'))
        })
}

fn custom_title(title: &str) -> Option<String> {
    let title = title.trim();
    (!title.is_empty()).then(|| title.to_owned())
}

impl Bookmarks {
    pub fn contains(&self, path: &Path) -> bool {
        self.items.iter().any(|item| item.path == path)
    }

    pub fn add(&mut self, vault: &Vault, path: &Path) -> Result<bool, String> {
        if !safe_relative(path) {
            return Err("Bookmark a note or image inside this vault.".into());
        }
        // Recheck the filesystem through the same guarded path used when opening.
        vault.open_document(path)?;
        if self.contains(path) {
            return Ok(false);
        }
        self.items.push(Bookmark {
            path: path.to_owned(),
            title: None,
        });
        Ok(true)
    }

    pub fn remove(&mut self, path: &Path) {
        self.items.retain(|item| item.path != path);
    }

    pub fn set_title(&mut self, path: &Path, title: &str) {
        if let Some(item) = self.items.iter_mut().find(|item| item.path == path) {
            item.title = custom_title(title);
        }
    }

    pub fn move_item(&mut self, path: &Path, offset: isize) {
        if let Some(index) = self.items.iter().position(|item| item.path == path)
            && let Some(target) = index.checked_add_signed(offset)
            && target < self.items.len()
        {
            self.items.swap(index, target);
        }
    }

    pub fn rename(&mut self, from: &Path, to: &Path) {
        for item in &mut self.items {
            item.path = crate::paths::moved_path(&item.path, from, to, true);
        }
        self.sanitize();
    }

    /// Keep missing files for recovery, but never restore outside or duplicate paths.
    pub fn sanitize(&mut self) {
        let mut seen = std::collections::HashSet::new();
        self.items
            .retain(|item| safe_relative(&item.path) && seen.insert(item.path.clone()));
        for item in &mut self.items {
            if let Some(title) = item.title.take() {
                item.title = custom_title(&title);
            }
        }
    }
}
