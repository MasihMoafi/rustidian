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
}

pub fn location() -> Option<PathBuf> {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .map(|base| crate::paths::app_directory(&base).join("vaults.json"))
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
        return Err("Choose a vault folder, Markdown file (.md) or plain-text file (.txt).".into());
    }
    let root = default_vault
        .and_then(|vault| vault.canonicalize().ok())
        .into_iter()
        .chain(
            registered_vaults
                .iter()
                .filter_map(|vault| vault.canonicalize().ok()),
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
        Self::load(location().ok_or("Cannot locate the vault list.")?)
    }

    pub fn load(location: PathBuf) -> Result<Self, String> {
        let stored = match std::fs::read(&location) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|error| format!("Cannot read the vault list: {error}"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Stored::default(),
            Err(error) => return Err(format!("Cannot read the vault list: {error}")),
        };
        Ok(Self { location, stored })
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
        let mut latest = Self::load(self.location.clone())?.stored;
        let before = latest.roots.clone();
        change(&mut latest.roots);
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
}
