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
    let events = std::iter::once(Event::ModifiersChanged(
        Modifiers::CTRL | Modifiers::COMMAND,
    ))
    .chain(events)
    .collect();
    ctx.run_ui(
        egui::RawInput {
            time: Some(time),
            events,
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
    let id = egui::Id::unique("cursor-snapshot-history");
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
    let id = egui::Id::unique("queued-undo-history");
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

#[test]
fn ordinary_edits_keep_independent_undo_and_redo_groups() {
    for (name, time, event, replace, expected) in [
        (
            "pause",
            0.7,
            Event::Text(" second".into()),
            false,
            "first second",
        ),
        (
            "paste",
            0.2,
            Event::Paste(" second".into()),
            false,
            "first second",
        ),
        (
            "replacement",
            0.2,
            Event::Text("second".into()),
            true,
            "second",
        ),
        ("cut", 0.2, Event::Cut, true, ""),
    ] {
        let ctx = egui::Context::default();
        let id = egui::Id::unique(name);
        let mut text = "Original note: ".to_owned();
        frame(&ctx, id, &mut text, 0.0, vec![]);
        let mut state = TextEditState::load(&ctx, id).unwrap();
        state
            .cursor
            .set_char_range(Some(cursor(text.chars().count())));
        state.store(&ctx, id);
        frame(&ctx, id, &mut text, 0.1, vec![Event::Text("first".into())]);
        if replace {
            let mut state = TextEditState::load(&ctx, id).unwrap();
            state.cursor.set_char_range(Some(CCursorRange::two(
                CCursor::new("Original note: ".chars().count()),
                CCursor::new(text.chars().count()),
            )));
            state.store(&ctx, id);
        }
        frame(&ctx, id, &mut text, time, vec![event]);
        assert_eq!(text, format!("Original note: {expected}"), "{name}");
        frame(&ctx, id, &mut text, time + 0.1, vec![key(Key::Z)]);
        assert_eq!(
            text, "Original note: first",
            "{name}: retain earlier typing"
        );
        frame(&ctx, id, &mut text, time + 0.2, vec![key(Key::Y)]);
        assert_eq!(text, format!("Original note: {expected}"), "{name}: redo");
        frame(&ctx, id, &mut text, time + 0.3, vec![key(Key::Z)]);
        frame(&ctx, id, &mut text, time + 0.4, vec![key(Key::Z)]);
        assert_eq!(text, "Original note: ", "{name}: second undo");
        frame(
            &ctx,
            id,
            &mut text,
            time + 0.5,
            vec![Event::Paste("branch".into())],
        );
        frame(&ctx, id, &mut text, time + 0.6, vec![key(Key::Y)]);
        assert_eq!(
            text, "Original note: branch",
            "{name}: a new edit must discard the old redo branch"
        );
    }
}

#[test]
fn uninterrupted_typing_retains_intermediate_undo_points() {
    let ctx = egui::Context::default();
    let id = egui::Id::unique("continuous-typing");
    let mut text = String::new();
    frame(&ctx, id, &mut text, 0.0, vec![]);
    for n in 1..=12 {
        frame(
            &ctx,
            id,
            &mut text,
            f64::from(n) / 10.0,
            vec![Event::Text("x".into())],
        );
    }
    frame(&ctx, id, &mut text, 1.3, vec![key(Key::Z)]);
    assert!(
        !text.is_empty() && text.len() < 12,
        "Undo must retain earlier continuous typing: {text:?}"
    );
    frame(&ctx, id, &mut text, 1.4, vec![key(Key::Y)]);
    assert_eq!(text, "xxxxxxxxxxxx");
}
