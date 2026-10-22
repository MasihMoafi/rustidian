//! Exercise fold state through the Reading renderer and real egui pointer events.
use eframe::egui;
use egui_commonmark::CommonMarkCache;
use rust_notes::{tab::Tab, vault::Document};

fn document(path: &str, text: &str) -> Document {
    let mut document = Document::new(path.into());
    document.text = text.into();
    document
}

fn text_shapes(shape: &egui::Shape, result: &mut Vec<(String, egui::Rect)>) {
    match shape {
        egui::Shape::Text(text) => result.push((
            text.galley.job.text.clone(),
            text.galley.rect.translate(text.pos.to_vec2()),
        )),
        egui::Shape::Vec(children) => {
            for child in children {
                text_shapes(child, result);
            }
        }
        _ => {}
    }
}

fn frame(
    ctx: &egui::Context,
    tab: &mut Tab,
    events: Vec<egui::Event>,
) -> Vec<(String, egui::Rect)> {
    let mut cache = CommonMarkCache::default();
    let output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 700.0),
            )),
            events,
            ..Default::default()
        },
        |ui| {
            super::show(ui, &mut cache, tab, None, None);
        },
    );
    let mut result = Vec::new();
    for shape in &output.shapes {
        text_shapes(&shape.shape, &mut result);
    }
    output.drop_without_applying_deltas();
    result
}

#[test]
fn reading_callout_folds_follow_the_document_when_a_tab_navigates() {
    let first = "> [!note]+ First\n> FIRST_BODY\n>\n> > [!tip]+ Nested\n> > NESTED_BODY\n";
    let second = "> [!warning]- Second\n> SECOND_BODY\n";
    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    egui_extras::install_image_loaders(&ctx);
    let mut tab = Tab::new(document("first.md", first));
    let rendered = frame(&ctx, &mut tab, vec![]);
    assert!(rendered.iter().any(|(text, _)| text == "FIRST_BODY"));
    assert!(rendered.iter().any(|(text, _)| text == "NESTED_BODY"));
    let nested = rendered
        .iter()
        .find(|(text, _)| text == "Nested")
        .expect("The nested callout must expose its actual rendered title")
        .1
        .center();
    for pressed in [true, false] {
        frame(
            &ctx,
            &mut tab,
            vec![
                egui::Event::PointerMoved(nested),
                egui::Event::PointerButton {
                    pos: nested,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    let rendered = frame(&ctx, &mut tab, vec![]);
    assert!(!rendered.iter().any(|(text, _)| text == "NESTED_BODY"));
    assert!(rendered.iter().any(|(text, _)| text == "FIRST_BODY"));

    // Real navigation reuses the tab ID. A tab-only key still leaks the first
    // document's open callout state into the second document's closed default.
    tab.replace(document("second.md", second));
    let rendered = frame(&ctx, &mut tab, vec![]);
    assert!(rendered.iter().any(|(text, _)| text == "Second"));
    assert!(!rendered.iter().any(|(text, _)| text == "SECOND_BODY"));

    tab.replace(document("first.md", first));
    let rendered = frame(&ctx, &mut tab, vec![]);
    assert!(rendered.iter().any(|(text, _)| text == "FIRST_BODY"));
    assert!(rendered.iter().any(|(text, _)| text == "Nested"));
    assert!(!rendered.iter().any(|(text, _)| text == "NESTED_BODY"));
    assert_eq!(tab.document.text, first);
}
