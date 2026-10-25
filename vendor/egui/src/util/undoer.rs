use alloc::collections::VecDeque;

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Settings {
    /// Maximum number of undos.
    /// If your state is resource intensive, you should keep this low.
    ///
    /// Default: `100`
    pub max_undos: usize,

    /// When that state hasn't changed for this many seconds,
    /// create a new undo point (if one is needed).
    ///
    /// Default value: `1.0` seconds.
    pub stable_time: f32,

    /// If the state is changing so often that we never get to `stable_time`,
    /// then still create a save point every `auto_save_interval` seconds,
    /// so we have something to undo to.
    ///
    /// Default value: `30` seconds.
    pub auto_save_interval: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            max_undos: 100,
            stable_time: 1.0,
            auto_save_interval: 30.0,
        }
    }
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
struct UndoPoint<State> {
    state: State,
    marker: Option<u64>,
}

#[cfg(feature = "serde")]
impl<'de, State: serde::Deserialize<'de>> serde::Deserialize<'de> for UndoPoint<State> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Existing serialized histories contain bare states.
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Stored<State> {
            Point {
                state: State,
                #[serde(default)]
                marker: Option<u64>,
            },
            State(State),
        }
        Ok(
            match <Stored<State> as serde::Deserialize<'de>>::deserialize(deserializer)? {
                Stored::Point { state, marker } => Self { state, marker },
                Stored::State(state) => Self {
                    state,
                    marker: None,
                },
            },
        )
    }
}

/// Automatic undo system.
///
/// Every frame you feed it the most recent state.
/// The [`Undoer`] compares it with the latest undo point
/// and if there is a change it may create a new undo point.
///
/// [`Undoer`] follows two simple rules:
///
/// 1) If the state has changed since the latest undo point, but has
///    remained stable for `stable_time` seconds, an new undo point is created.
/// 2) If the state does not stabilize within `auto_save_interval` seconds, an undo point is created.
///
/// Rule 1) will make sure an undo point is not created until you _stop_ dragging that slider.
/// Rule 2) will make sure that you will get some undo points even if you are constantly changing the state.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct Undoer<State> {
    settings: Settings,

    #[cfg_attr(feature = "serde", serde(skip))]
    same_state: Option<fn(&State, &State) -> bool>,

    /// New undoes are added to the back.
    /// Adjacent states may be equal when a marker records an external edit.
    /// The latest undo point may (often) be the current state.
    undos: VecDeque<UndoPoint<State>>,

    /// Stores redos immediately after a sequence of undos.
    /// Gets cleared every time the state changes.
    /// Does not need to be a deque, because there can only be up to `undos.len()` redos,
    /// which is already limited to `settings.max_undos`.
    redos: Vec<UndoPoint<State>>,

    #[cfg_attr(feature = "serde", serde(skip))]
    flux: Option<Flux<State>>,
}

impl<State> core::fmt::Debug for Undoer<State> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self { undos, redos, .. } = self;
        f.debug_struct("Undoer")
            .field("undo count", &undos.len())
            .field("redo count", &redos.len())
            .finish()
    }
}

impl<State> Default for Undoer<State>
where
    State: Clone + PartialEq,
{
    #[inline]
    fn default() -> Self {
        Self {
            settings: Settings::default(),
            same_state: None,
            undos: VecDeque::new(),
            redos: Vec::new(),
            flux: None,
        }
    }
}

/// Represents how the current state is changing
#[derive(Clone)]
struct Flux<State> {
    start_time: f64,
    latest_change_time: f64,
    latest_state: State,
}

