use crate::{navigation::History, vault::Document};
use serde::{Deserialize, Serialize};
use std::{
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
    pub image_scale: ImageScale,
    pub edited_at: Option<Instant>,
    pub autosave_failed: bool,
    // Editing state follows a moved file, but never leaks into another document.
    editor_path: std::path::PathBuf,
}

impl Tab {
    pub fn new(document: Document) -> Self {
        let editor_path = document.path.clone();
        let editing_view = if document.is_plain_text() {
            View::Source
        } else {
            View::Live
        };
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
            image_scale: ImageScale::Fit,
            edited_at: None,
            autosave_failed: false,
            editor_path,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
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
        Ok(())
    }

    pub fn replace(&mut self, document: Document) {
        if self.document.path != document.path {
            self.editor_path = document.path.clone();
        }
        if document.is_image() {
            self.view = View::Read;
            if self.document.path != document.path {
                self.image_scale = ImageScale::Fit;
            }
        } else if document.is_plain_text() && self.document.path != document.path {
            self.view = View::Source;
            self.editing_view = View::Source;
        } else if self.document.is_image() {
            self.view = self.editing_view;
        }
        self.document = document;
        self.edited_at = None;
        self.autosave_failed = false;
    }

    pub fn edited(&mut self) {
        self.edited_at = Some(Instant::now());
        self.autosave_failed = false;
    }

    pub fn set_view(&mut self, view: View) {
        if self.document.is_image() {
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

    pub fn toggle_reading(&mut self) {
        if self.document.is_image() {
            return;
        }
        self.view = if self.view == View::Read {
            self.editing_view
        } else {
            View::Read
        };
    }
}
