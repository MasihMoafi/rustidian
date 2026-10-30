use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    time::{Duration, Instant},
};
use tempfile::NamedTempFile;
use walkdir::WalkDir;

pub type Result<T> = std::result::Result<T, String>;

/// Navigation and watcher refreshes share the same development-clutter policy.
pub fn is_ignored_path(relative: &Path) -> bool {
    relative.components().any(|part| {
        let Component::Normal(name) = part else {
            return false;
        };
        name.as_encoded_bytes().first() == Some(&b'.')
            || matches!(
                name.to_str(),
                Some("node_modules" | "__pycache__" | "target" | "vendor" | "dist" | "build")
            )
    })
}

#[derive(Clone, Debug)]
pub struct Note {
    pub path: PathBuf,
    pub text: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    folded: std::sync::OnceLock<String>,
}

#[derive(Clone, Debug)]
pub struct LinkedMention {
    pub path: PathBuf,
    pub range: std::ops::Range<usize>,
    pub line: usize,
    pub before: String,
    pub link: String,
    pub after: String,
}

impl Note {
    pub(crate) fn new(path: PathBuf, text: String) -> Self {
        let metadata = crate::metadata::analyze(&text);
        Self {
            path,
            text,
            aliases: metadata.aliases,
            tags: metadata.tags,
            folded: std::sync::OnceLock::new(),
        }
    }
    pub fn folded_text(&self) -> &str {
        self.folded.get_or_init(|| self.text.to_lowercase())
    }
    pub fn title(&self) -> String {
        file_title(&self.path)
    }
}

#[derive(Clone, Debug)]
pub struct Document {
    pub path: PathBuf,
    pub text: String,
    original: Option<String>,
    saved_revision: u64,
    kind: DocumentKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocumentKind {
    Note,
    Image,
}

impl Document {
    fn next_revision() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    /// Changes only when saved bytes are loaded or successfully committed.
    pub fn saved_revision(&self) -> u64 {
        self.saved_revision
    }

    pub fn saved_text(&self) -> Option<&str> {
        self.original.as_deref()
    }

    pub fn blank() -> Self {
        Self {
            path: PathBuf::new(),
            text: String::new(),
            original: Some(String::new()),
            saved_revision: Self::next_revision(),
            kind: DocumentKind::Note,
        }
    }

    pub fn is_blank(&self) -> bool {
        self.path.as_os_str().is_empty()
    }

    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            text: String::new(),
            original: None,
            saved_revision: Self::next_revision(),
            kind: DocumentKind::Note,
        }
    }

    pub fn dirty(&self) -> bool {
        !self.is_image() && self.original.as_deref() != Some(&self.text)
    }
    pub fn is_image(&self) -> bool {
        self.kind == DocumentKind::Image
    }

    pub fn is_plain_text(&self) -> bool {
        !self.is_blank() && !self.is_image() && !is_markdown(&self.path)
    }

    pub fn title(&self) -> String {
        if self.is_blank() {
            "New tab".into()
        } else {
            file_title(&self.path)
        }
    }
}

#[derive(Clone)]
pub struct Vault {
    pub root: PathBuf,
    pub notes: Vec<Note>,
    pub warnings: Vec<String>,
    pub index_time: Duration,
    pub attachments: Vec<PathBuf>,
    pub folders: Vec<PathBuf>,
    paths: HashMap<PathBuf, usize>,
    names: HashMap<String, Vec<usize>>,
    incoming: std::sync::OnceLock<HashMap<PathBuf, Vec<usize>>>,
}

