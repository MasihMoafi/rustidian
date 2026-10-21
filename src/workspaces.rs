//! Named layouts contain paths and presentation state, never note contents.
use crate::{
    tab::{Tab, View},
    vault::{Document, Vault},
};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedTab {
    pub path: PathBuf,
    pub view: View,
    pub editing_view: View,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layout {
    pub tabs: Vec<SavedTab>,
    pub active: usize,
    pub left: bool,
    pub right: bool,
    pub left_width: f32,
    pub right_width: f32,
    pub right_tab: u8,
}

pub struct Restored {
    pub tabs: Vec<Tab>,
    pub active: usize,
}

impl Layout {
    fn validate(&self) -> Result<(), String> {
        if !self.left_width.is_finite() || !self.right_width.is_finite() {
            return Err("The workspace has invalid sidebar sizes.".into());
        }
        if (!self.tabs.is_empty() && self.active >= self.tabs.len())
            || self
                .tabs
                .iter()
                .any(|tab| !matches!(tab.editing_view, View::Live | View::Source))
        {
            return Err("The workspace has an invalid active tab or editing mode.".into());
        }
        if self.tabs.iter().any(|tab| {
            tab.path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        }) {
            return Err("Workspace files must stay inside this vault.".into());
        }
        Ok(())
    }

    pub fn restore(&self, vault: &Vault) -> Result<Restored, String> {
        self.validate()?;
        let mut tabs = Vec::new();
        let mut active = 0;
        for (index, saved) in self.tabs.iter().enumerate() {
            let document = if saved.path.as_os_str().is_empty() {
                Document::blank()
            } else {
                match vault.open_document(&saved.path) {
                    Ok(document) => document,
                    Err(_) => continue,
                }
            };
            if index == self.active {
                active = tabs.len();
            }
            let mut tab = Tab::new(document);
            tab.set_view(saved.editing_view);
            tab.set_view(saved.view);
            tabs.push(tab);
        }
        if tabs.is_empty() && !self.tabs.is_empty() {
            return Err(
                "None of this workspace's files can be opened. Your current tabs are unchanged."
                    .into(),
            );
        }
        Ok(Restored { tabs, active })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub name: String,
    pub modified_ms: i64,
    pub layout: Layout,
}

impl AsRef<str> for Entry {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    entries: Vec<Entry>,
    active: Option<String>,
}

pub struct Workspaces {
    location: PathBuf,
    stored: Stored,
}

pub fn location(root: &Path) -> Option<PathBuf> {
    crate::paths::state_file(root, "workspaces", "json")
}

impl Workspaces {
    pub fn open(root: &Path) -> Result<Self, String> {
        Self::load(location(root).ok_or("Cannot locate workspace storage.")?)
    }

    pub fn load(location: PathBuf) -> Result<Self, String> {
        let stored: Stored = match std::fs::read(&location) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("Cannot read workspaces: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Stored::default(),
            Err(error) => return Err(format!("Cannot read workspaces: {error}")),
        };
        for entry in &stored.entries {
            name_valid(&entry.name)?;
            entry.layout.validate()?;
        }
        Ok(Self { location, stored })
    }

    pub fn entries(&self) -> &[Entry] {
        &self.stored.entries
    }

    pub fn active(&self) -> Option<&str> {
        self.stored.active.as_deref()
    }

    pub fn save(&mut self, name: &str, layout: Layout, modified_ms: i64) -> Result<(), String> {
        name_valid(name)?;
        layout.validate()?;
        self.update(|stored| {
            let entry = Entry {
                name: name.into(),
                modified_ms,
                layout,
            };
            if let Some(previous) = stored.entries.iter_mut().find(|entry| entry.name == name) {
                *previous = entry;
            } else {
                stored.entries.push(entry);
            }
            stored.active = Some(name.into());
            Ok(())
        })
    }

    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        self.update(|stored| {
            stored.entries.retain(|entry| entry.name != name);
            if stored.active.as_deref() == Some(name) {
                stored.active = None;
            }
            Ok(())
        })
    }

    pub fn activate(&mut self, name: &str) -> Result<(), String> {
        self.update(|stored| {
            if !stored.entries.iter().any(|entry| entry.name == name) {
                return Err("This workspace no longer exists.".into());
            }
            stored.active = Some(name.into());
            Ok(())
        })
    }

    fn update(
        &mut self,
        change: impl FnOnce(&mut Stored) -> Result<(), String>,
    ) -> Result<(), String> {
        let parent = self
            .location
            .parent()
            .ok_or("Cannot locate workspace storage.")?;
        let save = || -> Result<Stored, String> {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            let _lock = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(self.location.with_extension("lock"))
                .and_then(|file| {
                    file.lock()?;
                    Ok(file)
                })
                .map_err(|error| error.to_string())?;
            let mut latest = Self::load(self.location.clone())?.stored;
            change(&mut latest)?;
            let bytes = serde_json::to_vec_pretty(&latest).map_err(|error| error.to_string())?;
            let mut file =
                tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
            file.write_all(&bytes)
                .and_then(|()| file.as_file().sync_all())
                .map_err(|error| error.to_string())?;
            file.persist(&self.location)
                .map_err(|error| error.to_string())?;
            Ok(latest)
        };
        self.stored = save().map_err(|error| format!("Cannot save workspaces: {error}"))?;
        Ok(())
    }
}

fn name_valid(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.chars().any(char::is_control) {
        Err("Enter a workspace name without line breaks.".into())
    } else {
        Ok(())
    }
}
