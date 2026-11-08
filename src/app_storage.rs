//! Save continuations stay on the UI; only completed writes can release them.
use super::{NotesApp, Pending};
use eframe::egui;
use std::{
    collections::{HashSet, VecDeque},
    path::PathBuf,
    sync::mpsc::Receiver,
};

type Action = Box<dyn FnOnce(&mut NotesApp)>;
type SavedAction = Box<dyn FnOnce(&mut NotesApp, Result<rustidian::vault::Document, String>)>;
struct DetachedSave {
    root: PathBuf,
    document: rustidian::vault::Document,
    action: SavedAction,
}
struct Creating {
    root: PathBuf,
    tab: Option<u64>,
    receiver: Receiver<Result<rustidian::vault::Document, String>>,
}
struct AfterSave {
    root: Option<PathBuf>,
    tabs: Vec<(u64, PathBuf)>,
    action: Action,
    failure: Option<Pending>,
    dialog: bool,
    save_dirty: bool,
}

pub(super) struct Copying {
    pub receiver: Receiver<Result<(PathBuf, rustidian::vault::Vault), String>>,
    pub root: PathBuf,
    pub generation: u64,
    pub cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Drop for Copying {
    fn drop(&mut self) {
        self.cancelled
            .store(true, std::sync::atomic::Ordering::Release);
    }
}

#[derive(Default)]
pub(super) struct State {
    pub requests: HashSet<u64>,
    after: VecDeque<AfterSave>,
    pub copying: Option<Copying>,
    pub generation: u64,
    detached: std::collections::HashMap<u64, DetachedSave>,
    creating: Option<Creating>,
}

impl State {
    pub fn writing(&self) -> bool {
        !self.requests.is_empty()
            || self.creating.is_some()
            || !self.detached.is_empty()
            || !self.after.is_empty()
    }
    pub fn creating(&self) -> bool {
        self.creating.is_some()
    }
}

impl NotesApp {
    pub(super) fn create_untitled(&mut self, folder: &std::path::Path) {
        if self.storage.creating.is_some() {
            self.error =
                Some("A new note is still being saved. Try again when it finishes.".into());
            return;
        }
        let Some(mut vault) = self.vault.clone() else {
            self.error = Some("Open a vault before creating a note.".into());
            return;
        };
        let root = vault.root.clone();
        let folder = folder.to_owned();
        let tab = self.tabs.get(self.active).map(|tab| tab.id());
        let wake = self.ctx.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        match std::thread::Builder::new()
            .name("rustidian-create-note".into())
            .spawn(move || {
                let _ = sender.send(vault.create_untitled_in(&folder));
                wake.request_repaint();
            }) {
            Ok(_) => {
                self.storage.creating = Some(Creating {
                    root,
                    tab,
                    receiver,
                })
            }
            Err(error) => self.error = Some(format!("Cannot start note creation: {error}")),
        }
    }