impl Vault {
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_cancellable(root, || false)
    }

    pub fn open_cancellable(root: &Path, cancelled: impl Fn() -> bool) -> Result<Self> {
        let root = root
            .canonicalize()
            .map_err(|e| format!("Cannot open vault: {e}"))?;
        if !root.is_dir() {
            return Err("Choose a folder for your vault.".into());
        }
        let mut vault = Self {
            root,
            notes: Vec::new(),
            warnings: Vec::new(),
            index_time: Duration::ZERO,
            attachments: Vec::new(),
            folders: Vec::new(),
            paths: HashMap::new(),
            names: HashMap::new(),
            incoming: std::sync::OnceLock::new(),
        };
        vault.refresh_cancellable(cancelled)?;
        Ok(vault)
    }

    pub fn name(&self) -> String {
        self.root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    fn refresh_cancellable(&mut self, cancelled: impl Fn() -> bool) -> Result<()> {
        if !self.root.is_dir() {
            return Err("The vault folder is no longer available.".into());
        }
        let start = Instant::now();
        let mut notes = Vec::new();
        let mut attachments = Vec::new();
        let mut folders = Vec::new();
        let mut warnings = Vec::new();
        let entries = WalkDir::new(&self.root)
            .follow_links(false)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0 || !is_ignored_path(Path::new(entry.file_name()))
            });
        for entry in entries {
            if cancelled() {
                return Err("Opening cancelled.".into());
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    warnings.push(error.to_string());
                    continue;
                }
            };
            if entry.file_type().is_dir() && entry.depth() > 0 {
                folders.push(entry.path().strip_prefix(&self.root).unwrap().into());
            }
            if !entry.file_type().is_file() {
                continue;
            }
            if !is_markdown(entry.path()) {
                if is_text_document(entry.path()) || crate::assets::is_image(entry.path()) {
                    attachments.push(entry.path().strip_prefix(&self.root).unwrap().into());
                }
                continue;
            }
            match fs::read_to_string(entry.path()) {
                Ok(text) => notes.push(Note::new(
                    entry.path().strip_prefix(&self.root).unwrap().into(),
                    text,
                )),
                Err(error) => warnings.push(format!("{}: {error}", entry.path().display())),
            }
        }
        notes.sort_by_key(|note| note.path.to_string_lossy().to_lowercase());
        self.notes = notes;
        self.attachments = attachments;
        folders.sort();
        self.folders = folders;
        self.rebuild_lookup();
        self.warnings = warnings;
        self.index_time = start.elapsed();
        Ok(())
    }

    pub(crate) fn safe_path(&self, relative: &Path, create_parents: bool) -> Result<PathBuf> {
        if !is_text_document(relative) {
            return Err("Choose a supported Markdown, text or source-code file.".into());
        }
        self.safe_file_path(relative, create_parents)
    }

    pub(crate) fn safe_file_path(&self, relative: &Path, create_parents: bool) -> Result<PathBuf> {
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("Notes must stay inside the vault. Use a relative Markdown path.".into());
        }
        let mut parent = self.root.clone();
        if let Some(parents) = relative.parent() {
            for part in parents.components() {
                parent.push(part);
                match fs::symlink_metadata(&parent) {
                    Ok(metadata) if metadata.file_type().is_symlink() => {
                        return Err("Linked folders cannot be edited.".into());
                    }
                    Ok(metadata) if !metadata.is_dir() => {
                        return Err("A parent path is not a folder.".into());
                    }
                    Ok(_) => {}
                    Err(error)
                        if error.kind() == std::io::ErrorKind::NotFound && !create_parents =>
                    {
                        parent.pop();
                        break;
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::NotFound && create_parents =>
                    {
                        fs::create_dir(&parent)
                            .map_err(|e| format!("Cannot create folder: {e}"))?;
                    }
                    Err(error) => return Err(format!("Cannot access folder: {error}")),
                }
            }
        }
        if !parent
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&self.root)
        {
            return Err("Path is outside this vault.".into());
        }
        let absolute = self.root.join(relative);
        if let Ok(metadata) = fs::symlink_metadata(&absolute)
            && metadata.file_type().is_symlink()
        {
            return Err("Linked files cannot be edited.".into());
        }
        Ok(absolute)
    }

    pub fn read(&self, path: &Path) -> Result<Document> {
        let absolute = self.safe_path(path, false)?;
        let text = fs::read_to_string(absolute).map_err(|e| format!("Cannot read note: {e}"))?;
        if text.contains('\0') {
            return Err("This file contains binary data and cannot be edited as text.".into());
        }
        Ok(Document {
            path: path.into(),
            original: Some(text.clone()),
            saved_revision: Document::next_revision(),
            text,
            kind: DocumentKind::Note,
        })
    }

    pub fn open_document(&self, path: &Path) -> Result<Document> {
        if is_text_document(path) {
            return self.read(path);
        }
        if !self.attachments.iter().any(|attachment| attachment == path) {
            return Err("Choose an indexed note or image inside this vault.".into());
        }
        let absolute = self.safe_file_path(path, false)?;
        if !absolute.is_file() {
            return Err("This image no longer exists.".into());
        }
        Ok(Document {
            path: path.to_owned(),
            text: String::new(),
            original: Some(String::new()),
            saved_revision: Document::next_revision(),
            kind: DocumentKind::Image,
        })
    }

    /// Create an empty note without asking for a name or replacing an existing file.
    pub fn create_untitled(&mut self) -> Result<Document> {
        self.create_untitled_in(Path::new(""))
    }

    pub fn create_untitled_in(&mut self, folder: &Path) -> Result<Document> {
        let absolute = if folder.as_os_str().is_empty() {
            self.root.clone()
        } else {
            self.folder_path(folder)?
        };
        let metadata = fs::symlink_metadata(&absolute)
            .map_err(|error| format!("Cannot create a note in this folder: {error}"))?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("Choose an existing folder inside this vault.".into());
        }
        for number in 0..u64::MAX {
            let name = if number == 0 {
                "Untitled.md".to_owned()
            } else {
                format!("Untitled {number}.md")
            };
            let path = folder.join(name);
            let absolute = self.root.join(&path);
            match fs::symlink_metadata(&absolute) {
                Ok(_) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            let mut document = Document::new(path);
            match self.save(&mut document) {
                Ok(()) => return Ok(document),
                // Another window may have created this name. save uses noclobber.
                Err(_) if fs::symlink_metadata(&absolute).is_ok() => continue,
                Err(error) => return Err(error),
            }
        }
        Err("Cannot find an unused note name.".into())
    }

    pub fn save(&mut self, document: &mut Document) -> Result<()> {
        if document.is_image() {
            return Err("Images cannot be edited as Markdown.".into());
        }
        if document.original.is_some() {
            let pending = self.prepare_save(document)?;
            pending.write()?;
            self.commit_save(document, pending.text);
            return Ok(());
        }
        let absolute = self.safe_path(&document.path, true)?;
        if absolute.exists() {
            return Err("A note already exists at this path. Choose a different name.".into());
        }
        prepare_file(&absolute, document.text.as_bytes(), None)?
            .persist_noclobber(&absolute)
            .map_err(|e| format!("Cannot create note: {}", e.error))?;
        self.commit_save(document, document.text.clone());
        Ok(())
    }

    /// The checks and bytes for saving a note that is already on disk. Its
    /// `write` borrows nothing from the vault, so autosave runs it off the UI thread.
    pub fn prepare_save(&self, document: &Document) -> Result<PendingSave> {
        if document.is_image() {
            return Err("Images cannot be edited as Markdown.".into());
        }
        let original = document
            .original
            .clone()
            .ok_or("This note is not on disk yet.")?;
        Ok(PendingSave {
            absolute: self.safe_path(&document.path, false)?,
            path: document.path.clone(),
            text: document.text.clone(),
            original,
        })
    }

    /// Record that `text`, written by `save` or `PendingSave::write`, is now the
    /// document's saved state.
    pub fn commit_save(&mut self, document: &mut Document, text: String) {
        document.original = Some(text.clone());
        document.saved_revision = Document::next_revision();
        self.index_note(document.path.clone(), text);
    }

    fn index_note(&mut self, path: PathBuf, text: String) {
        let legacy_text = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"));
        if !is_markdown(&path) && !legacy_text {
            if !self.attachments.contains(&path) {
                self.attachments.push(path);
                self.attachments.sort();
            }
            return;
        }
        self.attachments.retain(|attachment| attachment != &path);
        let note = Note::new(path.clone(), text);
        if let Some(existing) = self.notes.iter_mut().find(|note| note.path == path) {
            *existing = note;
        } else {
            self.notes.push(note);
            self.notes
                .sort_by_key(|note| note.path.to_string_lossy().to_lowercase());
        }
        self.rebuild_lookup();
    }

    pub fn duplicate_file(&mut self, from: &Path) -> Result<PathBuf> {
        if is_ignored_path(from) || !(is_text_document(from) || crate::assets::is_image(from)) {
            return Err("Choose a visible note or supported image inside this vault.".into());
        }
        let source = self.safe_file_path(from, false)?;
        let bytes =
            fs::read(&source).map_err(|error| format!("Cannot read the file to copy: {error}"))?;
        let text = is_text_document(from)
            .then(|| {
                std::str::from_utf8(&bytes)
                    .map(str::to_owned)
                    .map_err(|error| error.to_string())
            })
            .transpose()?;
        let stem = from
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("Invalid file name.")?;
        let extension = from
            .extension()
            .and_then(|value| value.to_str())
            .ok_or("Add a filename extension before copying this file.")?;
        let parent = from.parent().unwrap_or(Path::new(""));
        for counter in 1_u64.. {
            let path = parent.join(format!("{stem} {counter}.{extension}"));
            match fs::symlink_metadata(self.root.join(&path)) {
                Ok(_) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            let absolute = self.safe_file_path(&path, false)?;
            let prepared = prepare_file(&absolute, &bytes, None)?;
            match prepared.persist_noclobber(&absolute) {
                Ok(_) => {
                    if let Some(text) = text {
                        self.index_note(path.clone(), text);
                    } else {
                        self.attachments.push(path.clone());
                        self.attachments.sort();
                    }
                    return Ok(path);
                }
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(format!("Cannot create the copy: {}", error.error)),
            }
        }
        Err("Cannot find an available copy name.".into())
    }

    pub(crate) fn rebuild_lookup(&mut self) {
        self.paths.clear();
        self.names.clear();
        for (index, note) in self.notes.iter().enumerate() {
            self.paths.insert(note.path.clone(), index);
            if let Some(name) = note.path.file_name().and_then(|name| name.to_str()) {
                self.names.entry(name.into()).or_default().push(index);
            }
        }
        self.incoming.take();
    }

    /// Save a new image without replacing an existing vault entry.
    pub fn import_attachment(&mut self, name: &Path, bytes: &[u8]) -> Result<PathBuf> {
        if name.components().count() != 1
            || !matches!(name.components().next(), Some(Component::Normal(_)))
            || is_ignored_path(name)
            || !crate::assets::is_image(name)
        {
            return Err("Choose a supported image file.".into());
        }
        let stem = name
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or("Invalid image name.")?;
        let extension = name
            .extension()
            .and_then(|s| s.to_str())
            .ok_or("Invalid image extension.")?;
        for number in 0.. {
            let relative = if number == 0 {
                name.to_path_buf()
            } else {
                PathBuf::from(format!("{stem} {number}.{extension}"))
            };
            let absolute = self.root.join(&relative);
            match fs::symlink_metadata(&absolute) {
                Ok(_) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(format!("Cannot access attachment: {error}")),
            }
            let absolute = self.safe_file_path(&relative, false)?;
            let temporary = prepare_file(&absolute, bytes, None)?;
            match temporary.persist_noclobber(&absolute) {
                Ok(_) => {
                    self.index_note(relative.clone(), String::new());
                    return Ok(relative);
                }
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(format!("Cannot save attachment: {error}")),
            }
        }
        Err("Cannot find an available image name.".into())
    }

    pub fn query(&self, query: &str) -> Result<Vec<&Note>> {
        let query = crate::query::Query::parse(query)?;
        Ok(self
            .notes
            .iter()
            .filter(|note| query.matches(note))
            .collect())
    }

    pub fn resolve(&self, from: &Path, target: &str) -> Option<PathBuf> {
        let target = target.split('|').next()?.split('#').next()?.trim();
        if target.is_empty() {
            return Some(from.to_owned());
        }
        self.resolve_path(from, target).or_else(|| {
            crate::paths::decode(target)
                .filter(|decoded| decoded != target)
                .and_then(|decoded| self.resolve_path(from, &decoded))
        })
    }

    fn resolve_path(&self, from: &Path, target: &str) -> Option<PathBuf> {
        let target = if is_text_document(Path::new(target)) {
            PathBuf::from(target)
        } else {
            crate::paths::markdown_name(Path::new(target))
        };
        let exists = |path: &Path| {
            self.paths.contains_key(path)
                || (!is_markdown(path)
                    && is_text_document(path)
                    && self.safe_path(path, false).is_ok_and(|path| path.is_file()))
        };
        let sibling =
            crate::paths::normalise(&from.parent().unwrap_or(Path::new("")).join(&target));
        if let Some(sibling) = sibling
            && exists(&sibling)
        {
            return Some(sibling);
        }
        if let Some(root) = crate::paths::normalise(&target)
            && exists(&root)
        {
            return Some(root);
        }
        if target.components().count() != 1 {
            return None;
        }
        let matching = self.names.get(target.file_name()?.to_str()?)?;
        (matching.len() == 1).then(|| self.notes[matching[0]].path.clone())
    }

    /// Existing attachments and files outside the vault belong to the desktop file handler.
    /// Resolving them must not create notes, change the vault, or follow a symlink escape.
    pub fn external_file_uri(&self, from: &Path, target: &str) -> Option<String> {
        let target = crate::paths::decode(target.split('|').next()?.split('#').next()?.trim())?;
        let candidate = self
            .root
            .join(from.parent().unwrap_or(Path::new("")))
            .join(target);
        let mut lexical = PathBuf::new();
        for part in candidate.components() {
            match part {
                Component::CurDir => {}
                Component::ParentDir => {
                    if !lexical.pop() {
                        return None;
                    }
                }
                part => lexical.push(part.as_os_str()),
            }
        }
        let file = lexical.canonicalize().ok()?;
        if file != lexical || !file.is_file() {
            return None;
        }
        if file.starts_with(&self.root)
            && (is_text_document(&file) || crate::assets::is_image(&file))
        {
            return None;
        }
        Some(format!(
            "file://{}",
            crate::paths::markdown_destination(Path::new(""), &file)
        ))
    }

    pub fn open_link(&mut self, from: &Path, target: &str) -> Result<Document> {
        if let Some(path) = self.resolve_destination(from, target) {
            return self.open_document(&path);
        }
        let target = target
            .split('|')
            .next()
            .unwrap_or("")
            .split('#')
            .next()
            .unwrap_or("")
            .trim();
        let mut path = PathBuf::from(target);
        if target.is_empty()
            || !path
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
        {
            return Err("Use a note name inside this vault.".into());
        }
        if !is_markdown(&path) {
            let decoded = crate::paths::decode(target).unwrap_or_else(|| target.to_owned());
            if crate::assets::is_image(Path::new(&decoded)) {
                return Err("This image does not exist in the vault.".into());
            }
            if self.root.join(&path).is_file() {
                return Err(
                    "This link points to an attachment rather than a Markdown note.".into(),
                );
            }
            let mut name = path.into_os_string();
            name.push(".md");
            path = PathBuf::from(name);
        }
        if self
            .notes
            .iter()
            .filter(|note| note.path.file_name() == path.file_name())
            .count()
            > 1
        {
            return Err("Several notes share this name. Use the full vault-relative path.".into());
        }
        let mut document = Document::new(path);
        document.text = format!("# {}\n\n", document.title());
        self.save(&mut document)?;
        Ok(document)
    }

    /// Each actual linked mention, using open drafts instead of their saved copies.
    pub fn backlink_mentions(&self, path: &Path, open: &[(&Path, &str)]) -> Vec<LinkedMention> {
        let mut sources: HashMap<PathBuf, &str> = self
            .backlinks(path)
            .into_iter()
            .map(|note| (note.path.clone(), note.text.as_str()))
            .collect();
        for &(from, text) in open {
            if from != path && !from.as_os_str().is_empty() {
                sources.insert(from.to_owned(), text);
            }
        }
        let mut mentions = Vec::new();
        for (from, text) in sources {
            if from == path {
                continue;
            }
            for (range, target) in reference_spans(text) {
                if self.resolve_destination(&from, &target).as_deref() != Some(path) {
                    continue;
                }
                let line_start = text[..range.start].rfind('\n').map_or(0, |start| start + 1);
                let line_end = text[range.end..]
                    .find('\n')
                    .map_or(text.len(), |end| range.end + end);
                let prefix = &text[line_start..range.start];
                let suffix = text[range.end..line_end].trim_end_matches('\r');
                let mut before: String = prefix
                    .chars()
                    .rev()
                    .take(50)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                if prefix.chars().count() > 50 {
                    before.insert(0, '…');
                }
                let mut after: String = suffix.chars().take(70).collect();
                if suffix.chars().count() > 70 {
                    after.push('…');
                }
                mentions.push(LinkedMention {
                    path: from.clone(),
                    line: text[..range.start]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count()
                        + 1,
                    link: text[range.clone()].into(),
                    range,
                    before,
                    after,
                });
            }
        }
        mentions.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then_with(|| a.range.start.cmp(&b.range.start))
        });
        mentions
    }

    pub fn backlinks(&self, path: &Path) -> Vec<&Note> {
        let incoming = self.incoming.get_or_init(|| {
            let mut incoming: HashMap<PathBuf, Vec<usize>> = HashMap::new();
            for (index, note) in self.notes.iter().enumerate() {
                let mut targets = std::collections::HashSet::new();
                for (_, link) in reference_spans(&note.text) {
                    if let Some(target) = self.resolve_destination(&note.path, &link)
                        && target != note.path
                    {
                        targets.insert(target);
                    }
                }
                for target in targets {
                    incoming.entry(target).or_default().push(index);
                }
            }
            incoming
        });
        incoming
            .get(path)
            .into_iter()
            .flatten()
            .map(|&index| &self.notes[index])
            .collect()
    }
}

