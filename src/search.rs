use nucleo_matcher::{
    Config, Matcher,
    pattern::{Atom, AtomKind, CaseMatching, Normalization},
};
use std::path::PathBuf;

struct Entry {
    path: PathBuf,
    label: String,
}
impl AsRef<str> for Entry {
    fn as_ref(&self) -> &str {
        &self.label
    }
}

pub struct FileMatcher {
    entries: Vec<Entry>,
    matcher: Matcher,
}

impl FileMatcher {
    pub fn folders(vault: &crate::vault::Vault, current: &std::path::Path) -> Self {
        let mut paths = vec![current.to_owned(), PathBuf::new()];
        paths.extend(vault.folders.iter().cloned());
        let mut seen = std::collections::HashSet::new();
        let entries = paths
            .into_iter()
            .filter(|path| seen.insert(path.clone()))
            .map(|path| Entry {
                label: if path.as_os_str().is_empty() {
                    "/".into()
                } else {
                    path.to_string_lossy().into_owned()
                },
                path,
            })
            .collect();
        Self {
            entries,
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
        }
    }

    pub fn from_vault(vault: &crate::vault::Vault) -> Self {
        let mut matcher = Self::from_notes(&vault.notes);
        matcher
            .entries
            .extend(vault.attachments.iter().map(|path| Entry {
                path: path.clone(),
                label: path.to_string_lossy().into_owned(),
            }));
        matcher
    }

    pub fn new(paths: impl IntoIterator<Item = PathBuf>) -> Self {
        let entries = paths
            .into_iter()
            .map(|path| Entry {
                label: path.with_extension("").to_string_lossy().into_owned(),
                path,
            })
            .collect();
        Self {
            entries,
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
        }
    }

    pub fn from_notes(notes: &[crate::vault::Note]) -> Self {
        let mut matcher = Self::new(notes.iter().map(|note| note.path.clone()));
        for note in notes {
            for alias in &note.aliases {
                matcher.entries.push(Entry {
                    path: note.path.clone(),
                    label: alias.clone(),
                });
            }
        }
        matcher
    }

    pub fn rank(&mut self, query: &str, limit: usize) -> Vec<PathBuf> {
        self.rank_matching(query, limit, |_| true)
    }

    pub fn rank_folders(&mut self, query: &str, limit: usize) -> Vec<PathBuf> {
        let mut paths = self.rank(query, usize::MAX);
        if !query.is_empty()
            && let Some(index) = paths.iter().position(|path| {
                if path.as_os_str().is_empty() {
                    query == "/"
                } else {
                    path == std::path::Path::new(query)
                }
            })
        {
            paths[..=index].rotate_right(1);
        }
        paths.truncate(limit);
        paths
    }

    pub fn rank_links(&mut self, query: &str, limit: usize) -> Vec<PathBuf> {
        self.rank_matching(query, limit, crate::paths::wiki_path_supported)
    }

    fn rank_matching(
        &mut self,
        query: &str,
        limit: usize,
        include: impl Fn(&std::path::Path) -> bool,
    ) -> Vec<PathBuf> {
        Atom::new(
            query,
            CaseMatching::Smart,
            Normalization::Smart,
            AtomKind::Fuzzy,
            false,
        )
        .match_list(
            self.entries.iter().filter(|entry| include(&entry.path)),
            &mut self.matcher,
        )
        .into_iter()
        .map(|(entry, _)| entry.path.clone())
        .scan(std::collections::HashSet::new(), |seen, path| {
            Some(if seen.insert(path.clone()) {
                Some(path)
            } else {
                None
            })
        })
        .flatten()
        .take(limit)
        .collect()
    }
}
