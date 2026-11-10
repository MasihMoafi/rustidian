//! Text files on this computer outside the open vault, so Ctrl+O can find any
//! file by name. The list is built once, in the background, the first time the
//! dialog opens; hidden folders, dependency and build folders and the vault
//! itself are skipped.
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const SKIPPED: &[&str] = &[
    "node_modules",
    "target",
    "__pycache__",
    "venv",
    "snap",
    "dist",
    "build",
];
const LIMIT: usize = 200_000;

/// Every text document under `root`, outside `vault`, up to a fixed limit.
pub fn scan(root: &Path, vault: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut folders = vec![root.to_owned()];
    while let Some(folder) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                if !SKIPPED.contains(&name.as_ref()) && path != vault {
                    folders.push(path);
                }
            } else if kind.is_file() && crate::vault::is_text_document(&path) {
                found.push(path);
                if found.len() >= LIMIT {
                    return found;
                }
            }
        }
    }
    found
}

/// The folder to search: `RUSTIDIAN_ELSEWHERE_ROOT` for tests, else the home folder.
pub fn root() -> Option<PathBuf> {
    std::env::var_os("RUSTIDIAN_ELSEWHERE_ROOT")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// The background-built list; empty until the scan finishes.
#[derive(Clone, Default)]
pub struct Index(Arc<Mutex<Option<crate::search::FileMatcher>>>);

impl Index {
    pub fn start(root: PathBuf, vault: PathBuf, wake: impl Fn() + Send + 'static) -> Self {
        let index = Self::default();
        let slot = index.0.clone();
        let _ = std::thread::Builder::new()
            .name("rustidian-elsewhere".into())
            .spawn(move || {
                let vault = vault.canonicalize().unwrap_or(vault);
                let matcher = crate::search::FileMatcher::elsewhere(&root, scan(&root, &vault));
                *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(matcher);
                wake();
            });
        index
    }

    /// Ranked absolute paths, or `None` while the list is still being built.
    pub fn rank(&self, query: &str, limit: usize) -> Option<Vec<PathBuf>> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
            .map(|matcher| matcher.rank(query, limit))
    }
}
