mod app;
mod callout_ui;
mod code_ui;
mod editor_ui;
mod file_tree;
mod image_view;
mod preview;
mod properties_ui;
mod session;
mod syntax;
mod table_ui;
mod theme;
mod ui_probe;

use std::{path::PathBuf, time::Instant};

fn main() -> eframe::Result {
    let started = Instant::now();
    let mut arguments = std::env::args_os().skip(1);
    let first = arguments.next();
    if first.as_deref() == Some(std::ffi::OsStr::new("--help")) {
        println!(
            "Rust Notes — native Markdown notebook\n\nUsage: rust-notes [VAULT_FOLDER]\n       rust-notes --check-vault VAULT_FOLDER\n\nWithout a folder, opens the included demo vault. Ctrl+P opens commands."
        );
        return Ok(());
    }
    if first.as_deref() == Some(std::ffi::OsStr::new("--check-vault")) {
        let Some(root) = arguments.next() else {
            eprintln!("Pass a vault folder after --check-vault");
            std::process::exit(2);
        };
        match rust_notes::vault::Vault::open(&PathBuf::from(root)) {
            Ok(vault) => {
                println!(
                    "vault={} notes={} index_us={} warnings={}",
                    vault.root.display(),
                    vault.notes.len(),
                    vault.index_time.as_micros(),
                    vault.warnings.len()
                );
                for warning in &vault.warnings {
                    eprintln!("{warning}");
                }
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return Ok(());
    }
    let root = first.map(PathBuf::from).unwrap_or_else(|| {
        std::env::current_exe()
            .ok()
            .and_then(|executable| executable.parent().map(|parent| parent.join("demo-vault")))
            .filter(|vault| vault.is_dir())
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("demo-vault"))
    });
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Rust Notes")
            .with_inner_size([1380.0, 900.0])
            .with_min_inner_size([860.0, 560.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Rust Notes",
        options,
        Box::new(move |cc| Ok(Box::new(app::NotesApp::new(cc, &root, started)))),
    )
}
