//! CPU UI/layout measurement; run the release executable without other workloads.
#[allow(dead_code)]
#[path = "../src/file_tree.rs"]
mod file_tree;
#[allow(dead_code)]
#[path = "../src/theme.rs"]
mod theme;
#[path = "../src/ui_probe.rs"]
#[allow(dead_code)]
mod ui_probe;

use eframe::egui;
use rust_notes::vault::Vault;
use std::{fs, time::Instant};

fn main() {
    let mut conditions = Vec::new();
    for count in [100, 1000, 5000] {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("Bulk")).unwrap();
        fs::write(root.path().join("Home.md"), "# Home\n").unwrap();
        for number in 0..count {
            fs::write(
                root.path().join(format!("Bulk/Note{number:05}.md")),
                format!("# Note{number:05}\n\nShort content.\n"),
            )
            .unwrap();
        }
        let vault = Vault::open(root.path()).unwrap();
        let tree = file_tree::Tree::from_vault(&vault);
        let active = std::path::PathBuf::from(format!("Bulk/Note{:05}.md", count - 1));
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        theme::apply(&ctx, true);
        let mut samples = Vec::new();
        for frame in 0..25 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 960.0),
                )),
                time: Some(frame as f64 / 60.0),
                ..Default::default()
            };
            let started = Instant::now();
            let output = ctx.run_ui(input, |ui| {
                egui::Panel::left("explorer")
                    .exact_size(300.0)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        egui::ScrollArea::vertical()
                            .id_salt("file-tree")
                            .show(ui, |ui| {
                                std::hint::black_box(tree.show(
                                    ui,
                                    "",
                                    Some(&active),
                                    false,
                                    0,
                                    None,
                                ));
                            });
                    });
            });
            let ui_us = started.elapsed().as_micros();
            let started = Instant::now();
            let meshes = ctx.tessellate(output.shapes, output.pixels_per_point);
            std::hint::black_box(&meshes);
            if frame >= 5 {
                samples.push(serde_json::json!({"ui_us": ui_us, "tessellate_us": started.elapsed().as_micros(), "meshes": meshes.len()}));
            }
        }
        conditions.push(serde_json::json!({"notes_in_bulk": count, "samples": samples}));
    }
    println!("{}", serde_json::to_string_pretty(&serde_json::json!({
        "method": "release headless egui context UI/layout and tessellation; native Inter/theme; one expanded folder; five warmups then twenty measured frames per condition; 1440x960 viewport, 300px explorer",
        "corpus_recipe": "Home.md plus Bulk/Note{number:05}.md, each '# Note{number:05}\\n\\nShort content.\\n'",
        "conditions": conditions,
    })).unwrap());
}
