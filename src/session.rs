use crate::{app::NotesApp, theme};
use eframe::egui;
use rust_notes::{
    tab::{Tab, View},
    vault::Document,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct Session {
    tabs: Vec<(PathBuf, View)>,
    #[serde(default)]
    editing_modes: Vec<View>,
    active: usize,
    left: bool,
    right: bool,
    dark: bool,
    #[serde(default)]
    sort_descending: bool,
    #[serde(default)]
    bookmarks: rust_notes::bookmarks::Bookmarks,
    #[serde(default)]
    bookmarks_open: bool,
}

fn location(root: &Path) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state"))
        })?;
    let mut hash = DefaultHasher::new();
    root.hash(&mut hash);
    Some(
        base.join("rust-notes")
            .join(format!("{:016x}.json", hash.finish())),
    )
}

// The persisted state contains paths and layout, never unsaved note contents.
impl NotesApp {
    pub(crate) fn restore_session(&mut self, ctx: &egui::Context) -> bool {
        let Some(vault) = &self.vault else {
            return false;
        };
        let Some(path) = location(&vault.root) else {
            return false;
        };
        let Ok(bytes) = std::fs::read(path) else {
            return false;
        };
        let Ok(session) = serde_json::from_slice::<Session>(&bytes) else {
            return false;
        };
        let selected = session
            .tabs
            .get(session.active)
            .map(|(path, _)| path.clone());
        self.tabs = session
            .tabs
            .into_iter()
            .enumerate()
            .filter_map(|(index, (path, view))| {
                let document = if path.as_os_str().is_empty() {
                    Document::blank()
                } else {
                    vault.open_document(&path).ok()?
                };
                let mut tab = Tab::new(document);
                tab.set_view(
                    session
                        .editing_modes
                        .get(index)
                        .copied()
                        .unwrap_or(View::Live),
                );
                tab.set_view(view);
                Some(tab)
            })
            .collect();
        self.active = selected
            .and_then(|path| self.tabs.iter().position(|t| t.document.path == path))
            .unwrap_or(0);
        self.show_left = session.left;
        self.show_right = session.right;
        self.dark = session.dark;
        self.sort_descending = session.sort_descending;
        self.bookmarks = session.bookmarks;
        self.bookmarks.sanitize();
        self.show_bookmarks = session.bookmarks_open;
        theme::apply(ctx, self.dark);
        !self.tabs.is_empty()
    }

    pub(crate) fn persist_session(&self) {
        if self.vault.is_none() {
            return;
        }
        if let Err(error) = self.write_session() {
            eprintln!("Cannot preserve workspace layout: {error}");
        }
    }

    pub(crate) fn write_session(&self) -> Result<(), String> {
        let Some(vault) = &self.vault else {
            return Err("Open a vault first.".into());
        };
        let Some(path) = location(&vault.root) else {
            return Err("Cannot find a folder for workspace state.".into());
        };
        let session = Session {
            editing_modes: self.tabs.iter().map(Tab::editing_view).collect(),
            tabs: self
                .tabs
                .iter()
                .map(|t| (t.document.path.clone(), t.view))
                .collect(),
            active: self.active,
            left: self.show_left,
            right: self.show_right,
            dark: self.dark,
            sort_descending: self.sort_descending,
            bookmarks: self.bookmarks.clone(),
            bookmarks_open: self.show_bookmarks,
        };
        let bytes = serde_json::to_vec(&session).map_err(|error| error.to_string())?;
        let Some(parent) = path.parent().map(Path::to_owned) else {
            return Err("Cannot find a folder for workspace state.".into());
        };
        let result = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&parent)?;
            let mut file = tempfile::NamedTempFile::new_in(&parent)?;
            std::io::Write::write_all(&mut file, &bytes)?;
            file.persist(path)?;
            Ok(())
        })();
        result.map_err(|error| format!("Cannot preserve workspace state: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::Session;

    #[test]
    fn old_workspace_state_restores_without_bookmarks() {
        let old =
            r#"{"tabs":[["Home.md","Live"]],"active":0,"left":true,"right":false,"dark":true}"#;
        let state: Session = serde_json::from_str(old).unwrap();
        assert!(state.bookmarks.items.is_empty());
        assert!(!state.bookmarks_open);
        let restored: Session =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored.tabs.len(), 1);
        assert_eq!(restored.tabs[0].0, std::path::Path::new("Home.md"));
    }
}
