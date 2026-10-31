//! CPU editor experiment; run release with no GUI/build/reference in parallel.
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/callout_ui.rs"]
mod callout_ui;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/code_ui.rs"]
mod code_ui;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/diagrams.rs"]
mod diagrams;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/editor_ui.rs"]
mod editor_ui;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/math_inline.rs"]
mod math_inline;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/math_ui.rs"]
mod math_ui;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/page_preview.rs"]
mod page_preview;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/preview.rs"]
mod preview;
#[cfg(not(test))]
#[path = "../src/syntax.rs"]
mod syntax;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/table_layout.rs"]
mod table_layout;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/table_ui.rs"]
mod table_ui;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/theme.rs"]
mod theme;
#[cfg(not(test))]
#[allow(dead_code)]
#[path = "../src/ui_probe.rs"]
mod ui_probe;

#[cfg(not(test))]
fn main() {
    use eframe::egui::{
        self,
        text::{CCursor, CCursorRange},
        widgets::text_edit::TextEditState,
    };
    use rustidian::{search::FileMatcher, tab::Tab, vault::Vault};
    use std::{fs, path::Path, time::Instant};

    assert!(std::env::var_os("RUSTIDIAN_UI_PROBE").is_none());
    let arguments: Vec<_> = std::env::args().skip(1).collect();
    // `--note PATH` times typing at the end of one real note instead of the synthetic corpus.
    let note = arguments
        .iter()
        .position(|argument| argument == "--note")
        .map(|index| arguments[index + 1].clone());
    assert!(
        arguments
            .iter()
            .enumerate()
            .all(|(index, argument)| matches!(
                argument.as_str(),
                "--diverse" | "--folded" | "--note" | "--edits"
            ) || index > 0
                && matches!(arguments[index - 1].as_str(), "--note" | "--edits")),
        "Use --diverse, --folded, --note PATH and/or --edits N"
    );
    // `--edits N` types N characters (default 25) so a sampler sees enough frames.
    let edits: usize = arguments
        .iter()
        .position(|argument| argument == "--edits")
        .map_or(25, |index| arguments[index + 1].parse().unwrap());
    let diverse = arguments.iter().any(|argument| argument == "--diverse");
    let folded = arguments.iter().any(|argument| argument == "--folded");
    let mut conditions = Vec::new();
    let counts: Vec<usize> = if note.is_some() {
        vec![0]
    } else {
        vec![20, 200, 500]
    };
    for count in counts {
        let mut source = String::from("# Editor benchmark\n\n");
        if let Some(note) = &note {
            source = fs::read_to_string(note).unwrap();
        }
        for index in 0..count {
            let body = if diverse {
                format!("body π {index}")
            } else {
                "body π".into()
            };
            source.push_str(&format!(
                "### Section {index}\n\n> A quote with **emphasis** and unicode π.\n\n~~~rust\nfn main() {{\n    let value = \"{body}\";\n    // Preserve literal **markdown** [[No link]]\n}}\n~~~\n\nPlain paragraph for a real document.\n\n"
            ));
        }
        let fixture = tempfile::tempdir().unwrap();
        fs::write(fixture.path().join("Benchmark.md"), &source).unwrap();
        let vault = Vault::open(fixture.path()).unwrap();
        let mut files = FileMatcher::from_notes(&vault.notes);
        let mut tab = Tab::new(vault.read(Path::new("Benchmark.md")).unwrap());
        let mut editor = editor_ui::Editor::default();
        let mut find = editor_ui::Find::default();
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        theme::apply(&ctx, true);
        let id = egui::Id::unique("benchmark-editor");
        if folded {
            let sections = rustidian::folding::Sections::new(
                &source,
                &rustidian::editor::Markdown::parse(&source),
            );
            let collapsed: std::collections::HashSet<String> = sections
                .0
                .into_iter()
                .filter(|section| section.level == 3)
                .skip(1)
                .map(|section| section.key)
                .collect();
            assert_eq!(collapsed.len(), count - 1);
            ctx.data_mut(|data| data.insert_temp(id.with("editing-folds"), collapsed));
        }
        let insertion = if note.is_some() {
            source.len()
        } else {
            source.find("let value").unwrap() + 4
        };
        let character = source[..insertion].chars().count();
        let mut state = TextEditState::default();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(character))));
        state.store(&ctx, id);
        ctx.memory_mut(|memory| memory.request_focus(id));
        let mut cold = None;
        let mut stable = Vec::new();
        let mut edited = Vec::new();
        for frame in 0..26 + edits {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 960.0),
                )),
                time: Some(frame as f64 / 60.0),
                events: if frame >= 26 {
                    vec![egui::Event::Text("x".into())]
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let started = Instant::now();
            let mut output = ctx.run_ui(input, |ui| {
                ui.scope_builder(
                    egui::UiBuilder::new().max_rect(egui::Rect::from_min_size(
                        egui::pos2(426.0, 150.0),
                        egui::vec2(932.0, 780.0),
                    )),
                    |ui| {
                        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                        ui.spacing_mut().scroll.bar_width = 6.0;
                        ui.spacing_mut().scroll.bar_inner_margin = 4.0;
                        ui.spacing_mut().scroll.bar_outer_margin = 2.0;
                        egui::ScrollArea::vertical()
                            .id_salt("benchmark-document")
                            .auto_shrink([false, false])
                            .scroll_bar_visibility(
                                egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                            )
                            .show(ui, |ui| {
                                std::hint::black_box(editor.show(
                                    ui,
                                    &mut tab,
                                    id,
                                    &mut find,
                                    &mut files,
                                    Some(&vault),
                                ));
                            });
                    },
                );
            });
            let ui_us = started.elapsed().as_micros();
            let started = Instant::now();
            let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
            std::hint::black_box(&meshes);
            let sample = serde_json::json!({
                "ui_us": ui_us,
                "tessellate_us": started.elapsed().as_micros(),
                "meshes": meshes.len(),
            });
            output.textures_delta.clear();
            match frame {
                0 => cold = Some(sample),
                6..=25 => stable.push(sample),
                31.. => edited.push(sample),
                _ => {}
            }
        }
        let expected = format!(
            "{}{}{}",
            &source[..insertion],
            "x".repeat(edits),
            &source[insertion..]
        );
        assert_eq!(
            tab.document.text, expected,
            "Actual text-input path must preserve every other byte"
        );
        conditions.push(serde_json::json!({
            "blocks": count,
            "source_bytes": source.len(),
            "original_characters": source.chars().count(),
            "insertions_verified": edits,
            "cold": cold,
            "stable": stable,
            "edited": edited,
        }));
    }
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "method": "Native Editor::show; Inter/DejaVu/dark; 1440x960 viewport; 920x780 document inside solid 12px vertical scroll allocation. First frame separate, five static warmups, twenty stable frames, five text-input warmups, twenty text-input samples. UI and tessellation CPU times in microseconds. No OS/GPU presentation measurement.",
        "corpus_mode": if diverse { "diverse" } else { "repeated" },
        "diverse_recipe_adjustment": "With --diverse, each code literal is 'body π {index}' instead of 'body π', making every code block distinct. Everything else stays identical.",
        "corpus_recipe": "Header '# Editor benchmark\\n\\n', then for each index: '### Section {index}\\n\\n> A quote with **emphasis** and unicode π.\\n\\n~~~rust\\nfn main() {\\n    let value = \"body π\";\\n    // Preserve literal **markdown** [[No link]]\\n}\\n~~~\\n\\nPlain paragraph for a real document.\\n\\n'. Insert25 x characters immediately after the first 'let '.",
        "conditions": conditions,
        "folded_sections": if folded { "All H3 sections except the first; input edits the visible first code block" } else { "None" },
    })).unwrap());
}

#[cfg(test)]
fn main() {}
