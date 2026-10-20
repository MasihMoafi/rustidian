use std::path::{Path, PathBuf};

#[derive(Default, Clone)]
pub struct History {
    entries: Vec<PathBuf>,
    position: Option<usize>,
}

impl History {
    pub fn rename_path(&mut self, from: &Path, to: &Path) {
        for entry in &mut self.entries {
            *entry = crate::paths::moved_path(entry, from, to, true);
        }
    }
    pub fn visit(&mut self, path: &Path) {
        if self
            .position
            .and_then(|index| self.entries.get(index))
            .is_some_and(|current| current == path)
        {
            return;
        }
        self.entries
            .truncate(self.position.map_or(0, |index| index + 1));
        self.entries.push(path.into());
        self.position = Some(self.entries.len() - 1);
    }

    pub fn can_back(&self) -> bool {
        self.position.is_some_and(|index| index > 0)
    }
    pub fn can_forward(&self) -> bool {
        self.position
            .is_some_and(|index| index + 1 < self.entries.len())
    }

    pub fn back(&mut self) -> Option<PathBuf> {
        if !self.can_back() {
            return None;
        }
        self.position = self.position.map(|index| index - 1);
        self.position
            .and_then(|index| self.entries.get(index))
            .cloned()
    }

    pub fn forward(&mut self) -> Option<PathBuf> {
        if !self.can_forward() {
            return None;
        }
        self.position = self.position.map(|index| index + 1);
        self.position
            .and_then(|index| self.entries.get(index))
            .cloned()
    }
}
