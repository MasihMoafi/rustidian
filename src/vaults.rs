//! The vault chooser's list lives outside the notes it opens.
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Component, Path, PathBuf},
};

#[derive(Default, Serialize, Deserialize)]
struct Stored {
    roots: Vec<PathBuf>,
}

pub struct Vaults {
    location: PathBuf,
    stored: Stored,
    /// Folders that always head the list, in this order (Desktop, then the default vault).
    pinned: Vec<PathBuf>,
}

/// Masih's two starting places stay on top however many folders agents open (5 Nov: 29 roots
/// buried them).
fn default_pins() -> Vec<PathBuf> {
    let desktop = std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Desktop"));
    desktop
        .into_iter()
        .chain(default_vault())
        .filter_map(|path| folder(&path).ok())
        .fold(Vec::new(), |mut pins, path| {
            if !pins.contains(&path) {
                pins.push(path);
            }
            pins
        })
}

fn pin_first(roots: &mut Vec<PathBuf>, pinned: &[PathBuf]) {
    roots.retain(|root| !pinned.contains(root));
    roots.splice(0..0, pinned.iter().cloned());
}

pub fn location() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|base| crate::paths::app_directory(&base).join("vaults.json"))
}

/// Where the vault Masih last read is kept: beside the vault list, outside his notes.
fn last_opened_file() -> Option<PathBuf> {
    location().map(|catalog| catalog.with_file_name("last-vault"))
}

/// The vault Masih was last reading, and the page when it lies in another vault
/// than the window's own. A launch without a file opens it again, with the tabs
/// and scroll the vault's session file keeps (5 Nov: reopening forgot the vault).
pub fn last_opened() -> Option<(PathBuf, Option<PathBuf>)> {
    read_last(&last_opened_file()?)
}

/// Remember the vault (and page) that Masih looked at last; best effort.
pub fn remember_last_opened(root: &Path, document: Option<&Path>) {
    if let Some(file) = last_opened_file() {
        write_last(&file, root, document);
    }
}

fn read_last(file: &Path) -> Option<(PathBuf, Option<PathBuf>)> {
    let text = std::fs::read_to_string(file).ok()?;
    let mut lines = text.lines();
    let root = PathBuf::from(lines.next()?);
    let document = lines
        .next()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from);
    root.is_dir().then_some((root, document))
}

fn write_last(file: &Path, root: &Path, document: Option<&Path>) {
    let text = format!(
        "{}\n{}\n",
        root.display(),
        document
            .map(|path| path.display().to_string())
            .unwrap_or_default()
    );
    if std::fs::read_to_string(file).is_ok_and(|current| current == text) {
        return;
    }
    if let Some(folder) = file.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let temporary = file.with_extension("tmp");
    if std::fs::write(&temporary, text).is_ok() && std::fs::rename(&temporary, file).is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
}

pub fn default_vault() -> Option<PathBuf> {
    std::env::var_os("RUSTIDIAN_DEFAULT_VAULT")
        .or_else(|| std::env::var_os("RUST_NOTES_DEFAULT_VAULT"))
        .map(PathBuf::from)
}

/// CLI and native Open share the same document-to-vault policy.
pub fn resolve_input(
    input: &Path,
    default_vault: Option<&Path>,
    registered_vaults: &[PathBuf],
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let absolute = input
        .canonicalize()
        .map_err(|error| format!("Cannot open {}: {error}", input.display()))?;
    if absolute.is_dir() {
        return Ok((absolute, None));
    }
    if !absolute.is_file() || !crate::vault::is_text_document(&absolute) {
        return Err(
            "Choose a vault folder or a supported Markdown, text or source-code file.".into(),
        );
    }
    let default_vault = default_vault.and_then(|vault| vault.canonicalize().ok());
    // A registered vault that encloses the default vault (Masih's ~/Desktop around
    // ~/Desktop/o) is too broad to claim documents: a file outside the default vault
    // opens with its own folder instead of loading everything above it.
    let root = default_vault
        .clone()
        .into_iter()
        .chain(
            registered_vaults
                .iter()
                .filter_map(|vault| vault.canonicalize().ok())
                .filter(|vault| !encloses(vault, default_vault.as_deref())),
        )
        .filter(|vault| absolute.starts_with(vault) && vault.is_dir())
        .max_by_key(|vault| vault.components().count())
        .or_else(|| absolute.parent().map(Path::to_path_buf))
        .ok_or("Cannot determine the document's folder.")?;
    let document = absolute
        .strip_prefix(&root)
        .map_err(|error| error.to_string())?
        .to_owned();
    Ok((root, Some(document)))
}