/// A save of an existing note: `Vault::prepare_save`, `write`, then `Vault::commit_save`.
pub struct PendingSave {
    pub path: PathBuf,
    pub text: String,
    absolute: PathBuf,
    original: String,
}

impl PendingSave {
    /// The saved text the write expects to replace.
    pub fn original(&self) -> &str {
        &self.original
    }

    /// Atomically replaces the note, keeping its permissions, unless it no longer
    /// holds the text it was loaded or last saved with.
    pub fn write(&self) -> Result<()> {
        let current = fs::read_to_string(&self.absolute).map_err(|e| {
            format!("Note was removed or cannot be read. Your changes are still open: {e}")
        })?;
        if current != self.original {
            return Err("This note changed outside Rustidian. Save your edits to a new note, or keep them here while reviewing the disk version. Nothing was overwritten.".into());
        }
        let permissions = fs::metadata(&self.absolute)
            .map_err(|e| e.to_string())?
            .permissions();
        let temporary = prepare_file(&self.absolute, self.text.as_bytes(), Some(permissions))?;
        // Recheck after writing the temporary file to catch changes during a long write.
        if fs::read_to_string(&self.absolute).ok().as_ref() != Some(&self.original) {
            return Err(
                "This note changed outside Rustidian during save. Nothing was overwritten.".into(),
            );
        }
        temporary
            .persist(&self.absolute)
            .map_err(|e| format!("Cannot finish save: {}", e.error))?;
        Ok(())
    }
}

