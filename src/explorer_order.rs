//! The explorer's own order (rows dragged into place) and each folder's index note.
//! The order lives in the per-vault workspace state, never in the vault.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Child names per folder ("" is the vault root), in the order the user dragged them.
/// Folders and files share one list; each group is arranged by its own entries.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Order(BTreeMap<String, Vec<String>>);

fn key(folder: &Path) -> String {
    folder.to_string_lossy().replace('\\', "/")
}

impl Order {
    /// Sort `names` by their saved places; names without one keep their order, after the rest.
    pub fn arrange<T>(&self, folder: &Path, items: &mut [T], name: impl Fn(&T) -> String) {
        let Some(saved) = self.0.get(&key(folder)) else {
            return;
        };
        items.sort_by_key(|item| {
            let name = name(item);
            saved
                .iter()
                .position(|entry| *entry == name)
                .unwrap_or(usize::MAX)
        });
    }

    /// Save `group` (one folder's displayed folders or files, already in their new order).
    pub fn set(&mut self, folder: &Path, group: Vec<String>) {
        let entry = self.0.entry(key(folder)).or_default();
        entry.retain(|name| !group.contains(name));
        entry.extend(group);
    }

    /// Keep a moved or renamed item's place, and the places inside a moved folder.
    pub fn renamed(&mut self, from: &Path, to: &Path) {
        let (Some(from_name), Some(to_name)) = (from.file_name(), to.file_name()) else {
            return;
        };
        let (from_parent, to_parent) = (
            from.parent().unwrap_or(Path::new("")),
            to.parent().unwrap_or(Path::new("")),
        );
        let (from_name, to_name) = (
            from_name.to_string_lossy().into_owned(),
            to_name.to_string_lossy().into_owned(),
        );
        if from_parent == to_parent
            && let Some(entry) = self.0.get_mut(&key(from_parent))
            && let Some(slot) = entry.iter_mut().find(|name| **name == from_name)
        {
            *slot = to_name;
        } else if let Some(entry) = self.0.get_mut(&key(from_parent)) {
            entry.retain(|name| *name != from_name);
        }
        let (old, new) = (key(from), key(to));
        let moved: Vec<_> = self
            .0
            .keys()
            .filter(|folder| **folder == old || folder.starts_with(&format!("{old}/")))
            .cloned()
            .collect();
        for folder in moved {
            if let Some(names) = self.0.remove(&folder) {
                self.0
                    .insert(format!("{new}{}", &folder[old.len()..]), names);
            }
        }
        self.0.retain(|_, names| !names.is_empty());
    }
}

/// Move `moving` (names in `group`, the displayed order) to just before or after `target`.
pub fn placed(group: &[String], moving: &[String], target: &str, after: bool) -> Vec<String> {
    let mut rest: Vec<String> = group
        .iter()
        .filter(|name| !moving.contains(name))
        .cloned()
        .collect();
    let at = rest
        .iter()
        .position(|name| name == target)
        .map_or(rest.len(), |index| index + usize::from(after));
    let moved: Vec<String> = group
        .iter()
        .filter(|name| moving.contains(name))
        .cloned()
        .collect();
    rest.splice(at..at, moved);
    rest
}

/// A folder's index note: `Index.md`, `index.md` or a note named after the folder.
pub fn index_note<'a>(
    folder: &Path,
    files: impl IntoIterator<Item = &'a PathBuf>,
) -> Option<&'a PathBuf> {
    let name = folder.file_name()?.to_string_lossy().to_lowercase();
    let mut named = None;
    for path in files {
        if !path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();
        if stem == "index" {
            return Some(path);
        }
        if stem == name && named.is_none() {
            named = Some(path);
        }
    }
    named
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    #[test]
    fn dragged_rows_keep_their_places_and_new_rows_follow() {
        let group = names(&["a.md", "b.md", "c.md", "d.md"]);
        assert_eq!(
            placed(&group, &names(&["d.md"]), "a.md", false),
            names(&["d.md", "a.md", "b.md", "c.md"])
        );
        assert_eq!(
            placed(&group, &names(&["a.md", "c.md"]), "d.md", true),
            names(&["b.md", "d.md", "a.md", "c.md"])
        );
        assert_eq!(placed(&group, &names(&["b.md"]), "c.md", false), group);

        let mut order = Order::default();
        order.set(Path::new("Learning"), names(&["c.md", "a.md"]));
        let mut shown = names(&["a.md", "b.md", "c.md", "new.md"]);
        order.arrange(Path::new("Learning"), &mut shown, String::clone);
        assert_eq!(shown, names(&["c.md", "a.md", "b.md", "new.md"]));
        let mut elsewhere = names(&["b.md", "a.md"]);
        order.arrange(Path::new("Other"), &mut elsewhere, String::clone);
        assert_eq!(elsewhere, names(&["b.md", "a.md"]));
    }

    #[test]
    fn a_rename_or_move_carries_the_saved_places() {
        let mut order = Order::default();
        order.set(Path::new("Learning"), names(&["c.md", "Articles"]));
        order.set(Path::new("Learning/Articles"), names(&["z.md", "y.md"]));
        order.renamed(Path::new("Learning/c.md"), Path::new("Learning/see.md"));
        order.renamed(Path::new("Learning/Articles"), Path::new("Reading"));
        let json = serde_json::to_value(&order).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"Learning": ["see.md"], "Reading": ["z.md", "y.md"]})
        );
    }

    #[test]
    fn an_index_note_is_index_md_or_named_after_its_folder() {
        let files = [
            PathBuf::from("Learning/Articles/Articles.md"),
            PathBuf::from("Learning/Articles/Index.md"),
        ];
        assert_eq!(
            index_note(Path::new("Learning/Articles"), &files),
            Some(&files[1])
        );
        assert_eq!(
            index_note(Path::new("Learning/Articles"), &files[..1]),
            Some(&files[0])
        );
        let plain = [
            PathBuf::from("Notes/index.png"),
            PathBuf::from("Notes/Plan.md"),
        ];
        assert_eq!(index_note(Path::new("Notes"), &plain), None);
        assert_eq!(index_note(Path::new(""), &files), None);
    }
}