/// Whether `vault` is a folder around the default vault rather than the vault.
pub fn encloses(vault: &Path, default: Option<&Path>) -> bool {
    default.is_some_and(|default| default != vault && default.starts_with(vault))
}

pub fn folder(path: &Path) -> Result<PathBuf, String> {
    let root = path
        .canonicalize()
        .map_err(|error| format!("Cannot open folder: {error}"))?;
    if !root.is_dir() {
        return Err("Choose a folder for your vault.".into());
    }
    Ok(root)
}

pub fn create(parent: &Path, name: &str) -> Result<PathBuf, String> {
    let mut components = Path::new(name).components();
    if name.is_empty()
        || name.trim() != name
        || name.chars().any(char::is_control)
        || name.contains(['/', '\\'])
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("Enter a vault name without slashes or surrounding spaces.".into());
    }
    let root = folder(parent)?.join(name);
    std::fs::create_dir(&root).map_err(|error| format!("Cannot create vault: {error}"))?;
    Ok(root)
}

impl Vaults {
    pub fn open() -> Result<Self, String> {
        Self::load_pinned(
            location().ok_or("Cannot locate the vault list.")?,
            default_pins(),
        )
    }

    pub fn load(location: PathBuf) -> Result<Self, String> {
        Self::load_pinned(location, Vec::new())
    }

    pub fn load_pinned(location: PathBuf, pinned: Vec<PathBuf>) -> Result<Self, String> {
        let mut stored = match std::fs::read(&location) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("Cannot read the vault list: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Stored::default(),
            Err(error) => return Err(format!("Cannot read the vault list: {error}")),
        };
        pin_first(&mut stored.roots, &pinned);
        Ok(Self {
            location,
            stored,
            pinned,
        })
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.stored.roots
    }

    pub fn remember(&mut self, root: &Path) -> Result<(), String> {
        let root = folder(root)?;
        self.update(|roots| {
            roots.retain(|path| *path != root);
            roots.insert(0, root);
        })
    }

    pub fn forget(&mut self, root: &Path) -> Result<(), String> {
        self.update(|roots| roots.retain(|path| path != root))
    }

    fn update(&mut self, change: impl FnOnce(&mut Vec<PathBuf>)) -> Result<(), String> {
        let parent = self
            .location
            .parent()
            .ok_or("Cannot locate the vault list.")?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Cannot save the vault list: {error}"))?;
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
            .map_err(|error| format!("Cannot lock the vault list: {error}"))?;
        let mut latest = Self::load_pinned(self.location.clone(), self.pinned.clone())?.stored;
        let before = latest.roots.clone();
        change(&mut latest.roots);
        pin_first(&mut latest.roots, &self.pinned);
        if latest.roots == before {
            self.stored = latest;
            Ok(())
        } else {
            self.write(latest.roots)
        }
    }