fn prepare_file(
    absolute: &Path,
    bytes: &[u8],
    permissions: Option<fs::Permissions>,
) -> Result<NamedTempFile> {
    let parent = absolute.parent().ok_or("Invalid note path")?;
    let mut temporary =
        NamedTempFile::new_in(parent).map_err(|e| format!("Cannot prepare save: {e}"))?;
    temporary
        .write_all(bytes)
        .map_err(|e| format!("Cannot write note: {e}"))?;
    if let Some(permissions) = permissions {
        temporary
            .as_file()
            .set_permissions(permissions)
            .map_err(|e| e.to_string())?;
    }
    temporary
        .as_file()
        .sync_all()
        .map_err(|e| format!("Cannot flush note: {e}"))?;
    Ok(temporary)
}

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

pub fn is_text_document(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|name| name.to_str())
        && [
            "makefile",
            "dockerfile",
            "cmakelists.txt",
            "justfile",
            "license",
        ]
        .iter()
        .any(|candidate| name.eq_ignore_ascii_case(candidate))
    {
        return true;
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "md" | "txt"
                    | "rs"
                    | "py"
                    | "pyw"
                    | "js"
                    | "mjs"
                    | "cjs"
                    | "jsx"
                    | "ts"
                    | "tsx"
                    | "c"
                    | "h"
                    | "cpp"
                    | "cc"
                    | "cxx"
                    | "hpp"
                    | "hxx"
                    | "cs"
                    | "java"
                    | "kt"
                    | "kts"
                    | "go"
                    | "rb"
                    | "php"
                    | "swift"
                    | "m"
                    | "mm"
                    | "scala"
                    | "clj"
                    | "cljs"
                    | "ex"
                    | "exs"
                    | "erl"
                    | "hrl"
                    | "hs"
                    | "lua"
                    | "r"
                    | "jl"
                    | "pl"
                    | "pm"
                    | "ps1"
                    | "psm1"
                    | "sh"
                    | "bash"
                    | "zsh"
                    | "fish"
                    | "bat"
                    | "cmd"
                    | "html"
                    | "htm"
                    | "css"
                    | "scss"
                    | "sass"
                    | "less"
                    | "xml"
                    | "xsl"
                    | "xsd"
                    | "json"
                    | "jsonc"
                    | "toml"
                    | "yaml"
                    | "yml"
                    | "ini"
                    | "cfg"
                    | "conf"
                    | "properties"
                    | "env"
                    | "sql"
                    | "graphql"
                    | "gql"
                    | "proto"
                    | "tex"
                    | "bib"
                    | "rst"
                    | "adoc"
                    | "log"
                    | "csv"
                    | "tsv"
                    | "nix"
                    | "zig"
                    | "v"
                    | "sv"
                    | "vhd"
                    | "vhdl"
                    | "svelte"
                    | "vue"
                    | "elm"
                    | "dart"
                    | "ipynb"
            )
        })
}

