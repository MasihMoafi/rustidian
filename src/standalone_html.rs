//! Standalone pages belong in the browser; explicit source tabs still restore normally.
use super::NotesApp;
use eframe::egui;
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
};

#[derive(Default)]
pub(super) struct Browser {
    requests: Vec<Receiver<Result<(), String>>>,
}

impl Browser {
    fn open(&mut self, path: PathBuf, ctx: &egui::Context) -> Result<(), String> {
        let wake = ctx.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::Builder::new()
            .name("rustidian-html-browser".into())
            .spawn(move || {
                let result = if !path.is_file() {
                    Err("This HTML file is no longer available.".into())
                } else {
                    crate::media_ui::desktop_opener()
                        .arg(&path)
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map_err(|error| format!("Cannot open HTML in your browser: {error}"))
                        .and_then(|status| {
                            if status.success() {
                                Ok(())
                            } else {
                                Err(format!("Cannot open HTML in your browser ({status})."))
                            }
                        })
                };
                let _ = sender.send(result);
                wake.request_repaint();
            })
            .map_err(|error| format!("Cannot start the HTML browser request: {error}"))?;
        self.requests.push(receiver);
        Ok(())
    }
}

impl NotesApp {
    /// Called only for a deliberate open, never while restoring saved tabs.
    pub(super) fn open_html_in_browser(&mut self, path: &Path) -> bool {
        if !rustidian::html_import::is_html(path) {
            return false;
        }
        let absolute = if path.is_absolute() {
            path.to_owned()
        } else if let Some(vault) = &self.vault {
            vault.root.join(path)
        } else {
            return false;
        };
        match self.html_browser.open(absolute, &self.ctx) {
            Ok(()) => {
                self.error = None;
                self.status = "Opening HTML in browser…".into();
            }
            Err(error) => self.error = Some(error),
        }
        true
    }

    pub(super) fn edit_html_source(&mut self, path: &Path) {
        self.open_source_note_at(path, true);
        if self
            .tabs
            .get(self.active)
            .is_some_and(|tab| tab.document.path == path)
        {
            self.set_view(rustidian::tab::View::Source);
        }
    }

    pub(super) fn poll_html_browser(&mut self) {
        for index in (0..self.html_browser.requests.len()).rev() {
            let result = match self.html_browser.requests[index].try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty) => continue,
                Err(TryRecvError::Disconnected) => {
                    Err("The HTML browser request was interrupted. Try again.".into())
                }
            };
            self.html_browser.requests.swap_remove(index);
            match result {
                Ok(()) => self.status = "Opened HTML in browser".into(),
                Err(error) => {
                    self.status.clear();
                    self.error = Some(error);
                }
            }
        }
    }
}
