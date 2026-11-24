use super::{Edge, Key, Kind, Node};
use crate::{
    metadata,
    vault::{Note, Vault, is_ignored_path, is_markdown},
};
use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::PathBuf,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Focus {
    pub author: Option<PathBuf>,
    pub note_type: Option<String>,
    pub folder: Option<PathBuf>,
    pub show_authors: bool,
}

impl Focus {
    pub fn active(&self) -> bool {
        self.author.is_some() || self.note_type.is_some() || self.folder.is_some()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Facets {
    pub authors: BTreeMap<PathBuf, String>,
    pub types: BTreeMap<String, String>,
    pub folders: BTreeSet<PathBuf>,
    pub(super) authored: Vec<(Edge, PathBuf)>,
    notes: BTreeMap<PathBuf, Properties>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Properties {
    authors: BTreeSet<PathBuf>,
    types: BTreeSet<String>,
}

impl Facets {
    pub(super) fn matches(&self, node: &Node, focus: &Focus) -> bool {
        if focus
            .folder
            .as_ref()
            .is_some_and(|folder| !node.path.starts_with(folder))
        {
            return false;
        }
        let properties = self.notes.get(&node.path);
        focus.author.as_ref().is_none_or(|author| {
            properties.is_some_and(|properties| properties.authors.contains(author))
        }) && focus.note_type.as_ref().is_none_or(|note_type| {
            properties.is_some_and(|properties| properties.types.contains(note_type))
        })
    }

    pub(super) fn edges(&self, allowed: &mut [bool], focus: &Focus) -> Vec<Edge> {
        if !focus.active() && !focus.show_authors {
            return Vec::new();
        }
        // Only notes passing the filters contribute relationships. The author can
        // live outside the selected folder and must not seed unrelated edges.
        let edges: Vec<_> = self
            .authored
            .iter()
            .filter_map(|(edge, author)| {
                (allowed[edge.from]
                    && focus
                        .author
                        .as_ref()
                        .is_none_or(|selected| selected == author))
                .then_some(*edge)
            })
            .collect();
        for edge in &edges {
            allowed[edge.to] = true;
        }
        edges
    }
}

fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

struct Credit {
    name: String,
    handle: Option<String>,
    linked: Option<PathBuf>,
}

pub(super) struct Index<'a> {
    vault: &'a Vault,
    names: HashMap<String, Option<&'a Note>>,
    handles: HashMap<String, BTreeSet<String>>,
    credits: Vec<(PathBuf, Credit)>,
    facets: Facets,
}

impl<'a> Index<'a> {
    pub(super) fn new(vault: &'a Vault) -> Self {
        let mut names = HashMap::new();
        for note in &vault.notes {
            let title = note.title();
            for name in std::iter::once(&title).chain(&note.aliases) {
                names
                    .entry(normalized(name))
                    .and_modify(|match_: &mut Option<&Note>| {
                        if match_.is_some_and(|earlier| earlier.path != note.path) {
                            *match_ = None;
                        }
                    })
                    .or_insert(Some(note));
            }
        }
        Self {
            vault,
            names,
            handles: HashMap::new(),
            credits: Vec::new(),
            facets: Facets {
                folders: vault
                    .folders
                    .iter()
                    .filter(|folder| !is_ignored_path(folder))
                    .cloned()
                    .collect(),
                ..Facets::default()
            },
        }
    }

    pub(super) fn add(&mut self, note: &Note, properties: metadata::Frontmatter) {
        for (property, value) in properties.values {
            let Some(property) = property.as_str() else {
                continue;
            };
            let property = property.to_ascii_lowercase();
            if !matches!(property.as_str(), "author" | "authors" | "type") {
                continue;
            }
            for value in property_values(&value) {
                let value = value.trim();
                if value.is_empty() {
                    continue;
                }
                if property == "type" {
                    let key = normalized(value);
                    self.facets
                        .types
                        .entry(key.clone())
                        .or_insert_with(|| value.to_owned());
                    self.facets
                        .notes
                        .entry(note.path.clone())
                        .or_default()
                        .types
                        .insert(key);
                } else if let Some(credit) = self.credit(note, value) {
                    self.credits.push((note.path.clone(), credit));
                }
            }
        }
    }

    fn credit(&mut self, note: &Note, value: &str) -> Option<Credit> {
        let link = value
            .strip_prefix("[[")
            .and_then(|value| value.strip_suffix("]]"));
        let linked = link
            .and_then(|target| self.vault.resolve_destination(&note.path, target))
            .filter(|path| is_markdown(path) && !is_ignored_path(path));
        let label = link
            .map_or(value, |target| {
                target.rsplit_once('|').map_or(target, |(_, label)| label)
            })
            .trim();
        let (name, handle) = label
            .strip_suffix(')')
            .and_then(|label| label.rsplit_once(" (@"))
            .filter(|(name, handle)| {
                !name.is_empty()
                    && !handle.is_empty()
                    && handle
                        .chars()
                        .all(|ch| ch.is_alphanumeric() || matches!(ch, '_' | '-' | '.'))
            })
            .map_or((label, None), |(name, handle)| {
                (name, Some(handle.to_lowercase()))
            });
        if let Some(handle) = &handle {
            self.handles
                .entry(normalized(name))
                .or_default()
                .insert(handle.clone());
        }
        (!name.is_empty()).then(|| Credit {
            name: name.to_owned(),
            handle,
            linked,
        })
    }

    pub(super) fn finish(
        mut self,
        nodes: &mut BTreeMap<Key, Node>,
    ) -> (Facets, BTreeSet<(Key, Key)>) {
        let mut links = BTreeSet::new();
        for (
            note,
            Credit {
                name,
                handle,
                linked,
            },
        ) in self.credits
        {
            let key = normalized(&name);
            let known_handles = self.handles.get(&key);
            let ambiguous = known_handles.is_some_and(|handles| handles.len() > 1);
            let existing = linked
                .or_else(|| {
                    let exact = handle
                        .as_ref()
                        .and_then(|handle| self.names.get(&format!("{key} (@{handle})")));
                    exact
                        .or_else(|| (!ambiguous).then(|| self.names.get(&key)).flatten())
                        .and_then(|note| note.as_ref())
                        .map(|note| note.path.clone())
                })
                .filter(|path| nodes.contains_key(&(0, path.clone())));
            let (target, label) = if let Some(path) = existing {
                let target = (0, path);
                let label = nodes[&target].label.clone();
                (target, label)
            } else {
                let handle = handle.or_else(|| {
                    known_handles
                        .filter(|handles| handles.len() == 1)
                        .and_then(|handles| handles.first().cloned())
                });
                let (identity, label) = match handle {
                    Some(handle) => (format!("{key} (@{handle})"), format!("{name} (@{handle})")),
                    None => (key, name),
                };
                let path = PathBuf::from(format!(
                    "@author:{}",
                    identity.replace('%', "%25").replace('/', "%2F")
                ));
                let target = (2, path.clone());
                let node = nodes.entry(target.clone()).or_insert_with(|| Node {
                    path,
                    label,
                    kind: Kind::Author,
                });
                (target, node.label.clone())
            };
            self.facets.authors.entry(target.1.clone()).or_insert(label);
            self.facets
                .notes
                .entry(note.clone())
                .or_default()
                .authors
                .insert(target.1.clone());
            if note != target.1 {
                links.insert(((0, note), target));
            }
        }
        (self.facets, links)
    }
}

fn property_values(value: &Value) -> Vec<String> {
    // An unquoted Obsidian wikilink is parsed as a nested one-item YAML list.
    if let Value::Sequence(values) = value {
        return values
            .iter()
            .flat_map(|value| {
                if let Value::Sequence(inner) = value
                    && let [Value::String(target)] = inner.as_slice()
                {
                    vec![format!("[[{target}]]")]
                } else {
                    metadata::strings(Some(value))
                }
            })
            .collect();
    }
    metadata::strings(Some(value))
}