pub fn file_title(path: &Path) -> String {
    (if is_markdown(path) {
        path.file_stem()
    } else {
        path.file_name()
    })
    .unwrap_or_default()
    .to_string_lossy()
    .into_owned()
}

fn prose_lines(text: &str) -> impl Iterator<Item = &str> {
    let mut fence: Option<(char, usize)> = None;
    text.lines().filter(move |line| {
        let trimmed = line.trim_start();
        let marker = trimmed.chars().next().unwrap_or(' ');
        let count = trimmed.chars().take_while(|c| *c == marker).count();
        if matches!(marker, '`' | '~') && count >= 3 {
            match fence {
                None => fence = Some((marker, count)),
                Some((open, width)) if marker == open && count >= width => fence = None,
                _ => {}
            }
            return false;
        }
        fence.is_none()
    })
}

fn reference_spans(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    use pulldown_cmark::{Event, Options, Parser, Tag};
    let maths = crate::math::spans(text);
    let mut links = wiki_spans_outside(text, &maths);
    let wiki_ranges: Vec<_> = links.iter().map(|(range, _)| range.clone()).collect();
    let body = crate::metadata::frontmatter(text)
        .ok()
        .flatten()
        .map_or(0, |properties| properties.range.end);
    for (event, range) in Parser::new_ext(
        &text[body..],
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_FOOTNOTES,
    )
    .into_offset_iter()
    {
        if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) = event {
            let range = body + range.start..body + range.end;
            if maths
                .iter()
                .any(|math| math.range.start < range.end && math.range.end > range.start)
                || wiki_ranges
                    .iter()
                    .any(|wiki| wiki.start < range.end && wiki.end > range.start)
            {
                continue;
            }
            links.push((range, dest_url.into_string()));
        }
    }
    links.sort_by_key(|(range, _)| (range.start, range.end));
    links
}

