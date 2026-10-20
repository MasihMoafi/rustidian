//! Guarded note/folder moves and operating-system Trash integration.
use crate::{
    paths::{self, moved_path},
    vault::{Document, Result, Vault, wiki_spans},
};
use pulldown_cmark::{Event, Parser, Tag};
use std::path::{Path, PathBuf};

struct Rewrite {
    document: Document,
    before: String,
}
pub struct RenamePlan {
    from: PathBuf,
    to: PathBuf,
    source: Vec<Document>,
    kind: MoveKind,
    metadata: std::fs::Metadata,
    entries: Vec<(PathBuf, bool)>,
    rewrites: Vec<Rewrite>,
}

#[derive(PartialEq, Eq)]
enum MoveKind {
    Note,
    Image,
    Folder,
}
pub struct RenameOutcome {
    pub path: PathBuf,
    pub updated_notes: usize,
    pub warnings: Vec<String>,
}

impl Vault {
    pub fn create_folder(&mut self, relative: &Path) -> Result<()> {
        let absolute = self.folder_path(relative)?;
        std::fs::create_dir(&absolute).map_err(|error| format!("Cannot create folder: {error}"))?;
        self.folders.push(relative.to_owned());
        self.folders.sort();
        Ok(())
    }

    fn folder_path(&self, relative: &Path) -> Result<PathBuf> {
        if relative.as_os_str().is_empty()
            || relative.components().any(|part| match part {
                std::path::Component::Normal(name) => {
                    name.to_string_lossy().starts_with('.')
                        || matches!(
                            name.to_str(),
                            Some("node_modules" | "target" | "vendor" | "dist" | "build")
                        )
                }
                _ => true,
            })
        {
            return Err("Choose a visible folder path inside this vault.".into());
        }
        let mut parent = self.root.clone();
        if let Some(parents) = relative.parent() {
            for part in parents.components() {
                parent.push(part);
                let metadata = std::fs::symlink_metadata(&parent)
                    .map_err(|_| "Choose an existing parent folder.".to_owned())?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err("Choose an existing folder inside this vault.".into());
                }
            }
        }
        Ok(self.root.join(relative))
    }

    pub fn plan_rename(&self, from: &Path, to: &Path) -> Result<RenamePlan> {
        if from == to {
            return Err("Choose a different name.".into());
        }
        let folder = self.folders.iter().any(|path| path == from);
        let image = self.attachments.iter().any(|path| path == from);
        let kind = if folder {
            MoveKind::Folder
        } else if image {
            MoveKind::Image
        } else {
            MoveKind::Note
        };
        if image
            && !from
                .extension()
                .and_then(|value| value.to_str())
                .zip(to.extension().and_then(|value| value.to_str()))
                .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            return Err("Keep the image filename extension when renaming or moving it.".into());
        }
        if folder && to.starts_with(from) {
            return Err("A folder cannot move inside itself.".into());
        }
        let original = if folder {
            self.folder_path(from)?
        } else if image {
            self.safe_file_path(from, false)?
        } else {
            self.safe_path(from, false)?
        };
        let metadata = std::fs::symlink_metadata(&original)
            .map_err(|error| format!("Cannot read source: {error}"))?;
        if metadata.file_type().is_symlink()
            || (folder && !metadata.is_dir())
            || (image && !metadata.is_file())
        {
            return Err("Choose an existing file or folder inside this vault.".into());
        }
        let entries = if folder {
            folder_entries(&original)?
        } else {
            Vec::new()
        };
        let source = if folder {
            self.notes
                .iter()
                .filter(|note| note.path.starts_with(from))
                .map(|note| self.read(&note.path))
                .collect::<Result<Vec<_>>>()?
        } else if image {
            Vec::new()
        } else {
            vec![self.read(from)?]
        };
        let destination = if folder {
            self.folder_path(to)?
        } else if image {
            self.safe_file_path(to, false)?
        } else {
            self.safe_path(to, false)?
        };
        if destination
            .try_exists()
            .map_err(|error| format!("Cannot check destination: {error}"))?
        {
            return Err("A file or folder already exists at this destination.".into());
        }
        let mut rewrites = Vec::new();
        for note in &self.notes {
            let note_destination = moved_path(&note.path, from, to, folder);
            let mut changes = Vec::new();
            for (range, target) in wiki_spans(&note.text) {
                if let Some(resolved) = self.resolve_destination(&note.path, &target)
                    && (resolved == from
                        || (folder && resolved.starts_with(from))
                        || (note_destination != note.path
                            && self.resolve_destination(&note_destination, &target)
                                != Some(resolved.clone())))
                {
                    let name = target.split('|').next().unwrap();
                    let fragment = name.split_once('#').map(|(_, value)| value);
                    let destination = moved_path(&resolved, from, to, folder);
                    let mut link = paths::wiki_destination(&destination);
                    if !paths::wiki_path_supported(&destination) {
                        return Err("This destination would break wiki links. Choose another name without separators such as # or |.".into());
                    }
                    if let Some(fragment) = fragment {
                        link.push('#');
                        link.push_str(fragment);
                    }
                    let raw_target = &note.text[range.start + 2..range.end - 2];
                    if let Some((raw_name, alias)) = raw_target.split_once('|') {
                        if raw_name.ends_with('\\') {
                            link.push('\\');
                        }
                        link.push('|');
                        link.push_str(alias);
                    }
                    changes.push((range, format!("[[{link}]]")));
                }
            }
            let parser = Parser::new(&note.text);
            let mut markdown_links = Vec::new();
            for (_, definition) in parser.reference_definitions().iter() {
                let raw = &note.text[definition.span.clone()];
                if let Some(start) =
                    label_end(raw).filter(|end| raw.as_bytes().get(end + 1) == Some(&b':'))
                    && let Some(range) =
                        destination_range(&note.text, definition.span.clone(), start + 2)
                {
                    markdown_links.push((definition.dest.to_string(), range));
                }
            }
            for (event, range) in parser.into_offset_iter() {
                if let Event::Start(Tag::Link { dest_url, .. } | Tag::Image { dest_url, .. }) =
                    event
                {
                    let raw = &note.text[range.clone()];
                    if let Some(start) =
                        label_end(raw).filter(|end| raw.as_bytes().get(end + 1) == Some(&b'('))
                        && let Some(range) = destination_range(&note.text, range, start + 2)
                    {
                        markdown_links.push((dest_url.into_string(), range));
                    }
                }
            }
            for (url, range) in markdown_links {
                if let Some(resolved) = self.resolve_destination(&note.path, &url)
                    && (resolved == from
                        || (folder && resolved.starts_with(from))
                        || note_destination != note.path)
                {
                    let mut destination = paths::markdown_destination(
                        &note_destination,
                        &moved_path(&resolved, from, to, folder),
                    );
                    if let Some((_, fragment)) = url.split_once('#') {
                        destination.push('#');
                        destination.push_str(fragment);
                    }
                    changes.push((range, destination));
                }
            }
            if changes.is_empty() {
                continue;
            }
            let mut document = self.read(&note.path)?;
            if document.text != note.text {
                return Err(format!(
                    "{} changed outside the app. Refresh the vault before renaming.",
                    note.path.display()
                ));
            }
            let before = document.text.clone();
            changes.sort_by_key(|(range, _)| range.start);
            if changes
                .windows(2)
                .any(|pair| pair[0].0.end > pair[1].0.start)
            {
                return Err(format!(
                    "Overlapping links in {} need review before renaming.",
                    note.path.display()
                ));
            }
            for (range, replacement) in changes.into_iter().rev() {
                document.text.replace_range(range, &replacement);
            }
            rewrites.push(Rewrite { document, before });
        }
        Ok(RenamePlan {
            from: from.to_owned(),
            to: to.to_owned(),
            source,
            kind,
            metadata,
            entries,
            rewrites,
        })
    }

    pub fn rename_note(&mut self, plan: RenamePlan) -> Result<RenameOutcome> {
        let folder = plan.kind == MoveKind::Folder;
        let image = plan.kind == MoveKind::Image;
        let original = if folder {
            self.folder_path(&plan.from)?
        } else if image {
            self.safe_file_path(&plan.from, false)?
        } else {
            self.safe_path(&plan.from, false)?
        };
        let destination = if folder {
            self.folder_path(&plan.to)?
        } else if image {
            self.safe_file_path(&plan.to, true)?
        } else {
            self.safe_path(&plan.to, true)?
        };
        for source in &plan.source {
            if self.read(&source.path)?.text != source.text {
                return Err("The source changed outside the app. Nothing was renamed.".into());
            }
        }
        if folder && folder_entries(&original)? != plan.entries {
            return Err("The folder contents changed outside the app. Nothing was renamed.".into());
        }
        for rewrite in &plan.rewrites {
            if self.read(&rewrite.document.path)?.text != rewrite.before {
                return Err(format!(
                    "{} changed outside the app. Nothing was renamed.",
                    rewrite.document.path.display()
                ));
            }
        }
        #[cfg(target_os = "linux")]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::symlink_metadata(&original)
                .map_err(|error| format!("Cannot read source: {error}"))?;
            if metadata.dev() != plan.metadata.dev() || metadata.ino() != plan.metadata.ino() {
                return Err("The source was replaced outside the app. Nothing was renamed.".into());
            }
            if image
                && (metadata.len() != plan.metadata.len()
                    || metadata.modified().ok() != plan.metadata.modified().ok())
            {
                return Err("The image changed outside the app. Nothing was renamed.".into());
            }
            rustix::fs::renameat_with(
                rustix::fs::CWD,
                &original,
                rustix::fs::CWD,
                &destination,
                rustix::fs::RenameFlags::NOREPLACE,
            )
            .map_err(|error| format!("Cannot rename file or folder: {error}"))?;
        }
        #[cfg(not(target_os = "linux"))]
        return Err("Guarded renaming is currently available on Linux.".into());
        for note in &mut self.notes {
            note.path = moved_path(&note.path, &plan.from, &plan.to, folder);
        }
        if folder {
            for path in &mut self.folders {
                *path = moved_path(path, &plan.from, &plan.to, true);
            }
            self.folders.sort();
        }
        for path in &mut self.attachments {
            *path = moved_path(path, &plan.from, &plan.to, folder);
        }
        self.rebuild_lookup();
        let mut outcome = RenameOutcome {
            path: plan.to.clone(),
            updated_notes: 0,
            warnings: Vec::new(),
        };
        for mut rewrite in plan.rewrites {
            rewrite.document.path =
                moved_path(&rewrite.document.path, &plan.from, &plan.to, folder);
            match self.save(&mut rewrite.document) {
                Ok(()) => outcome.updated_notes += 1,
                Err(error) => outcome
                    .warnings
                    .push(format!("{}: {error}", rewrite.document.path.display())),
            }
        }
        self.notes
            .sort_by_key(|note| note.path.to_string_lossy().to_lowercase());
        self.rebuild_lookup();
        Ok(outcome)
    }

    pub fn trash_note(&mut self, path: &Path) -> Result<()> {
        let absolute = if self.attachments.iter().any(|attachment| attachment == path) {
            self.safe_file_path(path, false)?
        } else {
            self.safe_path(path, false)?
        };
        if !absolute.is_file() {
            return Err("Choose an existing file.".into());
        }
        trash::delete(&absolute)
            .map_err(|error| format!("Cannot move file to system Trash: {error}"))?;
        self.notes.retain(|note| note.path != path);
        self.attachments.retain(|attachment| attachment != path);
        self.rebuild_lookup();
        Ok(())
    }
}