    pub(super) fn poll_created_note(&mut self) {
        let Some(creating) = &self.storage.creating else {
            return;
        };
        let result = match creating.receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("Note creation was interrupted. Refresh the vault before trying again.".into())
            }
        };
        let Some(creating) = self.storage.creating.take() else {
            return;
        };
        if self
            .vault
            .as_ref()
            .is_none_or(|vault| vault.root != creating.root)
        {
            return;
        }
        match result {
            Ok(mut document) => {
                if let Some(vault) = &mut self.vault {
                    let text = document.text.clone();
                    vault.commit_save(&mut document, text);
                }
                if let Some(index) = self
                    .tabs
                    .iter()
                    .position(|tab| Some(tab.id()) == creating.tab)
                {
                    self.active = index;
                }
                self.show_created_note(document);
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub(super) fn write_document(
        &mut self,
        document: rustidian::vault::Document,
        action: impl FnOnce(&mut Self, Result<rustidian::vault::Document, String>) + 'static,
    ) {
        let Some(vault) = &self.vault else {
            action(self, Err("Open a vault first.".into()));
            return;
        };
        if self
            .storage
            .detached
            .values()
            .any(|job| job.root == vault.root && job.document.path == document.path)
        {
            return;
        }
        let save = match vault.prepare_save(&document) {
            Ok(save) => save,
            Err(error) => {
                action(self, Err(error));
                return;
            }
        };
        let id = rustidian::tab::Tab::new(document.clone()).id();
        self.storage.detached.insert(
            id,
            DetachedSave {
                root: vault.root.clone(),
                document,
                action: Box::new(action),
            },
        );
        let wake = self.ctx.clone();
        self.autosave
            .submit(id, save, move || wake.request_repaint());
    }

    pub(super) fn finish_detached_save(
        &mut self,
        done: rustidian::autosave::Done,
    ) -> Option<rustidian::autosave::Done> {
        let Some(mut job) = self.storage.detached.remove(&done.tab) else {
            return Some(done);
        };
        let vault = self.vault.as_mut()?;
        if vault.root != job.root {
            return None;
        }
        let result = done.result.map(|()| {
            vault.commit_save(&mut job.document, done.save.text);
            job.document
        });
        (job.action)(self, result);
        None
    }
    pub(super) fn copy_folder(&mut self, path: &std::path::Path) {
        if self.storage.copying.is_some() {
            self.error = Some("A folder copy is already running.".into());
            return;
        }
        let Some(mut vault) = self.vault.clone() else {
            return;
        };
        let root = vault.root.clone();
        let path = path.to_owned();
        let wake = self.ctx.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancelling = cancelled.clone();
        match std::thread::Builder::new()
            .name("rustidian-folder-copy".into())
            .spawn(move || {
                let warnings = vault.warnings.len();
                let result = vault
                    .duplicate_folder_cancellable(&path, || {
                        cancelling.load(std::sync::atomic::Ordering::Acquire)
                    })
                    .map(|copy| {
                        drop(vault.warnings.drain(..warnings));
                        let prefix = format!("{}: ", copy.display());
                        for warning in &mut vault.warnings {
                            *warning = warning.strip_prefix(&prefix).unwrap_or(warning).to_owned();
                        }
                        vault.notes.retain(|note| note.path.starts_with(&copy));
                        for note in &mut vault.notes {
                            note.path = note
                                .path
                                .strip_prefix(&copy)
                                .unwrap_or(&note.path)
                                .to_owned();
                        }
                        vault.attachments = vault
                            .attachments
                            .into_iter()
                            .filter_map(|path| {
                                path.strip_prefix(&copy).ok().map(std::path::Path::to_owned)
                            })
                            .collect();
                        vault.folders = vault
                            .folders
                            .into_iter()
                            .filter_map(|path| {
                                path.strip_prefix(&copy)
                                    .ok()
                                    .filter(|path| !path.as_os_str().is_empty())
                                    .map(std::path::Path::to_owned)
                            })
                            .collect();
                        (copy, vault)
                    });
                let _ = sender.send(result);
                wake.request_repaint();
            }) {
            Ok(_) => {
                self.storage.copying = Some(Copying {
                    receiver,
                    root,
                    generation: self.storage.generation,
                    cancelled,
                });
                self.status = "Copying folder…".into();
            }
            Err(error) => self.error = Some(format!("Cannot start folder copying: {error}")),
        }
    }

    pub(super) fn poll_folder_copy(&mut self) {
        let Some(copying) = &self.storage.copying else {
            return;
        };
        let result = match copying.receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err("Folder copying was interrupted. Refresh the vault before trying again.".into())
            }
        };
        let Some(copying) = self.storage.copying.take() else {
            return;
        };
        if self
            .vault
            .as_ref()
            .is_none_or(|vault| vault.root != copying.root)
            || self
                .loading_root
                .as_ref()
                .is_some_and(|root| *root != copying.root)
        {
            return;
        }
        match result {
            Ok((copy, index)) => {
                if copying.generation == self.storage.generation {
                    if let Some(vault) = &mut self.vault {
                        vault.commit_folder_copy(copy.clone(), index);
                    }
                    self.refresh_file_views();
                } else {
                    self.queue_index(copying.root.clone());
                }
                self.status = format!("Copied folder to {}", copy.display());
                self.reveal_folder = Some(copy.clone());
                crate::explorer_selection::select(&self.ctx, &copy);
                self.error = None;
            }
            Err(error) => self.error = Some(error),
        }
    }
    pub(super) fn active_needs_save(&self) -> bool {
        self.tabs
            .get(self.active)
            .is_some_and(|tab| tab.document.dirty() || self.autosave.busy(tab.id()))
    }

