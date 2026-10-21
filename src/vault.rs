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

#[derive(Clone, Debug)]
pub struct Note {
    pub path: PathBuf,
    pub text: String,
    pub aliases: Vec<String>,
    pub tags: Vec<String>,
    folded: std::sync::OnceLock<String>,
}

impl Note {
    fn new(path: PathBuf, text: String) -> Self {
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
    kind: DocumentKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DocumentKind {
    Note,
    Image,
}

impl Document {
    pub fn blank() -> Self {
        Self {
            path: PathBuf::new(),
            text: String::new(),
            original: Some(String::new()),
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
        self.path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
    }

    pub fn title(&self) -> String {
        if self.is_blank() {
            "New tab".into()
        } else {
            file_title(&self.path)
        }
    }
}

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
        vault.refresh()?;
        Ok(vault)
    }

    pub fn name(&self) -> String {
        self.root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    }

    pub fn refresh(&mut self) -> Result<()> {
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
                entry.depth() == 0
                    || (!entry.file_name().to_string_lossy().starts_with('.')
                        && !matches!(
                            entry.file_name().to_str(),
                            Some("node_modules" | "target" | "vendor" | "dist" | "build")
                        ))
            });
        for entry in entries {
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
                if crate::assets::is_image(entry.path()) {
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
            return Err("Documents must use the .md or .txt extension.".into());
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
        Ok(Document {
            path: path.into(),
            original: Some(text.clone()),
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
            kind: DocumentKind::Image,
        })
    }

    pub fn save(&mut self, document: &mut Document) -> Result<()> {
        if document.is_image() {
            return Err("Images cannot be edited as Markdown.".into());
        }
        let absolute = self.safe_path(&document.path, document.original.is_none())?;
        let permissions = if let Some(original) = &document.original {
            let current = fs::read_to_string(&absolute).map_err(|e| {
                format!("Note was removed or cannot be read. Your changes are still open: {e}")
            })?;
            if &current != original {
                return Err("This note changed outside Rust Notes. Save your edits to a new note, or keep them here while reviewing the disk version. Nothing was overwritten.".into());
            }
            Some(
                fs::metadata(&absolute)
                    .map_err(|e| e.to_string())?
                    .permissions(),
            )
        } else {
            if absolute.exists() {
                return Err("A note already exists at this path. Choose a different name.".into());
            }
            None
        };
        let parent = absolute.parent().ok_or("Invalid note path")?;
        let mut temporary =
            NamedTempFile::new_in(parent).map_err(|e| format!("Cannot prepare save: {e}"))?;
        temporary
            .write_all(document.text.as_bytes())
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
        if document.original.is_none() {
            temporary
                .persist_noclobber(&absolute)
                .map_err(|e| format!("Cannot create note: {}", e.error))?;
        } else {
            // Recheck after writing the temporary file to catch changes during a long write.
            if fs::read_to_string(&absolute).ok().as_ref() != document.original.as_ref() {
                return Err(
                    "This note changed outside Rust Notes during save. Nothing was overwritten."
                        .into(),
                );
            }
            temporary
                .persist(&absolute)
                .map_err(|e| format!("Cannot finish save: {}", e.error))?;
        }
        document.original = Some(document.text.clone());
        let note = Note::new(document.path.clone(), document.text.clone());
        if let Some(existing) = self
            .notes
            .iter_mut()
            .find(|note| note.path == document.path)
        {
            *existing = note;
        } else {
            self.notes.push(note);
            self.notes
                .sort_by_key(|note| note.path.to_string_lossy().to_lowercase());
        }
        self.rebuild_lookup();
        Ok(())
    }

    pub fn search(&self, query: &str) -> Vec<&Note> {
        self.query(query).unwrap_or_default()
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

    pub fn backlinks(&self, path: &Path) -> Vec<&Note> {
        let incoming = self.incoming.get_or_init(|| {
            let mut incoming: HashMap<PathBuf, Vec<usize>> = HashMap::new();
            for (index, note) in self.notes.iter().enumerate() {
                let mut targets = std::collections::HashSet::new();
                for link in wiki_links(&note.text) {
                    if let Some(target) = self.resolve_destination(&note.path, &link)
                        && target != note.path
                    {
                        targets.insert(target);
                    }
                }
                for event in pulldown_cmark::Parser::new(&note.text) {
                    if let pulldown_cmark::Event::Start(
                        pulldown_cmark::Tag::Link { dest_url, .. }
                        | pulldown_cmark::Tag::Image { dest_url, .. },
                    ) = event
                        && let Some(target) = self.resolve_destination(&note.path, &dest_url)
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

pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

pub fn is_text_document(path: &Path) -> bool {
    is_markdown(path)
        || path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("txt"))
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

pub fn wiki_spans(text: &str) -> Vec<(std::ops::Range<usize>, String)> {
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
                if !target.is_empty() {
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

pub fn headings(text: &str) -> Vec<(usize, String)> {
    crate::structure::outline(text)
        .into_iter()
        .map(|heading| (heading.level, heading.title))
        .collect()
}
