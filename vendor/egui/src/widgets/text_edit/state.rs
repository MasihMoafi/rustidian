use alloc::sync::Arc;

use epaint::text::cursor::CCursor;

use crate::mutex::Mutex;

use crate::{
    Context, Id, Vec2,
    text_selection::{CCursorRange, TextCursorState},
};

pub type TextEditUndoer = crate::util::undoer::Undoer<(CCursorRange, String)>;

fn default_text_undoer() -> TextEditUndoer {
    TextEditUndoer::with_settings(crate::util::undoer::Settings {
        stable_time: 0.5,
        auto_save_interval: 1.0,
        ..Default::default()
    })
    .with_state_equality(|left, right| left.1 == right.1)
}

/// The text edit state stored between frames.
///
/// Attention: You also need to `store` the updated state.
/// ```
/// # egui::__run_test_ui(|ui| {
/// # let mut text = String::new();
/// use egui::text::{CCursor, CCursorRange};
///
/// let mut output = egui::TextEdit::singleline(&mut text).show(ui);
///
/// // Create a new selection range
/// let min = CCursor::new(0);
/// let max = CCursor::new(0);
/// let new_range = CCursorRange::two(min, max);
///
/// // Update the state
/// output.state.cursor.set_char_range(Some(new_range));
/// // Store the updated state
/// output.state.store(ui.ctx(), output.response.id);
/// # });
/// ```
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct TextEditState {
    /// Controls the text selection.
    pub cursor: TextCursorState,

    /// The purpose of the cursor.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) cursor_purpose: TextEditCursorPurpose,

    /// Wrapped in Arc for cheaper clones.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) undoer: Arc<Mutex<TextEditUndoer>>,

    // Text offset within the widget area.
    // Used for sensing and singleline text clipping.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) text_offset: Vec2,

    /// When did the user last press a key or click on the `TextEdit`.
    /// Used to pause the cursor animation when typing.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) last_interaction_time: f64,

    /// The selection last sent with [`crate::OutputCommand::TextSelectionSettled`].
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) reported_selection: Option<CCursorRange>,
}

impl Default for TextEditState {
    fn default() -> Self {
        Self {
            cursor: Default::default(),
            cursor_purpose: Default::default(),
            undoer: Arc::new(Mutex::new(default_text_undoer())),
            text_offset: Vec2::ZERO,
            last_interaction_time: 0.0,
            reported_selection: None,
        }
    }
}

impl TextEditState {
    pub fn load(ctx: &Context, id: Id) -> Option<Self> {
        ctx.data_mut(|d| d.get_persisted(id))
    }

    pub fn store(self, ctx: &Context, id: Id) {
        ctx.data_mut(|d| d.insert_persisted(id, self));
    }

    pub fn undoer(&self) -> TextEditUndoer {
        self.undoer.lock().clone()
    }

    /// Track text changes without cloning the complete shared history.
    pub fn feed_undo_state(&self, time: f64, text: &str) {
        let cursor = self.cursor.char_range().unwrap_or_default();
        self.undoer
            .lock()
            .feed_state(time, &(cursor, text.to_owned()));
    }

    #[expect(clippy::needless_pass_by_ref_mut)] // Intentionally hide interiority of mutability
    pub fn set_undoer(&mut self, undoer: TextEditUndoer) {
        *self.undoer.lock() = undoer;
    }

    /// Share edit history while retaining this view's cursor and scroll state.
    pub fn share_undoer_with(&mut self, other: &Self) {
        self.undoer = other.undoer.clone();
    }

    pub fn clear_undoer(&mut self) {
        self.set_undoer(default_text_undoer());
    }

    /// Move through text changes without turning cursor-only snapshots into edits.
    pub(super) fn undo_or_redo(
        &self,
        cursor: CCursorRange,
        text: &str,
        redo: bool,
    ) -> Option<(CCursorRange, String)> {
        let mut current = (cursor, text.to_owned());
        let mut history = self.undoer.lock();
        loop {
            let next = if redo {
                history.redo(&current)
            } else {
                history.undo(&current)
            }?
            .clone();
            if next.1 != text {
                return Some(next);
            }
            current = next;
        }
    }
}

#[derive(Clone, Default)]
pub(crate) enum TextEditCursorPurpose {
    /// The cursor is used for text selection.
    #[default]
    Selection,

    /// The cursor is used for IME composition. Its direction is irrelevant in
    /// this case.
    ImeComposition {
        /// An optional cursor/segment within the composing text itself,
        /// relative to the start of the composing region. Its direction is
        /// irrelevant.
        ///
        /// When `None`, no active range is displayed.
        active_range: Option<core::ops::Range<CCursor>>,
    },
}

impl TextEditCursorPurpose {
    pub(crate) fn is_selection(&self) -> bool {
        matches!(self, Self::Selection)
    }

    pub(crate) fn is_ime_composition(&self) -> bool {
        matches!(self, Self::ImeComposition { .. })
    }
}