    fn write(&mut self, roots: Vec<PathBuf>) -> Result<(), String> {
        let stored = Stored { roots };
        let bytes = serde_json::to_vec_pretty(&stored).map_err(|error| error.to_string())?;
        let parent = self
            .location
            .parent()
            .ok_or("Cannot locate the vault list.")?;
        let save = || -> std::io::Result<()> {
            let mut file = tempfile::NamedTempFile::new_in(parent)?;
            file.write_all(&bytes)?;
            file.as_file().sync_all()?;
            file.persist(&self.location)?;
            Ok(())
        };
        save().map_err(|error| format!("Cannot save the vault list: {error}"))?;
        self.stored = stored;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_last_read_vault_and_page_are_kept_and_a_missing_vault_is_forgotten() {
        let home = tempfile::tempdir().unwrap();
        let vault = tempfile::tempdir().unwrap();
        let file = home.path().join("state/last-vault");
        assert_eq!(read_last(&file), None, "nothing is remembered at first");
        write_last(&file, vault.path(), None);
        assert_eq!(read_last(&file), Some((vault.path().to_owned(), None)));
        write_last(&file, vault.path(), Some(Path::new("Notes/Agent.md")));
        assert_eq!(
            read_last(&file),
            Some((
                vault.path().to_owned(),
                Some(PathBuf::from("Notes/Agent.md"))
            ))
        );
        drop(vault);
        assert_eq!(read_last(&file), None, "a vault that is gone opens nothing");
    }

    use super::*;
    use std::fs;

    #[test]
    fn recent_vaults_keep_distinct_paths_and_removal_preserves_notes() {
        let base = tempfile::tempdir().unwrap();
        let a = base.path().join("a/Notes");
        let b = base.path().join("b/Notes");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("Keep.md"), "original").unwrap();
        let location = base.path().join("state/vaults.json");
        let mut list = Vaults::load(location.clone()).unwrap();
        list.remember(&a).unwrap();
        list.remember(&b).unwrap();
        list.remember(&a.join(".")).unwrap();
        assert_eq!(list.roots(), [a.clone(), b.clone()]);
        assert_eq!(
            Vaults::load(location.clone()).unwrap().roots(),
            list.roots()
        );
        list.forget(&a).unwrap();
        assert_eq!(Vaults::load(location).unwrap().roots(), [b]);
        assert_eq!(fs::read_to_string(a.join("Keep.md")).unwrap(), "original");
    }

    #[test]
    fn the_desktop_and_the_main_vault_stay_on_top_however_many_folders_are_opened() {
        let base = tempfile::tempdir().unwrap();
        let [desktop, main, other, newer] =
            ["Desktop", "Main", "Other", "Newer"].map(|name| base.path().join(name));
        for folder in [&desktop, &main, &other, &newer] {
            fs::create_dir_all(folder).unwrap();
        }
        let pins = vec![
            desktop.canonicalize().unwrap(),
            main.canonicalize().unwrap(),
        ];
        let location = base.path().join("state/vaults.json");
        let mut list = Vaults::load_pinned(location.clone(), pins.clone()).unwrap();
        assert_eq!(list.roots(), pins, "pins show before anything is stored");
        list.remember(&other).unwrap();
        list.remember(&newer).unwrap();
        list.remember(&main).unwrap();
        let expected = [
            pins[0].clone(),
            pins[1].clone(),
            newer.canonicalize().unwrap(),
            other.canonicalize().unwrap(),
        ];
        assert_eq!(list.roots(), expected);
        assert_eq!(
            Vaults::load_pinned(location, pins).unwrap().roots(),
            expected
        );
    }

    #[test]
    fn failed_list_writes_keep_the_previous_entries_and_corruption_is_reported() {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("Notes");
        fs::create_dir(&root).unwrap();
        let location = base.path().join("vaults.json");
        let mut list = Vaults::load(location.clone()).unwrap();
        list.remember(&root).unwrap();
        fs::remove_file(&location).unwrap();
        fs::create_dir(&location).unwrap();
        assert!(list.forget(&root).is_err());
        assert_eq!(list.roots(), [root]);
        fs::remove_dir(&location).unwrap();
        fs::write(&location, "{broken").unwrap();
        assert!(Vaults::load(location.clone()).is_err());
        assert_eq!(fs::read_to_string(location).unwrap(), "{broken");
    }