impl<State> Undoer<State>
where
    State: Clone + PartialEq,
{
    /// Create a new [`Undoer`] with the given [`Settings`].
    pub fn with_settings(settings: Settings) -> Self {
        Self {
            settings,
            ..Default::default()
        }
    }

    /// Define which changes represent an edit, independently of ancillary state.
    pub fn with_state_equality(mut self, equal: fn(&State, &State) -> bool) -> Self {
        self.same_state = Some(equal);
        self
    }

    fn equality(&self) -> fn(&State, &State) -> bool {
        self.same_state.unwrap_or(PartialEq::eq)
    }

    /// Is there an earlier state or external edit to undo?
    pub fn has_undo(&self, current_state: &State) -> bool {
        match self.undos.len() {
            0 => false,
            1 => !self
                .undos
                .back()
                .is_some_and(|point| (self.equality())(&point.state, current_state)),
            _ => true,
        }
    }

    pub fn has_redo(&self, current_state: &State) -> bool {
        !self.redos.is_empty()
            && self
                .undos
                .back()
                .is_some_and(|point| (self.equality())(&point.state, current_state))
    }

    /// Return true if the state is currently changing
    pub fn is_in_flux(&self) -> bool {
        self.flux.is_some()
    }

    pub fn undo(&mut self, current_state: &State) -> Option<&State> {
        if self.has_undo(current_state) {
            self.flux = None;

            let equal = self.equality();
            if let Some(state) = self
                .undos
                .pop_back_if(|point| equal(&point.state, current_state))
            {
                self.redos.push(state);
            } else {
                self.redos.push(UndoPoint {
                    state: current_state.clone(),
                    marker: None,
                });
            }

            // Note: we keep the undo point intact.
            self.undos.back().map(|point| &point.state)
        } else {
            None
        }
    }

    pub fn redo(&mut self, current_state: &State) -> Option<&State> {
        if !self.undos.is_empty()
            && !self
                .undos
                .back()
                .is_some_and(|point| (self.equality())(&point.state, current_state))
        {
            // state changed since the last undo, redos should be cleared.
            self.redos.clear();
            None
        } else if let Some(state) = self.redos.pop() {
            self.undos.push_back(state);
            self.undos.back().map(|point| &point.state)
        } else {
            None
        }
    }

    /// Add an undo point if, and only if, there has been a change since the latest undo point.
    pub fn add_undo(&mut self, current_state: &State) {
        if !self
            .undos
            .back()
            .is_some_and(|point| (self.equality())(&point.state, current_state))
        {
            self.undos.push_back(UndoPoint {
                state: current_state.clone(),
                marker: None,
            });
        } else if let Some(point) = self.undos.back_mut()
            && point.state != *current_state
        {
            point.state = current_state.clone();
        }
        self.trim_undos();
        self.flux = None;
    }

    fn trim_undos(&mut self) {
        while self.undos.len() > self.settings.max_undos {
            self.undos.pop_front();
        }
    }

    /// Record an external edit without changing the state tracked by this undoer.
    ///
    /// Checkpoint the current state first, then append a distinct undo step carrying
    /// `marker`. Callers own the external edit and apply it when traversing this step.
    /// Like ordinary checkpoints, markers count toward `max_undos`; retaining a
    /// marker and its preceding checkpoint requires a capacity of at least two.
    pub fn add_marker(&mut self, current_state: &State, marker: u64) {
        self.add_undo(current_state);
        self.undos.push_back(UndoPoint {
            state: current_state.clone(),
            marker: Some(marker),
        });
        self.trim_undos();
        self.redos.clear();
        self.flux = None;
    }

    /// The marker transferred by the next successful undo or redo, if any.
    ///
    /// This does not advance history. An uncheckpointed state must be undone
    /// before a marker at the latest checkpoint can be transferred.
    pub fn step_marker(&self, current_state: &State, redo: bool) -> Option<u64> {
        if !self
            .undos
            .back()
            .is_some_and(|point| (self.equality())(&point.state, current_state))
        {
            return None;
        }
        if redo {
            self.redos.last().and_then(|point| point.marker)
        } else if self.has_undo(current_state) {
            self.undos.back().and_then(|point| point.marker)
        } else {
            None
        }
    }

    /// Drop external commands when their owning workspace is replaced.
    /// Text checkpoints and text redo remain available.
    pub fn clear_markers(&mut self) {
        self.undos.retain(|point| point.marker.is_none());
        self.redos.retain(|point| point.marker.is_none());
    }

    /// Call this as often as you want (e.g. every frame)
    /// and [`Undoer`] will determine if a new undo point should be created.
    ///
    /// * `current_time`: current time in seconds.
    pub fn feed_state(&mut self, current_time: f64, current_state: &State) {
        let equal = self.equality();
        match self.undos.back() {
            None => {
                // First time feed_state is called.
                // always create an undo point:
                self.add_undo(current_state);
            }
            Some(latest_undo) => {
                if equal(&latest_undo.state, current_state) {
                    self.flux = None;
                } else {
                    self.redos.clear();

                    match self.flux.as_mut() {
                        None => {
                            self.flux = Some(Flux {
                                start_time: current_time,
                                latest_change_time: current_time,
                                latest_state: current_state.clone(),
                            });
                        }
                        Some(flux) => {
                            if equal(&flux.latest_state, current_state) {
                                let time_since_latest_change =
                                    (current_time - flux.latest_change_time) as f32;
                                if time_since_latest_change >= self.settings.stable_time {
                                    self.add_undo(current_state);
                                }
                            } else {
                                let time_since_flux_start = (current_time - flux.start_time) as f32;
                                if time_since_flux_start >= self.settings.auto_save_interval {
                                    self.add_undo(current_state);
                                } else {
                                    flux.latest_change_time = current_time;
                                    flux.latest_state = current_state.clone();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Settings, Undoer};

    #[test]
    fn ordinary_automatic_checkpoints_still_undo_and_redo() {
        let mut history = Undoer::default();
        history.feed_state(0.0, &"before");
        history.feed_state(0.1, &"after");
        assert!(history.is_in_flux());
        history.feed_state(1.2, &"after");
        assert!(!history.is_in_flux());
        assert_eq!(history.step_marker(&"after", false), None);
        assert_eq!(history.undo(&"after"), Some(&"before"));
        assert_eq!(history.step_marker(&"before", true), None);
        assert_eq!(history.redo(&"before"), Some(&"after"));
        assert!(!history.has_redo(&"after"));
    }

    #[test]
    fn markers_traverse_between_text_changes_in_both_directions() {
        let mut history = Undoer::default();
        history.add_undo(&"before");
        history.add_marker(&"at resize", 7);

        // New typing may still be in flux when Undo is pressed.
        assert_eq!(history.step_marker(&"later typing", false), None);
        assert_eq!(history.undo(&"later typing"), Some(&"at resize"));
        assert_eq!(history.step_marker(&"at resize", false), Some(7));
        assert_eq!(history.undo(&"at resize"), Some(&"at resize"));
        assert_eq!(history.step_marker(&"at resize", false), None);
        assert_eq!(history.undo(&"at resize"), Some(&"before"));

        assert_eq!(history.step_marker(&"before", true), None);
        assert_eq!(history.redo(&"before"), Some(&"at resize"));
        assert_eq!(history.step_marker(&"at resize", true), Some(7));
        assert_eq!(history.redo(&"at resize"), Some(&"at resize"));
        assert_eq!(history.step_marker(&"at resize", true), None);
        assert_eq!(history.redo(&"at resize"), Some(&"later typing"));
        assert!(!history.has_redo(&"later typing"));
    }

    #[test]
    fn returning_to_the_same_text_does_not_skip_newer_text_history() {
        let mut history = Undoer::default();
        history.add_marker(&"A", 1);
        history.add_undo(&"B");
        history.add_undo(&"A");
        assert_eq!(history.step_marker(&"A", false), None);
        assert_eq!(history.undo(&"A"), Some(&"B"));
        assert_eq!(history.step_marker(&"B", false), None);
        assert_eq!(history.undo(&"B"), Some(&"A"));
        assert_eq!(history.step_marker(&"A", false), Some(1));
        assert_eq!(history.undo(&"A"), Some(&"A"));
        assert!(!history.has_undo(&"A"));
    }

    #[test]
    fn consecutive_markers_and_branch_edits_preserve_step_identity() {
        let mut history = Undoer::default();
        history.add_marker(&"unchanged", 1);
        history.add_marker(&"unchanged", 2);
        history.feed_state(100.0, &"unchanged");
        assert_eq!(history.step_marker(&"unchanged", false), Some(2));
        assert_eq!(history.undo(&"unchanged"), Some(&"unchanged"));
        assert_eq!(history.step_marker(&"unchanged", false), Some(1));
        assert_eq!(history.undo(&"unchanged"), Some(&"unchanged"));
        assert_eq!(history.step_marker(&"unchanged", true), Some(1));
        assert_eq!(history.redo(&"unchanged"), Some(&"unchanged"));
        assert_eq!(history.step_marker(&"unchanged", true), Some(2));
        history.add_marker(&"unchanged", 3);
        assert_eq!(history.step_marker(&"unchanged", true), None);
        assert!(!history.has_redo(&"unchanged"));
        assert_eq!(history.step_marker(&"unchanged", false), Some(3));
        assert_eq!(history.undo(&"unchanged"), Some(&"unchanged"));
        history.feed_state(101.0, &"new text branch");
        assert_eq!(history.step_marker(&"new text branch", true), None);
        assert_eq!(history.redo(&"new text branch"), None);
    }

    #[test]
    fn markers_respect_history_capacity_and_leave_a_retained_baseline() {
        let mut history = Undoer::with_settings(Settings {
            max_undos: 2,
            ..Settings::default()
        });
        history.add_undo(&"old");
        history.add_marker(&"current", 42);
        assert_eq!(history.step_marker(&"current", false), Some(42));
        assert_eq!(history.undo(&"current"), Some(&"current"));
        assert!(!history.has_undo(&"current"));
        assert_eq!(history.step_marker(&"current", true), Some(42));
        assert_eq!(history.redo(&"current"), Some(&"current"));
    }
    #[test]
    fn ancillary_cursor_changes_keep_redo_but_text_changes_invalidate_it() {
        let mut history = Undoer::default().with_state_equality(|a: &(usize, &str), b| a.1 == b.1);
        history.add_undo(&(0, "before"));
        history.add_marker(&(4, "after"), 9);
        assert_eq!(history.undo(&(7, "after")), Some(&(4, "after")));
        history.feed_state(1.0, &(2, "after"));
        assert_eq!(history.step_marker(&(2, "after"), true), Some(9));
        assert_eq!(history.redo(&(2, "after")), Some(&(4, "after")));
        history.undo(&(4, "after"));
        history.feed_state(2.0, &(2, "new text"));
        assert!(!history.has_redo(&(2, "new text")));
    }
}
