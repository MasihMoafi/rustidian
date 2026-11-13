//! What the user opens, so Ctrl+O can put the files and folders opened most and most
//! recently first among loose matches and in its empty list. Exact-name matches keep
//! their own order (the least nested first), so a habit never hides the main file.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// How many opened paths are remembered; the weakest are forgotten first.
const KEPT: usize = 500;
/// An open counts half as much after this many days.
const HALF_LIFE_DAYS: f64 = 14.0;

#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
struct Opened {
    count: u32,
    /// Seconds since the Unix epoch.
    last: u64,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct History {
    opens: HashMap<PathBuf, Opened>,
}

/// Seconds since the Unix epoch.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

impl History {
    pub fn file(state_base: &Path) -> PathBuf {
        crate::paths::app_directory(state_base).join("open-history.json")
    }

    /// The saved history; a missing or unreadable file is an empty one.
    pub fn load(file: &Path) -> Self {
        std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let staging = file.with_extension("json.tmp");
        std::fs::write(&staging, serde_json::to_vec(self)?)?;
        std::fs::rename(staging, file)
    }

    /// Count one open of absolute `path` at `now`.
    pub fn record(&mut self, path: &Path, now: u64) {
        let opened = self.opens.entry(path.to_owned()).or_insert(Opened {
            count: 0,
            last: now,
        });
        opened.count = opened.count.saturating_add(1);
        opened.last = now;
        while self.opens.len() > KEPT {
            let weakest = self
                .opens
                .keys()
                .min_by(|a, b| self.score(a, now).total_cmp(&self.score(b, now)))
                .cloned();
            match weakest {
                Some(path) => self.opens.remove(&path),
                None => break,
            };
        }
    }

    /// Opens, each worth less the longer ago it was; 0 for a path never opened.
    pub fn score(&self, path: &Path, now: u64) -> f64 {
        self.opens.get(path).map_or(0.0, |opened| {
            let days = now.saturating_sub(opened.last) as f64 / 86_400.0;
            f64::from(opened.count) * 0.5_f64.powf(days / HALF_LIFE_DAYS)
        })
    }

    /// Up to `limit` remembered paths, strongest first.
    pub fn top(&self, now: u64, limit: usize) -> Vec<PathBuf> {
        let mut paths: Vec<_> = self.opens.keys().cloned().collect();
        paths.sort_by(|a, b| {
            self.score(b, now)
                .total_cmp(&self.score(a, now))
                .then_with(|| a.cmp(b))
        });
        paths.truncate(limit);
        paths
    }
}

/// `paths` with the ones `score` rates above zero moved to the front, at most `limit`
/// of them, strongest first; everything else keeps its order. A loose match the user
/// opens often rises without pushing any other match out of the list.
pub fn lift(paths: Vec<PathBuf>, limit: usize, score: impl Fn(&Path) -> f64) -> Vec<PathBuf> {
    let mut scored: Vec<(f64, usize)> = paths
        .iter()
        .enumerate()
        .map(|(index, path)| (score(path), index))
        .filter(|(value, _)| *value > 0.0)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.truncate(limit);
    let lifted: Vec<usize> = scored.iter().map(|(_, index)| *index).collect();
    let mut out: Vec<PathBuf> = lifted.iter().map(|index| paths[*index].clone()).collect();
    out.extend(
        paths
            .into_iter()
            .enumerate()
            .filter(|(index, _)| !lifted.contains(index))
            .map(|(_, path)| path),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_rank_by_count_and_fade_with_time() {
        let day = 86_400;
        let mut history = History::default();
        let (often, once, old) = (
            Path::new("/a/often.md"),
            Path::new("/a/once.md"),
            Path::new("/a/old.md"),
        );
        for _ in 0..3 {
            history.record(often, 100 * day);
        }
        history.record(once, 100 * day);
        for _ in 0..3 {
            history.record(old, 30 * day);
        }
        assert_eq!(
            history.top(100 * day, 3),
            vec![often.to_owned(), once.to_owned(), old.to_owned()]
        );
        assert_eq!(history.score(Path::new("/never.md"), 100 * day), 0.0);
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("rustidian/open-history.json");
        history.save(&file).unwrap();
        assert_eq!(
            History::load(&file).top(100 * day, 1),
            vec![often.to_owned()]
        );
        assert_eq!(
            History::load(Path::new("/missing/history.json")).top(0, 5),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn lifting_moves_known_paths_forward_and_drops_nothing() {
        let paths: Vec<PathBuf> = ["a", "b", "c", "d"].iter().map(PathBuf::from).collect();
        let score = |path: &Path| match path.to_str() {
            Some("c") => 2.0,
            Some("d") => 5.0,
            _ => 0.0,
        };
        let names = |paths: Vec<PathBuf>| {
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(lift(paths.clone(), 5, score)), ["d", "c", "a", "b"]);
        assert_eq!(names(lift(paths.clone(), 1, score)), ["d", "a", "b", "c"]);
        assert_eq!(names(lift(paths, 5, |_| 0.0)), ["a", "b", "c", "d"]);
    }
}