// Find the label boundary before looking for a destination. Titles can contain
// the same delimiters, and inline code can contain brackets inside the label.
fn folder_entries(root: &Path) -> Result<Vec<(PathBuf, bool)>> {
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| format!("Cannot inspect folder: {error}"))?;
        let kind = entry.file_type();
        if !kind.is_dir() && !kind.is_file() {
            return Err("Folders containing linked or special files cannot be moved.".into());
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?;
        entries.push((relative.to_owned(), kind.is_dir()));
    }
    entries.sort();
    Ok(entries)
}

fn label_end(raw: &str) -> Option<usize> {
    let bytes = raw.as_bytes();
    let mut index = usize::from(bytes.first() == Some(&b'!'));
    if bytes.get(index) != Some(&b'[') {
        return None;
    }
    index += 1;
    let mut depth = 1;
    let code: Vec<_> = Parser::new(raw)
        .into_offset_iter()
        .filter_map(|(event, range)| matches!(event, Event::Code(_)).then_some(range))
        .collect();
    while index < bytes.len() {
        if let Some(range) = code.iter().find(|range| range.contains(&index)) {
            index = range.end;
            continue;
        }
        if bytes[index] == 92 {
            index += 2;
            continue;
        }
        match bytes[index] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn destination_range(
    source: &str,
    range: std::ops::Range<usize>,
    offset: usize,
) -> Option<std::ops::Range<usize>> {
    let mut start = range.start + offset;
    while source
        .as_bytes()
        .get(start)
        .is_some_and(u8::is_ascii_whitespace)
    {
        start += 1;
    }
    let angled = source.as_bytes().get(start) == Some(&b'<');
    if angled {
        start += 1;
    }
    let mut escaped = false;
    let mut depth = 0usize;
    for (offset, character) in source[start..range.end].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if (angled && character == '>')
            || (!angled && (character.is_whitespace() || (character == ')' && depth == 0)))
        {
            return Some(start..start + offset);
        }
        if !angled {
            if character == '(' {
                depth += 1;
            }
            if character == ')' {
                depth = depth.saturating_sub(1);
            }
        }
    }
    (start < range.end).then_some(start..range.end)
}
