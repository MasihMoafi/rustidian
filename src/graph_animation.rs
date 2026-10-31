//! Transient, bounded chronological playback; no note or workspace writes.
use std::time::SystemTime;

const DURATION: f32 = 8.0;
const FADE: f32 = 0.2;

pub struct Timeline {
    ranks: Vec<Option<usize>>,
    dates: Vec<SystemTime>,
}

impl Timeline {
    pub fn new(nodes: usize, mut dates: Vec<(usize, SystemTime)>) -> Self {
        dates.sort_by_key(|&(node, date)| (date, node));
        let mut ranks = vec![None; nodes];
        for (rank, &(node, _)) in dates.iter().enumerate() {
            ranks[node] = Some(rank);
        }
        Self {
            ranks,
            dates: dates.into_iter().map(|(_, date)| date).collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.dates.len()
    }

    pub fn alpha(&self, node: usize, elapsed: f32) -> f32 {
        let Some(rank) = self.ranks.get(node).copied().flatten() else {
            return 0.0;
        };
        if rank == 0 {
            return 1.0;
        }
        let born = rank as f32 * DURATION / (self.len().saturating_sub(1).max(1) as f32);
        let fraction = ((elapsed - born) / FADE).clamp(0.0, 1.0);
        fraction * fraction * (3.0 - 2.0 * fraction)
    }

    pub fn date(&self, elapsed: f32) -> Option<SystemTime> {
        let rank = (elapsed / DURATION * self.len().saturating_sub(1) as f32).floor() as usize;
        self.dates
            .get(rank.min(self.len().saturating_sub(1)))
            .copied()
    }
}

#[derive(Default)]
pub struct Playback {
    pub elapsed: f32,
}

impl Playback {
    pub fn tick(&mut self, seconds: f32) -> bool {
        if seconds.is_finite() {
            self.elapsed += seconds.clamp(0.0, 0.1);
        }
        self.elapsed < DURATION + FADE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn playback_reveals_chronologically_and_finishes_without_a_background_clock() {
        let day = |day: u64| UNIX_EPOCH + Duration::from_secs(day * 86_400);
        let timeline = Timeline::new(4, vec![(0, day(3)), (2, day(1)), (1, day(2))]);
        assert_eq!(timeline.alpha(2, 0.0), 1.0);
        assert_eq!(timeline.alpha(0, 0.0), 0.0);
        assert_eq!(timeline.alpha(3, 9.0), 0.0);
        assert_eq!(timeline.date(0.0), Some(day(1)));
        assert!((timeline.alpha(1, 4.1) - 0.5).abs() < 0.0001);
        assert_eq!(timeline.alpha(0, 7.9), 0.0);
        assert_eq!(timeline.alpha(0, 8.3), 1.0);
        let mut playback = Playback::default();
        assert!(playback.tick(f32::NAN));
        assert_eq!(playback.elapsed, 0.0);
        for _ in 0..83 {
            playback.tick(0.1);
        }
        assert!(!playback.tick(0.0));
        assert!(Timeline::new(0, vec![]).date(1.0).is_none());
    }
}
