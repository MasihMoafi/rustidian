//! Middle-button page panning through the document's existing ScrollArea.
use eframe::egui::{self, Event, Id, PointerButton, Pos2, Rect, ScrollArea, Vec2};

#[derive(Clone, Copy)]
struct Viewport {
    id: Id,
    rect: Rect,
    offset: Vec2,
    maximum: Vec2,
}

struct Drag {
    viewport: Viewport,
    origin: Pos2,
    panning: bool,
    cancelled: bool,
}

#[derive(Default)]
pub struct PagePan {
    viewport: Option<Viewport>,
    drag: Option<Drag>,
    desired: Option<(Id, Vec2)>,
}

impl PagePan {
    /// Read the same native events as the editor. Ordinary middle clicks and
    /// every keyboard/primary-button event continue through unchanged.
    pub fn raw_input(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput, enabled: bool) {
        if !enabled || !raw.focused {
            self.viewport = None;
            self.drag = None;
            self.desired = None;
            return;
        }
        let threshold = ctx.options(|options| options.input_options.max_click_dist);
        let mut other_buttons = ctx.input(|input| {
            [
                input.pointer.button_down(PointerButton::Primary),
                input.pointer.button_down(PointerButton::Secondary),
            ]
        });
        let mut events = Vec::with_capacity(raw.events.len());
        for event in std::mem::take(&mut raw.events) {
            match &event {
                Event::PointerButton {
                    pos,
                    button: PointerButton::Middle,
                    pressed: true,
                    ..
                } if !other_buttons.into_iter().any(|down| down) => {
                    self.drag = self
                        .viewport
                        .filter(|viewport| viewport.rect.contains(*pos))
                        .map(|viewport| Drag {
                            viewport,
                            origin: *pos,
                            panning: false,
                            cancelled: false,
                        });
                    self.desired = None;
                }
                // The note pane reserves middle-button input for panning and
                // links. Winit emits PRIMARY paste on press, before a drag can
                // be distinguished, so never let that event mutate a note.
                Event::MiddleClickPaste { pos, .. }
                    if self
                        .viewport
                        .is_some_and(|viewport| viewport.rect.contains(*pos)) =>
                {
                    continue;
                }
                Event::PointerMoved(pos) => self.motion(*pos, threshold),
                Event::PointerButton {
                    pos,
                    button: PointerButton::Middle,
                    pressed: false,
                    ..
                } => {
                    self.motion(*pos, threshold);
                    if self.drag.as_ref().is_some_and(|drag| drag.panning) {
                        // Some backends coalesce movement into the release.
                        // Let egui observe its distance so a drag cannot become
                        // a middle-click link action, and still deliver release.
                        events.push(Event::PointerMoved(*pos));
                    }
                    self.drag = None;
                }
                Event::PointerButton {
                    button: button @ (PointerButton::Primary | PointerButton::Secondary),
                    pressed,
                    ..
                } => {
                    other_buttons[usize::from(*button == PointerButton::Secondary)] = *pressed;
                    if *pressed {
                        self.drag = None;
                        self.desired = None;
                    }
                }
                Event::Key {
                    key: egui::Key::Escape,
                    pressed: true,
                    ..
                } => {
                    if let Some(drag) = &mut self.drag {
                        if drag.panning {
                            self.desired = Some((drag.viewport.id, drag.viewport.offset));
                        }
                        drag.cancelled = true;
                    }
                }
                Event::PointerGone => self.drag = None,
                _ => {}
            }
            events.push(event);
        }
        raw.events = events;
    }

    fn motion(&mut self, pointer: Pos2, threshold: f32) {
        let Some(drag) = &mut self.drag else {
            return;
        };
        if drag.cancelled || !pointer.is_finite() {
            return;
        }
        let delta = pointer - drag.origin;
        drag.panning |= delta.length() > threshold;
        if drag.panning {
            self.desired = Some((
                drag.viewport.id,
                (drag.viewport.offset - delta).clamp(Vec2::ZERO, drag.viewport.maximum),
            ));
        }
    }

    /// Apply only to the originating document, even if a keyboard shortcut
    /// changes tabs between raw input and rendering this frame.
    pub fn scroll_area(&mut self, id: Id, area: ScrollArea) -> ScrollArea {
        let Some((owner, offset)) = self.desired else {
            return area;
        };
        if owner != id {
            self.drag = None;
            self.desired = None;
            return area;
        }
        if self.drag.as_ref().is_none_or(|drag| drag.cancelled) {
            self.desired = None;
        }
        area.scroll_offset(offset)
    }

