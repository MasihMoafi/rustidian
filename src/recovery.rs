//! Local note history, with separate metadata and content tables.
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, WriteTransaction};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

type Result<T> = std::result::Result<T, String>;
type StorageResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const FILES: TableDefinition<&str, u64> = TableDefinition::new("files");
const VERSIONS: TableDefinition<(&str, u64), (i64, u64)> = TableDefinition::new("versions");
const CONTENT: TableDefinition<(&str, u64), &str> = TableDefinition::new("content");
const COUNTERS: TableDefinition<&str, u64> = TableDefinition::new("counters");

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub interval_minutes: f64,
    pub keep_days: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            interval_minutes: 5.0,
            keep_days: 7.0,
        }
    }
}

impl Config {
    pub fn from_obsidian(root: &Path) -> Result<Self> {
        let config: Self = crate::paths::read_obsidian(root, "file-recovery.json")?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if !self.interval_minutes.is_finite()
            || self.interval_minutes < 0.0
            || !self.keep_days.is_finite()
            || self.keep_days < 1.0
        {
            Err(
                "Snapshot interval must be at least zero and history length at least one day."
                    .into(),
            )
        } else {
            Ok(())
        }
    }

    fn cutoff(&self, now_ms: i64) -> i64 {
        now_ms.saturating_sub((self.keep_days * 86_400_000.0) as i64)
    }
}

#[derive(Clone, Debug)]
pub struct Version {
    pub id: u64,
    pub timestamp_ms: i64,
    pub bytes: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    Context,
    Removed,
    Added,
}

/// Compute once when a snapshot is selected, using the Rust histogram diff.
pub fn changes(before: &str, after: &str) -> Vec<(Change, String)> {
    use imara_diff::{Algorithm, Diff, InternedInput};
    let input = InternedInput::new(before, after);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    let mut lines = Vec::new();
    let mut position = 0;
    for hunk in diff.hunks() {
        for token in &input.before[position..hunk.before.start as usize] {
            lines.push((Change::Context, input.interner[*token].to_owned()));
        }
        for token in &input.before[hunk.before.start as usize..hunk.before.end as usize] {
            lines.push((Change::Removed, input.interner[*token].to_owned()));
        }
        for token in &input.after[hunk.after.start as usize..hunk.after.end as usize] {
            lines.push((Change::Added, input.interner[*token].to_owned()));
        }
        position = hunk.before.end as usize;
    }
    for token in &input.before[position..] {
        lines.push((Change::Context, input.interner[*token].to_owned()));
    }
    lines
}

#[derive(Clone)]
pub struct History {
    location: PathBuf,
}

impl History {
    pub fn at(location: PathBuf) -> Self {
        Self { location }
    }

    pub fn open(root: &Path) -> Result<Self> {
        let location = crate::paths::state_file(root, "recovery", "redb")
            .ok_or("Cannot locate file recovery storage.")?;
        Ok(Self::at(location))
    }

