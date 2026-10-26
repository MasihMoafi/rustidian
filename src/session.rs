use crate::{app::NotesApp, theme};
use eframe::egui;
use rustidian::{
    tab::{Tab, View},
    vault::{Document, Vault},
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
struct Session {
    tabs: Vec<(PathBuf, View)>,
    #[serde(default)]
    editing_modes: Vec<View>,
    #[serde(default)]
    pinned: Vec<bool>,
    active: usize,
    left: bool,
    right: bool,
    dark: bool,
    #[serde(default)]
    palette: theme::palette::Palette,
    #[serde(default)]
    sort_descending: bool,
    #[serde(default)]
    bookmarks: rustidian::bookmarks::Bookmarks,
    #[serde(default)]
    bookmarks_open: bool,
    #[serde(default)]
    right_tab: u8,
    #[serde(default = "default_zoom")]
    ui_zoom: f32,
    #[serde(default)]
    document_font_size: Option<u8>,
    #[serde(default)]
    editor_preferences: Option<crate::app::settings_ui::EditorPreferences>,
    #[serde(default = "default_autosave")]
    autosave_enabled: bool,
    #[serde(default)]
    note_aids: Option<rustidian::note_aids::Config>,
    #[serde(default = "default_sidebars")]
    sidebar_widths: [f32; 2],
    #[serde(default)]
    recovery_config: Option<rustidian::recovery::Config>,
    #[serde(default)]
    always_update_links: Option<bool>,
    #[serde(default)]
    confirm_trash: Option<bool>,
    #[serde(default)]
    table_layouts: crate::table_layout::Layouts,
    #[serde(default)]
    graph: Option<crate::graph_ui::State>,
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
    rustidian::paths::state_file(root, "", "json")
}

impl Session {
    fn restore_tabs(&self, vault: &Vault) -> (Vec<Tab>, usize) {
        let mut tabs = Vec::new();
        let mut active = 0;
        for (index, (path, view)) in self.tabs.iter().enumerate() {
            let document = if path.as_os_str().is_empty() {
                Document::blank()
            } else {
                let Ok(document) = vault.open_document(path) else {
                    continue;
                };
                document
            };
            if index == self.active {
                active = tabs.len();
            }
            let mut tab = Tab::new(document);
            tab.pinned = self.pinned.get(index).copied().unwrap_or(false);
            tab.set_view(self.editing_modes.get(index).copied().unwrap_or(View::Live));
            tab.set_view(*view);
            tabs.push(tab);
        }
        (tabs, active)
    }
}

// The persisted state contains paths and layout, never unsaved note contents.
impl NotesApp {
    pub(crate) fn restore_session(&mut self, ctx: &egui::Context) -> bool {
        if let Some(vault) = &self.vault {
            crate::editor_ui::clear_layout_history(ctx, &vault.root);
        }
        crate::table_layout::restore(ctx, Default::default());
        self.graph.restore(Default::default());
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
        (self.tabs, self.active) = session.restore_tabs(vault);
        self.graph.restore(session.graph.unwrap_or_default());
        self.show_left = session.left;
        crate::table_layout::restore(ctx, session.table_layouts);
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
        self.palette = session.palette;
        theme::palette::select(ctx, self.palette);
        self.ui_zoom = if session.ui_zoom.is_finite() {
            session.ui_zoom.clamp(0.75, 1.5)
        } else {
            1.0
        };
        self.autosave_enabled = session.autosave_enabled;
        self.confirm_trash = session.confirm_trash.unwrap_or(true);
        if let Some(size) = session.document_font_size {
            self.document_font_size = size.clamp(10, 30);
        }
        if let Some(preferences) = session.editor_preferences {
            self.editor_preferences = preferences;
        }
        if let Some(config) = session.note_aids {
            self.note_aids = config;
        }
        if let Some(config) = session.recovery_config
            && config.validate().is_ok()
        {
            self.recovery_config = config;
        }
        if let Some(always) = session.always_update_links {
            self.always_update_links = always;
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
            pinned: self.tabs.iter().map(|tab| tab.pinned).collect(),
            tabs: self
                .tabs
                .iter()
                .map(|t| (t.document.path.clone(), t.view))
                .collect(),
            active: self.active,
            left: self.show_left,
            right: self.show_right,
            dark: self.dark,
            palette: self.palette,
            sort_descending: self.sort_descending,
            bookmarks: self.bookmarks.clone(),
            bookmarks_open: self.show_bookmarks,
            right_tab: self.right_tab,
            ui_zoom: self.ui_zoom,
            document_font_size: Some(self.document_font_size),
            editor_preferences: Some(self.editor_preferences.clone()),
            autosave_enabled: self.autosave_enabled,
            note_aids: Some(self.note_aids.clone()),
            sidebar_widths: self.sidebar_widths,
            recovery_config: Some(self.recovery_config.clone()),
            always_update_links: Some(self.always_update_links),
            confirm_trash: Some(self.confirm_trash),
            table_layouts: crate::table_layout::snapshot(&self.ctx),
            graph: Some(self.graph.snapshot()),
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
    fn restoration_keeps_the_selected_duplicate_path_view_after_skipping_missing_files() {
        use rustidian::{tab::View, vault::Vault};
        let root = tempfile::tempdir().unwrap();
        let bytes = b"# Home\r\n";
        std::fs::write(root.path().join("Home.md"), bytes).unwrap();
        let vault = Vault::open(root.path()).unwrap();
        let mut state: Session = serde_json::from_str(
            r#"{"tabs":[["Missing.md","Read"],["Home.md","Graph"],["Home.md","Source"],["","Graph"]],"active":2,"left":true,"right":false,"dark":true,"editing_modes":["Live","Source","Live","Source"],"pinned":[false,true,false,false]}"#,
        ).unwrap();
        for (selected, expected, view) in [
            (2, 1, View::Source),
            (1, 0, View::Graph),
            (3, 2, View::Graph),
            (0, 0, View::Graph),
            (99, 0, View::Graph),
        ] {
            state.active = selected;
            let (tabs, active) = state.restore_tabs(&vault);
            assert_eq!(active, expected, "saved active {selected}");
            assert_eq!(tabs.len(), 3);
            assert_eq!(tabs[active].view, view);
            assert!(tabs[0].pinned);
            assert!(!tabs[1].pinned);
            assert_eq!(tabs[0].editing_view(), View::Source);
            assert_eq!(tabs[1].document.text.as_bytes(), bytes);
            assert!(tabs[2].document.is_blank());
        }
        assert_eq!(std::fs::read(root.path().join("Home.md")).unwrap(), bytes);
    }

    #[test]
    fn old_workspace_state_restores_without_bookmarks() {
        let old =
            r#"{"tabs":[["Home.md","Live"]],"active":0,"left":true,"right":false,"dark":true}"#;
        let state: Session = serde_json::from_str(old).unwrap();
        assert!(state.bookmarks.items.is_empty());
        assert!(!state.bookmarks_open);
        assert!(state.note_aids.is_none());
        assert!(state.graph.is_none());
        assert_eq!(state.document_font_size, None);
        assert!(state.pinned.is_empty());
        assert_eq!(state.sidebar_widths, [300.0, 234.0]);
        let restored: Session =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        assert_eq!(restored.tabs.len(), 1);
        assert_eq!(restored.tabs[0].0, std::path::Path::new("Home.md"));
    }
}