    #[test]
    fn separate_windows_preserve_each_others_vaults_and_removals() {
        let base = tempfile::tempdir().unwrap();
        let a = create(base.path(), "First").unwrap();
        let b = create(base.path(), "Second").unwrap();
        let location = base.path().join("shared/vaults.json");
        let mut first = Vaults::load(location.clone()).unwrap();
        let mut second = Vaults::load(location.clone()).unwrap();
        first.remember(&a).unwrap();
        second.remember(&b).unwrap();
        assert_eq!(
            Vaults::load(location.clone()).unwrap().roots(),
            [b.clone(), a.clone()]
        );
        first.forget(&a).unwrap();
        assert_eq!(
            Vaults::load(location).unwrap().roots(),
            std::slice::from_ref(&b)
        );

        let location = base.path().join("parallel/vaults.json");
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let writers: Vec<_> = [a.clone(), b.clone()]
            .into_iter()
            .map(|root| {
                let mut list = Vaults::load(location.clone()).unwrap();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    list.remember(&root).unwrap();
                })
            })
            .collect();
        for writer in writers {
            writer.join().unwrap();
        }
        let saved = Vaults::load(location).unwrap();
        assert_eq!(saved.roots().len(), 2);
        assert!(saved.roots().contains(&a));
        assert!(saved.roots().contains(&b));
        assert!(a.is_dir() && b.is_dir());
    }

    #[test]
    fn creating_a_vault_is_contained_empty_and_never_reuses_a_destination() {
        let base = tempfile::tempdir().unwrap();
        let root = create(base.path(), "یادداشت Notes").unwrap();
        assert_eq!(root.parent(), Some(base.path()));
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::write(root.join("Keep.md"), "original").unwrap();
        assert!(create(base.path(), "یادداشت Notes").is_err());
        for name in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "/escape",
            " padded ",
            "bad\nname",
            "bad\0name",
        ] {
            assert!(create(base.path(), name).is_err(), "{name}");
        }
        assert!(create(&base.path().join("missing"), "Notes").is_err());
        assert_eq!(
            fs::read_to_string(root.join("Keep.md")).unwrap(),
            "original"
        );
    }

    #[test]
    fn a_loose_file_beside_the_default_vault_loads_only_its_own_folder() {
        // Masih's ~/Desktop/info.md: ~/Desktop holds the default vault `o` and
        // every project, so it must not load its subfolders.
        let desktop = tempfile::tempdir().unwrap();
        let desktop = desktop.path().canonicalize().unwrap();
        let default = desktop.join("o");
        fs::create_dir_all(default.join("deep")).unwrap();
        fs::create_dir_all(desktop.join("p/repo")).unwrap();
        fs::write(desktop.join("info.md"), "# info").unwrap();
        fs::write(default.join("deep/Vault note.md"), "x").unwrap();
        fs::write(desktop.join("p/repo/README.md"), "x").unwrap();
        let (root, document) = resolve_input(
            &desktop.join("info.md"),
            Some(&default),
            std::slice::from_ref(&desktop),
        )
        .unwrap();
        assert_eq!(
            (root.as_path(), document.as_deref()),
            (desktop.as_path(), Some(Path::new("info.md")))
        );
        let vault = crate::vault::Vault::open_beside(&root, Some(&default), || false).unwrap();
        let notes = vault
            .notes
            .iter()
            .map(|note| note.path.clone())
            .collect::<Vec<_>>();
        assert_eq!(notes, [PathBuf::from("info.md")]);
        assert!(vault.folders.is_empty(), "{:?}", vault.folders);
        // The default vault itself still loads in full.
        let vault = crate::vault::Vault::open_beside(&default, Some(&default), || false).unwrap();
        assert_eq!(vault.notes.len(), 1);
    }
}
