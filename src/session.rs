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
        theme::apply(ctx, self.dark);
        !self.tabs.is_empty()
    }

    pub(crate) fn persist_session(&self) {
        let Some(vault) = &self.vault else {
            return;
        };
        let Some(path) = location(&vault.root) else {
            return;
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
        };
        let Ok(bytes) = serde_json::to_vec(&session) else {
            return;
        };
        let Some(parent) = path.parent().map(Path::to_owned) else {
            return;
        };
        let result = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&parent)?;
            let mut file = tempfile::NamedTempFile::new_in(&parent)?;
            std::io::Write::write_all(&mut file, &bytes)?;
            file.persist(path)?;
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("Cannot preserve workspace layout: {error}");
        }
    }
}