pub fn wiki_spans(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    wiki_spans_outside(text, &crate::math::spans(text))
}

/// `wiki_spans` for a caller that already parsed `text`'s math.
pub fn wiki_spans_outside(
    text: &str,
    maths: &[crate::math::Span],
) -> Vec<(std::ops::Range<usize>, String)> {
    let mut links = Vec::new();
    for line in prose_lines(text) {
        let offset = line.as_ptr() as usize - text.as_ptr() as usize;
        let bytes = line.as_bytes();
        let mut cursor = 0;
        let mut code_width = 0;
        while cursor < bytes.len() {
            if bytes[cursor] == b'\\' && code_width == 0 {
                cursor += 2;
                continue;
            }
            if bytes[cursor] == b'`' {
                let width = bytes[cursor..].iter().take_while(|b| **b == b'`').count();
                if code_width == width {
                    code_width = 0;
                } else if code_width == 0 && line[cursor + width..].contains(&"`".repeat(width)) {
                    code_width = width;
                }
                cursor += width;
                continue;
            }
            if code_width == 0
                && bytes[cursor] == b'['
                && line[cursor..].starts_with("[[")
                && let Some(end) = line[cursor + 2..].find("]]")
            {
                let end = cursor + 2 + end;
                let target = line[cursor + 2..end].trim();
                if !target.is_empty()
                    && !maths.iter().any(|math| {
                        math.range.start < offset + end + 2 && math.range.end > offset + cursor
                    })
                {
                    // Markdown tables escape the alias separator: [[Note\|Label]].
                    links.push((
                        offset + cursor..offset + end + 2,
                        target.replace("\\|", "|"),
                    ));
                }
                cursor = end + 2;
                continue;
            }
            cursor += 1;
        }
    }
    links
}

pub fn wiki_links(text: &str) -> Vec<String> {
    wiki_spans(text)
        .into_iter()
        .map(|(_, target)| target)
        .collect()
}
