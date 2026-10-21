use crate::{app::NotesApp, theme};
use eframe::egui;
use rust_notes::{
    tab::{Tab, View},
    vault::Document,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    #[serde(default)]
    right_tab: u8,
    #[serde(default = "default_zoom")]
    ui_zoom: f32,
    #[serde(default = "default_autosave")]
    autosave_enabled: bool,
    #[serde(default)]
    note_aids: Option<rust_notes::note_aids::Config>,
    #[serde(default = "default_sidebars")]
    sidebar_widths: [f32; 2],
    #[serde(default)]
    recovery_config: Option<rust_notes::recovery::Config>,
}

fn default_sidebars() -> [f32; 2] {
    [300.0, 234.0]
}

fn default_zoom() -> f32 {
    1.0
}
fn default_autosave() -> bool {
    true
}

fn location(root: &Path) -> Option<PathBuf> {
    rust_notes::paths::state_file(root, "", "json")
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
        self.sidebar_widths = if session.sidebar_widths.iter().all(|width| width.is_finite()) {
            [
                session.sidebar_widths[0].clamp(180.0, 450.0),
                session.sidebar_widths[1].clamp(170.0, 420.0),
            ]
        } else {
            default_sidebars()
        };
        self.restore_sidebars = [true; 2];
        self.right_tab = session.right_tab.min(2);
        self.dark = session.dark;
        self.ui_zoom = if session.ui_zoom.is_finite() {
            session.ui_zoom.clamp(0.75, 1.5)
        } else {
            1.0
        };
        self.autosave_enabled = session.autosave_enabled;
        if let Some(config) = session.note_aids {
            self.note_aids = config;
        }
        if let Some(config) = session.recovery_config
            && config.validate().is_ok()
        {
            self.recovery_config = config;
        }
        ctx.set_zoom_factor(self.ui_zoom);
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
            right_tab: self.right_tab,
            ui_zoom: self.ui_zoom,
            autosave_enabled: self.autosave_enabled,
            note_aids: Some(self.note_aids.clone()),
            sidebar_widths: self.sidebar_widths,
            recovery_config: Some(self.recovery_config.clone()),
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
        assert!(state.note_aids.is_none());
        assert_eq!(state.sidebar_widths, [300.0, 234.0]);
        let restored: Session =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored.tabs.len(), 1);
        assert_eq!(restored.tabs[0].0, std::path::Path::new("Home.md"));
    }
}