    pub(super) fn save_active_then(&mut self, action: impl FnOnce(&mut Self) + 'static) -> bool {
        let Some(id) = self.tabs.get(self.active).map(|tab| tab.id()) else {
            return false;
        };
        self.defer_saves(vec![id], true, None, false, move |app| {
            if let Some(index) = app.tabs.iter().position(|tab| tab.id() == id) {
                app.active = index;
                action(app);
            }
        })
    }
    pub(super) fn finish_pending_save(&mut self, mut next: Pending, ctx: &egui::Context) {
        if let Pending::RestoreSnapshot(request) = &mut next
            && let Some(tab) = self
                .tabs
                .iter()
                .find(|tab| tab.document.path == request.expected.path)
        {
            request.expected = tab.document.clone();
        }
        self.pending = None;
        self.run_pending(next, ctx);
    }
    pub(super) fn tab_ids(&self) -> Vec<u64> {
        self.tabs.iter().map(|tab| tab.id()).collect()
    }

    pub(super) fn defer_saves(
        &mut self,
        ids: Vec<u64>,
        save_dirty: bool,
        failure: Option<Pending>,
        dialog: bool,
        action: impl FnOnce(&mut Self) + 'static,
    ) -> bool {
        let tabs: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| ids.contains(&tab.id()))
            .filter(|tab| self.autosave.busy(tab.id()) || (save_dirty && tab.document.dirty()))
            .map(|tab| (tab.id(), tab.document.path.clone()))
            .collect();
        if tabs.is_empty() && self.storage.detached.is_empty() && self.storage.creating.is_none() {
            return false;
        }
        if self.storage.after.len() >= 32 {
            self.error = Some(
                "Another action is waiting for a save. Wait or press Escape to cancel it.".into(),
            );
            return true;
        }
        if dialog && self.storage.after.iter().any(|job| job.dialog) {
            return true;
        }
        for (id, _) in &tabs {
            if !self.autosave.busy(*id)
                && let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id() == *id)
            {
                tab.autosave_failed = false;
            }
        }
        self.storage.after.push_back(AfterSave {
            root: self.vault.as_ref().map(|vault| vault.root.clone()),
            tabs,
            action: Box::new(action),
            failure,
            dialog,
            save_dirty,
        });
        self.ctx.request_repaint();
        true
    }

    pub(super) fn cancel_save_actions(&mut self) {
        self.storage.after.clear();
    }

    pub(super) fn poll_save_actions(&mut self, ctx: &egui::Context) {
        if self.pending.is_none()
            && self.dialog.is_none()
            && !self.storage.after.is_empty()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.cancel_save_actions();
        }
        let requests: Vec<_> = self.storage.requests.iter().copied().collect();
        for id in requests {
            let Some(index) = self.tabs.iter().position(|tab| tab.id() == id) else {
                self.storage.requests.remove(&id);
                continue;
            };
            if self.autosave.busy(id) {
                continue;
            }
            if self.tabs[index].autosave_failed || !self.tabs[index].document.dirty() {
                self.storage.requests.remove(&id);
            } else {
                self.save(index);
            }
        }
        let Some(job) = self.storage.after.pop_front() else {
            return;
        };
        if job.root != self.vault.as_ref().map(|vault| vault.root.clone())
            || (job.dialog && self.pending.is_none())
        {
            return;
        }
        let mut waiting = !self.storage.detached.is_empty() || self.storage.creating.is_some();
        for (id, path) in &job.tabs {
            let Some(index) = self
                .tabs
                .iter()
                .position(|tab| tab.id() == *id && tab.document.path == *path)
            else {
                return;
            };
            if self.autosave.busy(*id) {
                waiting = true;
                continue;
            }
            if job.save_dirty && self.tabs[index].autosave_failed {
                if let Some(failure) = job.failure {
                    self.pending = Some(failure);
                }
                return;
            }
            if job.save_dirty && self.tabs[index].document.dirty() {
                self.save(index);
                waiting = true;
            }
        }
        if waiting {
            self.storage.after.push_front(job);
        } else {
            (job.action)(self);
        }
    }
}
