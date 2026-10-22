use eframe::egui::{
    self, Event, Key, Modifiers,
    text::{CCursor, CCursorRange},
    widgets::text_edit::TextEditState,
};

fn cursor(index: usize) -> CCursorRange {
    CCursorRange::one(CCursor::new(index))
}

fn key(key: Key) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed: true,
        repeat: false,
        modifiers: Modifiers::CTRL | Modifiers::COMMAND,
    }
}

fn frame(ctx: &egui::Context, id: egui::Id, text: &mut String, time: f64, events: Vec<Event>) {
    ctx.memory_mut(|memory| memory.request_focus(id));
    ctx.run_ui(
        egui::RawInput {
            time: Some(time),
            events,
            modifiers: Modifiers::CTRL | Modifiers::COMMAND,
            ..Default::default()
        },
        |ui| {
            ui.add(egui::TextEdit::multiline(text).id(id).desired_width(400.0));
        },
    )
    .drop_without_applying_deltas();
}

#[test]
fn one_keyboard_undo_and_redo_skip_cursor_only_snapshots() {
    let ctx = egui::Context::default();
    let id = egui::Id::new("cursor-snapshot-history");
    let mut text = "After".to_owned();
    frame(&ctx, id, &mut text, 0.0, vec![]);
    let mut state = TextEditState::load(&ctx, id).unwrap();
    state.clear_undoer();
    let mut history = state.undoer();
    history.add_undo(&(cursor(0), "Before".to_owned()));
    history.add_undo(&(cursor(1), text.clone()));
    history.add_undo(&(cursor(2), text.clone()));
    state.set_undoer(history);
    state.cursor.set_char_range(Some(cursor(5)));
    state.store(&ctx, id);

    frame(&ctx, id, &mut text, 0.1, vec![key(Key::Z)]);
    assert_eq!(text, "Before");
    frame(&ctx, id, &mut text, 0.2, vec![key(Key::Y)]);
    assert_eq!(text, "After");
    frame(&ctx, id, &mut text, 0.3, vec![key(Key::Y)]);
    assert_eq!(text, "After");
}

#[test]
fn queued_typing_then_undo_keeps_event_order_and_a_caret_move_alone_changes_no_text() {
    let ctx = egui::Context::default();
    let id = egui::Id::new("queued-undo-history");
    let mut text = "Before".to_owned();
    frame(&ctx, id, &mut text, 0.0, vec![]);
    let mut state = TextEditState::load(&ctx, id).unwrap();
    state.clear_undoer();
    let mut history = state.undoer();
    history.add_undo(&(cursor(6), text.clone()));
    state.set_undoer(history);
    state.cursor.set_char_range(Some(cursor(6)));
    state.store(&ctx, id);
    frame(
        &ctx,
        id,
        &mut text,
        0.1,
        vec![Event::Text("X".into()), key(Key::Z)],
    );
    assert_eq!(text, "Before");
    frame(&ctx, id, &mut text, 0.2, vec![key(Key::Y)]);
    assert_eq!(text, "BeforeX");

    let mut state = TextEditState::load(&ctx, id).unwrap();
    state.clear_undoer();
    let mut history = state.undoer();
    history.add_undo(&(cursor(0), text.clone()));
    history.add_undo(&(cursor(2), text.clone()));
    state.set_undoer(history);
    state.cursor.set_char_range(Some(cursor(4)));
    state.store(&ctx, id);
    frame(&ctx, id, &mut text, 0.3, vec![key(Key::Z)]);
    assert_eq!(text, "BeforeX");
}
