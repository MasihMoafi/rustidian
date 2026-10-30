use crate::{navigation::History, vault::Document};
use serde::{Deserialize, Serialize};
use std::{
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

static NEXT_TAB_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum View {
    Live,
    Source,
    Read,
    Split,
    Graph,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageScale {
    Fit,
    Actual,
}

pub struct Tab {
    id: u64,
    pub document: Document,
    pub history: History,
    pub view: View,
    pub pinned: bool,
    editing_view: View,
    // A plain-text document borrows the tab in Source mode; the note's modes return after it.
    note_views: Option<(View, View)>,
    pub image_scale: ImageScale,
    pub edited_at: Option<Instant>,
    pub autosave_failed: bool,
    // Editing state follows a moved file, but never leaks into another document.
    editor_path: std::path::PathBuf,
}

/// A closed view retains its identity and navigation, without retaining file text.
pub struct ClosedTab {
    id: u64,
    path: PathBuf,
    editor_path: PathBuf,
    history: History,
    view: View,
    editing_view: View,
    note_views: Option<(View, View)>,
    pinned: bool,
    image_scale: ImageScale,
    saved_fingerprint: Option<u64>,
}

fn saved_fingerprint(document: &Document) -> Option<u64> {
    document.saved_text().map(|text| {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut hash);
        hash.finish()
    })
}

impl ClosedTab {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn matches_saved_text(&self, document: &Document) -> bool {
        self.saved_fingerprint.is_some() && self.saved_fingerprint == saved_fingerprint(document)
    }

    pub fn rename_path(&mut self, source: &Path, destination: &Path) {
        self.path = crate::paths::moved_path(&self.path, source, destination, true);
        self.history.rename_path(source, destination);
    }

    pub fn restore(self, document: Document) -> Tab {
        let mut tab = Tab::new(document);
        tab.id = self.id;
        tab.editor_path = self.editor_path;
        tab.history = self.history;
        tab.view = self.view;
        tab.editing_view = self.editing_view;
        tab.note_views = self.note_views;
        tab.pinned = self.pinned;
        tab.image_scale = self.image_scale;
        tab
    }
}

/// Preserve the selected view and every pinned view, in existing tab order.
pub fn other_tab_indices(tabs: &[Tab], keep: u64) -> Vec<usize> {
    if !tabs.iter().any(|tab| tab.id() == keep) {
        return Vec::new();
    }
    tabs.iter()
        .enumerate()
        .filter(|(_, tab)| tab.id() != keep && !tab.pinned)
        .map(|(index, _)| index)
        .collect()
}

pub fn right_tab_indices(tabs: &[Tab], keep: u64) -> Vec<usize> {
    let Some(index) = tabs.iter().position(|tab| tab.id() == keep) else {
        return Vec::new();
    };
    tabs.iter()
        .enumerate()
        .skip(index + 1)
        .filter(|(_, tab)| !tab.pinned)
        .map(|(index, _)| index)
        .collect()
}

impl Tab {
    pub fn new(document: Document) -> Self {
        let editor_path = document.path.clone();
        let editing_view = if document.is_plain_text() {
            View::Source
        } else {
            View::Live
        };
        let note_views = document.is_plain_text().then_some((View::Live, View::Live));
        let view = if document.is_image() {
            View::Read
        } else {
            editing_view
        };
        let mut history = History::default();
        if !document.is_blank() {
            history.visit(&document.path);
        }
        Self {
            id: NEXT_TAB_ID.fetch_add(1, Ordering::Relaxed),
            document,
            history,
            view,
            pinned: false,
            editing_view,
            note_views,
            image_scale: ImageScale::Fit,
            edited_at: None,
            autosave_failed: false,
            editor_path,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn title(&self) -> String {
        if self.view == View::Graph {
            "Graph view".into()
        } else {
            self.document.title()
        }
    }

    pub fn into_closed(self) -> Option<ClosedTab> {
        if self.document.is_blank() && self.view != View::Graph {
            return None;
        }
        let fingerprint = if self.document.dirty() {
            None
        } else {
            saved_fingerprint(&self.document)
        };
        Some(ClosedTab {
            id: self.id,
            path: self.document.path,
            editor_path: self.editor_path,
            history: self.history,
            view: self.view,
            editing_view: self.editing_view,
            note_views: self.note_views,
            pinned: self.pinned,
            image_scale: self.image_scale,
            saved_fingerprint: fingerprint,
        })
    }

    pub fn editor_path(&self) -> &std::path::Path {
        &self.editor_path
    }

    pub fn open(&mut self, document: Document) -> Result<(), &'static str> {
        if self.document.dirty() {
            return Err("Save this note before navigating away.");
        }
        self.history.visit(&document.path);
        self.replace(document);
        self.show_document();
        Ok(())
    }

    pub fn replace(&mut self, document: Document) {
        let graph = self.view == View::Graph;
        if self.document.path != document.path {
            self.editor_path = document.path.clone();
        }
        if self.document.is_plain_text()
            && !document.is_plain_text()
            && let Some((view, editing_view)) = self.note_views.take()
        {
            self.view = view;
            self.editing_view = editing_view;
        }
        if document.is_image() {
            self.view = View::Read;
            if self.document.path != document.path {
                self.image_scale = ImageScale::Fit;
            }
        } else if document.is_plain_text() && self.document.path != document.path {
            if !self.document.is_plain_text() {
                let view = if self.document.is_image() || self.view == View::Graph {
                    self.editing_view
                } else {
                    self.view
                };
                self.note_views = Some((view, self.editing_view));
            }
            self.view = View::Source;
            self.editing_view = View::Source;
        } else if self.document.is_image() {
            self.view = self.editing_view;
        }
        self.document = document;
        if graph {
            self.view = View::Graph;
        }
        self.edited_at = None;
        self.autosave_failed = false;
    }

    pub fn edited(&mut self) {
        self.edited_at = Some(Instant::now());
        self.autosave_failed = false;
    }

    pub fn set_view(&mut self, view: View) {
        if self.document.is_image() && view != View::Graph {
            return;
        }
        if matches!(view, View::Source | View::Live) {
            self.editing_view = view;
        }
        self.view = view;
    }

    pub fn editing_view(&self) -> View {
        self.editing_view
    }

    pub fn show_document(&mut self) {
        if self.view == View::Graph {
            self.view = if self.document.is_image() {
                View::Read
            } else {
                self.editing_view
            };
        }
    }

    pub fn toggle_reading(&mut self) {
        if self.document.is_image() || self.view == View::Graph {
            return;
        }
        self.view = if self.view == View::Read {
            self.editing_view
        } else {
            View::Read
        };
    }
}