    /// Register the actual shared document scroller after Source/Live/Read/Split
    /// rendering. Sidebar and nested table scroll areas never own this gesture.
    pub fn viewport<R>(&mut self, output: &egui::scroll_area::ScrollAreaOutput<R>) {
        if self
            .drag
            .as_ref()
            .is_some_and(|drag| drag.viewport.id != output.id)
        {
            self.drag = None;
            self.desired = None;
        }
        self.viewport = Some(Viewport {
            id: output.id,
            rect: output.inner_rect,
            offset: output.state.clamped_offset(),
            maximum: (output.content_size - output.inner_rect.size()).max(Vec2::ZERO),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn viewport() -> Viewport {
        Viewport {
            id: Id::unique("test-document"),
            rect: Rect::from_min_max(egui::pos2(100.0, 20.0), egui::pos2(500.0, 400.0)),
            offset: egui::vec2(0.0, 200.0),
            maximum: egui::vec2(0.0, 800.0),
        }
    }

    fn button(pos: Pos2, button: PointerButton, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn raw(events: Vec<Event>) -> egui::RawInput {
        egui::RawInput {
            events,
            focused: true,
            ..Default::default()
        }
    }

    fn escape() -> Event {
        Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn note_pan_suppresses_primary_selection_paste_but_preserves_keyboard_paste_and_other_fields() {
        let ctx = egui::Context::default();
        let mut pan = PagePan {
            viewport: Some(viewport()),
            ..Default::default()
        };
        let inside = egui::pos2(200.0, 100.0);
        let outside = egui::pos2(2.0, 2.0);
        let other_field = Event::MiddleClickPaste {
            pos: outside,
            text: "other field".into(),
        };
        let keyboard = Event::Paste("keyboard paste".into());
        let mut input = raw(vec![
            button(inside, PointerButton::Middle, true),
            Event::MiddleClickPaste {
                pos: inside,
                text: "must not enter note".into(),
            },
            keyboard.clone(),
            other_field.clone(),
            Event::PointerMoved(inside + egui::vec2(0.0, 80.0)),
            button(inside, PointerButton::Middle, false),
        ]);
        pan.raw_input(&ctx, &mut input, true);
        assert!(input.events.contains(&keyboard));
        assert!(input.events.contains(&other_field));
        assert!(
            !input.events.iter().any(
                |event| matches!(event, Event::MiddleClickPaste { pos, .. } if *pos == inside)
            )
        );
    }

    #[test]
    fn middle_click_remains_a_click_and_preserves_keyboard_and_primary_events() {
        let ctx = egui::Context::default();
        let mut pan = PagePan {
            viewport: Some(viewport()),
            ..Default::default()
        };
        let pos = egui::pos2(200.0, 100.0);
        let events = vec![
            button(pos, PointerButton::Middle, true),
            Event::Text("unchanged".into()),
            button(pos, PointerButton::Middle, false),
            button(pos, PointerButton::Primary, true),
        ];
        let mut input = raw(events.clone());
        pan.raw_input(&ctx, &mut input, true);
        assert_eq!(input.events, events);
        assert!(pan.drag.is_none());
        assert!(pan.desired.is_none());
    }

    #[test]
    fn drag_tracks_release_position_and_does_not_route_scroll_to_hovered_sidebar() {
        let ctx = egui::Context::default();
        let owner = viewport();
        let mut pan = PagePan {
            viewport: Some(owner),
            ..Default::default()
        };
        let origin = egui::pos2(200.0, 100.0);
        let release = egui::pos2(50.0, 160.0);
        let mut input = raw(vec![
            button(origin, PointerButton::Middle, true),
            Event::PointerMoved(egui::pos2(180.0, 140.0)),
            button(release, PointerButton::Middle, false),
            Event::PointerMoved(egui::pos2(300.0, 300.0)),
        ]);
        pan.raw_input(&ctx, &mut input, true);
        assert_eq!(pan.desired, Some((owner.id, egui::vec2(0.0, 140.0))));
        assert!(pan.drag.is_none());
        assert!(
            !input
                .events
                .iter()
                .any(|event| matches!(event, Event::MouseWheel { .. }))
        );
        assert!(input.events.iter().any(|event| matches!(
            event,
            Event::PointerButton {
                button: PointerButton::Middle,
                pressed: false,
                ..
            }
        )));
        let _ = pan.scroll_area(owner.id, ScrollArea::vertical());
        assert!(pan.desired.is_none());
    }

    #[test]
    fn escape_restores_origin_and_disabled_or_changed_document_cancels_capture() {
        let ctx = egui::Context::default();
        let owner = viewport();
        let mut pan = PagePan {
            viewport: Some(owner),
            ..Default::default()
        };
        let origin = egui::pos2(200.0, 100.0);
        let mut input = raw(vec![
            button(origin, PointerButton::Middle, true),
            Event::PointerMoved(egui::pos2(200.0, 170.0)),
            escape(),
            Event::PointerMoved(egui::pos2(200.0, 250.0)),
        ]);
        pan.raw_input(&ctx, &mut input, true);
        assert_eq!(pan.desired, Some((owner.id, owner.offset)));
        assert!(input.events.contains(&escape()));
        let _ = pan.scroll_area(Id::unique("another-document"), ScrollArea::vertical());
        assert!(pan.drag.is_none());
        assert!(pan.desired.is_none());

        pan.viewport = Some(owner);
        let mut input = raw(vec![
            button(origin, PointerButton::Middle, true),
            Event::PointerMoved(egui::pos2(200.0, 180.0)),
        ]);
        pan.raw_input(&ctx, &mut input, true);
        assert!(pan.desired.is_some());
        pan.raw_input(&ctx, &mut raw(vec![]), false);
        assert!(pan.drag.is_none());
        assert!(pan.desired.is_none());
        assert!(pan.viewport.is_none());
    }

    #[test]
    fn real_scroll_area_pans_immediately_and_keeps_simple_middle_link_clicks() {
        fn frame(
            ctx: &egui::Context,
            pan: &mut PagePan,
            events: Vec<Event>,
            initial: bool,
        ) -> (f32, usize) {
            let mut offset = 0.0;
            let mut middle_clicks = 0;
            let mut input = raw(events);
            input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(600.0, 400.0)));
            pan.raw_input(ctx, &mut input, true);
            ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let id = ui.make_persistent_id(egui::IdSalt::new("page-pan-contract"));
                    let builder = ScrollArea::vertical()
                        .id_salt("page-pan-contract")
                        .auto_shrink([false, false]);
                    let builder = if initial {
                        builder.vertical_scroll_offset(200.0)
                    } else {
                        builder
                    };
                    let output = pan.scroll_area(id, builder).show(ui, |ui| {
                        ui.allocate_ui(egui::vec2(400.0, 1200.0), |ui| {
                            ui.set_min_size(egui::vec2(400.0, 1200.0));
                            let response = ui.interact(
                                ui.max_rect(),
                                ui.scope_id().with("link"),
                                egui::Sense::click(),
                            );
                            middle_clicks +=
                                usize::from(response.clicked_by(PointerButton::Middle));
                        });
                    });
                    assert_eq!(
                        output.id, id,
                        "The gesture owner must match the actual ScrollArea ID"
                    );
                    offset = output.state.clamped_offset().y;
                    pan.viewport(&output);
                });
            })
            .drop_without_applying_deltas();
            (offset, middle_clicks)
        }
        let ctx = egui::Context::default();
        let mut pan = PagePan::default();
        let mut middle_clicks = 0;
        let initial = frame(&ctx, &mut pan, vec![], true).0;
        assert!(
            (initial - 200.0).abs() < 1.0,
            "The fixture must actually overflow before panning: {initial}"
        );
        let origin = egui::pos2(100.0, 100.0);
        middle_clicks += frame(
            &ctx,
            &mut pan,
            vec![
                Event::PointerMoved(origin),
                button(origin, PointerButton::Middle, true),
            ],
            false,
        )
        .1;
        middle_clicks += frame(
            &ctx,
            &mut pan,
            vec![Event::PointerMoved(egui::pos2(100.0, 150.0))],
            false,
        )
        .1;
        let (offset, clicks) = frame(
            &ctx,
            &mut pan,
            vec![button(
                egui::pos2(100.0, 160.0),
                PointerButton::Middle,
                false,
            )],
            false,
        );
        middle_clicks += clicks;
        assert!((offset - 140.0).abs() < 1.0, "offset={offset}");
        assert_eq!(middle_clicks, 0);
        middle_clicks += frame(
            &ctx,
            &mut pan,
            vec![button(origin, PointerButton::Middle, true)],
            false,
        )
        .1;
        middle_clicks += frame(
            &ctx,
            &mut pan,
            vec![button(origin, PointerButton::Middle, false)],
            false,
        )
        .1;
        assert_eq!(middle_clicks, 1);
    }
}