    fn exists(&self) -> Result<bool> {
        match fs::symlink_metadata(&self.location) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => Ok(true),
            Ok(_) => Err("File recovery storage must be an ordinary file.".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("Cannot read file recovery storage: {error}")),
        }
    }

    fn access<T>(&self, change: impl FnOnce(&mut Database) -> StorageResult<T>) -> Result<T> {
        self.exists()?;
        let operation = || -> StorageResult<T> {
            let parent = self
                .location
                .parent()
                .ok_or("Cannot locate file recovery storage.")?;
            let mut folders = fs::DirBuilder::new();
            folders.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                folders.mode(0o700);
            }
            folders.create(parent)?;
            // A stable lock also serializes separate app processes. It survives
            // database compaction and is held until all transactions are closed.
            let lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(self.location.with_extension("lock"))?;
            lock.lock()?;
            let exists = self.exists()?;
            let mut builder = Database::builder();
            builder.set_cache_size(8 * 1024 * 1024);
            let mut database = if exists {
                builder.open(&self.location)?
            } else {
                let database = builder.create(&self.location)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&self.location, fs::Permissions::from_mode(0o600))?;
                    fs::File::open(parent)?.sync_all()?;
                }
                database
            };
            change(&mut database)
        };
        operation().map_err(|error| format!("Cannot access local history: {error}"))
    }

    pub fn files(&self) -> Result<Vec<PathBuf>> {
        if !self.exists()? {
            return Ok(Vec::new());
        }
        self.access(|db| {
            let read = db.begin_read()?;
            let files = read.open_table(FILES)?;
            files
                .iter()?
                .map(|row| {
                    row.map(|(key, _)| PathBuf::from(key.value()))
                        .map_err(Into::into)
                })
                .collect()
        })
    }

    pub fn versions(&self, path: &Path) -> Result<Vec<Version>> {
        let path = supported(path)?;
        if !self.exists()? {
            return Ok(Vec::new());
        }
        self.access(|db| {
            let read = db.begin_read()?;
            let table = read.open_table(VERSIONS)?;
            table
                .range((path, 0)..=(path, u64::MAX))?
                .rev()
                .map(|row| {
                    let (key, value) = row?;
                    let (timestamp_ms, bytes) = value.value();
                    Ok(Version {
                        id: key.value().1,
                        timestamp_ms,
                        bytes,
                    })
                })
                .collect()
        })
    }

    pub fn content(&self, path: &Path, id: u64) -> Result<String> {
        let path = supported(path)?;
        if !self.exists()? {
            return Err("This snapshot no longer exists.".into());
        }
        self.access(|db| {
            let read = db.begin_read()?;
            let table = read.open_table(CONTENT)?;
            let text = table
                .get_owned((path, id))?
                .ok_or("This snapshot no longer exists.")?;
            Ok(text.value().to_owned())
        })
    }

    pub fn record(
        &self,
        path: &Path,
        text: &str,
        now_ms: i64,
        config: &Config,
        force: bool,
    ) -> Result<bool> {
        let path = supported(path)?;
        config.validate()?;
        self.access(|db| {
            let write = db.begin_write()?;
            let captured = {
                let mut files = write.open_table(FILES)?;
                let mut versions = write.open_table(VERSIONS)?;
                let mut contents = write.open_table(CONTENT)?;
                let mut counters = write.open_table(COUNTERS)?;
                let expired: Vec<_> = versions
                    .range((path, 0)..=(path, u64::MAX))?
                    .filter_map(|row| match row {
                        Ok((key, value)) if value.value().0 < config.cutoff(now_ms) => {
                            Some(Ok(key.value().1))
                        }
                        Ok(_) => None,
                        Err(error) => Some(Err(error)),
                    })
                    .collect::<std::result::Result<_, _>>()?;
                for id in expired {
                    versions.remove((path, id))?;
                    contents.remove((path, id))?;
                }
                let latest = versions
                    .range((path, 0)..=(path, u64::MAX))?
                    .next_back()
                    .transpose()?
                    .map(|(key, value)| (key.value().1, value.value().0));
                let should_capture = if let Some((id, timestamp)) = latest {
                    let previous = contents
                        .get((path, id))?
                        .ok_or("A saved snapshot's content is missing.")?;
                    previous.value() != text
                        && (force
                            || now_ms < timestamp
                            || now_ms.saturating_sub(timestamp)
                                >= (config.interval_minutes * 60_000.0) as i64)
                } else {
                    true
                };
                if should_capture {
                    let id = counters
                        .get("last-id")?
                        .map(|value| value.value())
                        .unwrap_or(0)
                        .checked_add(1)
                        .ok_or("Snapshot identifiers are exhausted.")?;
                    counters.insert("last-id", id)?;
                    versions.insert((path, id), (now_ms, text.len() as u64))?;
                    contents.insert((path, id), text)?;
                    files.insert(path, id)?;
                } else if let Some((id, _)) = latest {
                    files.insert(path, id)?;
                } else {
                    files.remove(path)?;
                }
                should_capture
            };
            write.commit()?;
            Ok(captured)
        })
    }

    /// Restore through the ordinary conflict-aware atomic save path. A failure
    /// to preserve the displaced version also prevents the write.
    pub fn restore(
        &self,
        vault: &mut crate::vault::Vault,
        expected: &crate::vault::Document,
        version: u64,
        now_ms: i64,
        config: &Config,
    ) -> Result<crate::vault::Document> {
        if expected.dirty() && (expected.saved_text().is_some() || !expected.text.is_empty()) {
            return Err("Save or explicitly discard the current edits before restoring.".into());
        }
        let text = self.content(&expected.path, version)?;
        if let Some(text) = expected.saved_text() {
            self.record(&expected.path, text, now_ms, config, true)?;
        }
        let mut restored = expected.clone();
        restored.text = text;
        vault.save(&mut restored)?;
        Ok(restored)
    }

    pub fn prune(&self, now_ms: i64, config: &Config) -> Result<()> {
        config.validate()?;
        if !self.exists()? {
            return Ok(());
        }
        self.access(|db| {
            let write = db.begin_write()?;
            prune_transaction(&write, config.cutoff(now_ms))?;
            write.commit()?;
            db.compact()?;
            Ok(())
        })
    }

    pub fn clear(&self) -> Result<()> {
        if !self.exists()? {
            return Ok(());
        }
        self.access(|db| {
            let write = db.begin_write()?;
            write.open_table(FILES)?.retain(|_, _| false)?;
            write.open_table(VERSIONS)?.retain(|_, _| false)?;
            write.open_table(CONTENT)?.retain(|_, _| false)?;
            // Keep the counter, so stale version selections cannot name new data.
            write.commit()?;
            db.compact()?;
            Ok(())
        })
    }
}

fn prune_transaction(write: &WriteTransaction, cutoff: i64) -> StorageResult<()> {
    let mut versions = write.open_table(VERSIONS)?;
    let mut contents = write.open_table(CONTENT)?;
    let mut files = write.open_table(FILES)?;
    let expired: Vec<_> = versions
        .iter()?
        .filter_map(|row| match row {
            Ok((key, value)) if value.value().0 < cutoff => {
                Some(Ok((key.value().0.to_owned(), key.value().1)))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<std::result::Result<_, _>>()?;
    for (path, id) in expired {
        versions.remove((path.as_str(), id))?;
        contents.remove((path.as_str(), id))?;
    }
    let paths: Vec<_> = files
        .iter()?
        .map(|row| row.map(|(key, _)| key.value().to_owned()))
        .collect::<std::result::Result<_, _>>()?;
    for path in paths {
        let latest = versions
            .range((path.as_str(), 0)..=(path.as_str(), u64::MAX))?
            .next_back()
            .transpose()?
            .map(|(key, _)| key.value().1);
        if let Some(id) = latest {
            files.insert(path.as_str(), id)?;
        } else {
            files.remove(path.as_str())?;
        }
    }
    Ok(())
}

fn supported(path: &Path) -> Result<&str> {
    let valid = !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
        && path.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("canvas")
        });
    if !valid {
        return Err("Choose a Markdown or Canvas file inside the vault.".into());
    }
    path.to_str()
        .ok_or_else(|| "This file name cannot be stored in local history.".into())
}
