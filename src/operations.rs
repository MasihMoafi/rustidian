//! Guarded note/folder moves and operating-system Trash integration.
use crate::{
    paths::{self, moved_path},
    vault::{Document, Result, Vault, wiki_spans},
};
use pulldown_cmark::{Event, Parser, Tag};
use std::path::{Path, PathBuf};

struct Rewrite {
    document: Document,
    changes: Vec<LinkChange>,
}
struct LinkChange {
    range: std::ops::Range<usize>,
    replacement: String,
    target: String,
    destination: PathBuf,
}
pub struct LinkUpdates {
    root: PathBuf,
    path: PathBuf,
    rewrites: Vec<Rewrite>,
}

impl LinkUpdates {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn counts(&self) -> (usize, usize) {
        (
            self.rewrites
                .iter()
                .map(|rewrite| rewrite.changes.len())
                .sum(),
            self.rewrites.len(),
        )
    }
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

    pub fn begin_move_file(
        &mut self,
        from: &Path,
        folder: &Path,
        create_folder: bool,
    ) -> Result<Option<LinkUpdates>> {
        if crate::vault::is_ignored_path(from)
            || !(crate::vault::is_text_document(from) || crate::assets::is_image(from))
            || crate::vault::is_ignored_path(folder)
            || folder
                .components()
                .any(|part| !matches!(part, std::path::Component::Normal(_)))
        {
            return Err("Choose a visible file and relative folder inside this vault.".into());
        }
        let source = self.safe_file_path(from, false)?;
        let expected = std::fs::symlink_metadata(&source).map_err(|error| error.to_string())?;
        if !expected.is_file() || expected.file_type().is_symlink() {
            return Err("Choose an existing file inside this vault.".into());
        }
        let filename = from.file_name().ok_or("Choose a file first.")?;
        let first = folder.join(filename);
        if !folder.as_os_str().is_empty() {
            self.safe_file_path(folder, false)?;
        }
        match std::fs::symlink_metadata(self.root.join(folder)) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("Choose a real folder inside this vault.".into());
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create_folder => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err("This folder does not exist. Use Shift+Enter to create it.".into());
            }
            Err(error) => return Err(error.to_string()),
        }
        if first == from {
            return Ok(None);
        }
        let stem = from
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or("Invalid file name.")?;
        let extension = from.extension().and_then(|name| name.to_str());
        for counter in 0_u64.. {
            let destination = if counter == 0 {
                first.clone()
            } else {
                // "Makefile 1" would no longer count as a text document, so an
                // extension-less file keeps its name or stays where it is.
                let Some(extension) = extension else {
                    return Err("A file with this name already exists in that folder.".into());
                };
                folder.join(format!("{stem} {counter}.{extension}"))
            };
            let absolute = self.root.join(&destination);
            match std::fs::symlink_metadata(&absolute) {
                Ok(_) => continue,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
            let current = std::fs::symlink_metadata(&source).map_err(|error| error.to_string())?;
            if !same_move_source(&expected, &current) {
                return Err("The source changed outside the app. Nothing was moved.".into());
            }
            let attempt = self.plan_rename(from, &destination).and_then(|plan| {
                let current =
                    std::fs::symlink_metadata(&source).map_err(|error| error.to_string())?;
                if !same_move_source(&expected, &current) {
                    return Err("The source changed outside the app. Nothing was moved.".into());
                }
                self.begin_rename(plan)
            });
            match attempt {
                Ok(mut updates) => {
                    for rewrite in &mut updates.rewrites {
                        rewrite.changes.retain(|change| {
                            self.resolve_destination(&rewrite.document.path, &change.target)
                                .as_deref()
                                != Some(change.destination.as_path())
                        });
                    }
                    updates
                        .rewrites
                        .retain(|rewrite| !rewrite.changes.is_empty());
                    return Ok(Some(updates));
                }
                Err(_)
                    if std::fs::symlink_metadata(&absolute).is_ok()
                        && std::fs::symlink_metadata(&source)
                            .is_ok_and(|current| same_move_source(&expected, &current)) => {}
                Err(error) => return Err(error),
            }
        }
        Err("Cannot find an available destination name.".into())
    }

    pub fn plan_rename(&self, from: &Path, to: &Path) -> Result<RenamePlan> {
        if from == to {
            return Err("Choose a different name.".into());
        }
        if crate::vault::is_ignored_path(to) {
            return Err("Choose a visible destination inside this vault.".into());
        }
        let folder = self.folders.iter().any(|path| path == from);
        let attachment = self.attachments.iter().any(|path| path == from);
        let image = attachment && crate::assets::is_image(from);
        let kind = if folder {
            MoveKind::Folder
        } else if attachment {
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
        if attachment && !image && !crate::vault::is_text_document(to) {
            return Err("Keep a text filename extension when renaming a text file.".into());
        }
        if folder && to.starts_with(from) {
            return Err("A folder cannot move inside itself.".into());
        }
        let original = if folder {
            self.folder_path(from)?
        } else if attachment {
            self.safe_file_path(from, false)?
        } else {
            self.safe_path(from, false)?
        };
        let metadata = std::fs::symlink_metadata(&original)
            .map_err(|error| format!("Cannot read source: {error}"))?;
        if metadata.file_type().is_symlink()
            || (folder && !metadata.is_dir())
            || (attachment && !metadata.is_file())
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
        } else if attachment {
            Vec::new()
        } else {
            vec![self.read(from)?]
        };
        let destination = if folder {
            self.folder_path(to)?
        } else if attachment {
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
                    changes.push(LinkChange {
                        range,
                        replacement: format!("[[{link}]]"),
                        target,
                        destination,
                    });
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
                    let relocated = moved_path(&resolved, from, to, folder);
                    let mut destination =
                        paths::markdown_destination(&note_destination, &relocated);
                    if let Some((_, fragment)) = url.split_once('#') {
                        destination.push('#');
                        destination.push_str(fragment);
                    }
                    changes.push(LinkChange {
                        range,
                        replacement: destination,
                        target: url,
                        destination: relocated,
                    });
                }
            }
            if changes.is_empty() {
                continue;
            }
            let document = self.read(&note.path)?;
            if document.text != note.text {
                return Err(format!(
                    "{} changed outside the app. Refresh the vault before renaming.",
                    note.path.display()
                ));
            }
            changes.sort_by_key(|change| change.range.start);
            if changes
                .windows(2)
                .any(|pair| pair[0].range.end > pair[1].range.start)
            {
                return Err(format!(
                    "Overlapping links in {} need review before renaming.",
                    note.path.display()
                ));
            }
            rewrites.push(Rewrite { document, changes });
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
        let updates = self.begin_rename(plan)?;
        Ok(self.finish_link_updates(updates))
    }

    fn begin_rename(&mut self, plan: RenamePlan) -> Result<LinkUpdates> {
        let folder = plan.kind == MoveKind::Folder;
        let image = plan.kind == MoveKind::Image;
        let original = if folder {
            self.folder_path(&plan.from)?
        } else if image {
            self.safe_file_path(&plan.from, false)?
        } else {
            self.safe_path(&plan.from, false)?
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
            if self.read(&rewrite.document.path)?.text != rewrite.document.text {
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
            let destination = if folder {
                self.folder_path(&plan.to)?
            } else if image {
                self.safe_file_path(&plan.to, true)?
            } else {
                self.safe_path(&plan.to, true)?
            };
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
        } else {
            self.folders.extend(
                plan.to
                    .ancestors()
                    .skip(1)
                    .filter(|path| !path.as_os_str().is_empty())
                    .map(Path::to_owned),
            );
            self.folders.sort();
            self.folders.dedup();
        }
        for path in &mut self.attachments {
            *path = moved_path(path, &plan.from, &plan.to, folder);
        }
        self.rebuild_lookup();
        let rewrites = plan
            .rewrites
            .into_iter()
            .map(|mut rewrite| {
                rewrite.document.path =
                    moved_path(&rewrite.document.path, &plan.from, &plan.to, folder);
                rewrite
            })
            .collect();
        Ok(LinkUpdates {
            root: self.root.clone(),
            path: plan.to,
            rewrites,
        })
    }

    pub fn finish_link_updates(&mut self, updates: LinkUpdates) -> RenameOutcome {
        let mut outcome = RenameOutcome {
            path: updates.path,
            updated_notes: 0,
            warnings: Vec::new(),
        };
        if updates.root != self.root {
            outcome
                .warnings
                .push("No links updated: this move belongs to a different vault.".into());
            return outcome;
        }
        for mut rewrite in updates.rewrites {
            for change in rewrite.changes.into_iter().rev() {
                rewrite
                    .document
                    .text
                    .replace_range(change.range, &change.replacement);
            }
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
        outcome
    }

    /// Moves a note, attachment or folder (with everything inside) to the system Trash.
    pub fn trash_entry(&mut self, path: &Path) -> Result<()> {
        let folder = self.folders.iter().any(|folder| folder == path);
        let absolute = if folder || self.attachments.iter().any(|attachment| attachment == path) {
            self.safe_file_path(path, false)?
        } else {
            self.safe_path(path, false)?
        };
        let metadata = std::fs::symlink_metadata(&absolute)
            .map_err(|_| "Choose an existing file or folder.".to_owned())?;
        if metadata.file_type().is_symlink() || metadata.is_dir() != folder {
            return Err("Choose an existing file or folder.".into());
        }
        trash::delete(&absolute)
            .map_err(|error| format!("Cannot move to system Trash: {error}"))?;
        self.notes.retain(|note| !note.path.starts_with(path));
        self.attachments
            .retain(|attachment| !attachment.starts_with(path));
        self.folders.retain(|entry| !entry.starts_with(path));
        self.rebuild_lookup();
        Ok(())
    }
}

// Find the label boundary before looking for a destination. Titles can contain
// the same delimiters, and inline code can contain brackets inside the label.
fn same_move_source(expected: &std::fs::Metadata, current: &std::fs::Metadata) -> bool {
    let same = current.is_file()
        && !current.file_type().is_symlink()
        && expected.len() == current.len()
        && expected.modified().ok() == current.modified().ok();
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        same && expected.dev() == current.dev() && expected.ino() == current.ino()
    }
    #[cfg(not(unix))]
    {
        same
    }
}

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
