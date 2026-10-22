mod app;
mod callout_ui;
mod code_ui;
mod diagrams;
mod editor_ui;
mod file_move_ui;
mod file_tree;
mod graph_ui;
mod image_view;
mod page_preview;
mod preview;
mod properties_ui;
mod recovery_ui;
mod session;
mod syntax;
mod table_ui;
mod theme;
mod title_ui;
mod ui_probe;
mod vault_manager;
mod window;
mod workspace_ui;

use rust_notes::vaults::resolve_input;
use std::{path::PathBuf, time::Instant};

fn main() -> eframe::Result {
    let started = Instant::now();
    let mut arguments = std::env::args_os().skip(1);
    let first = arguments.next();
    if first.as_deref() == Some(std::ffi::OsStr::new("--help")) {
        println!(
            "{} — native Markdown notebook\n\nUsage: rust-notes [VAULT_FOLDER | FILE.md | FILE.txt]\n       rust-notes --check-vault VAULT_FOLDER\n\nRUST_NOTES_DEFAULT_VAULT selects the default vault; otherwise opens the included demo vault. Ctrl+P opens commands.",
            rust_notes::APP_NAME
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
    let default_vault = std::env::var_os("RUST_NOTES_DEFAULT_VAULT").map(PathBuf::from);
    let input = first
        .map(PathBuf::from)
        .or_else(|| default_vault.clone())
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|executable| executable.parent().map(|parent| parent.join("demo-vault")))
                .filter(|vault| vault.is_dir())
                .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("demo-vault"))
        });
    let registered = if input.is_file() {
        rust_notes::vaults::Vaults::open().ok()
    } else {
        None
    };
    let roots = registered.as_ref().map_or(&[][..], |list| list.roots());
    let (root, startup_document) = match resolve_input(&input, default_vault.as_deref(), roots) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(rust_notes::APP_NAME)
            .with_app_id("rust-notes")
            .with_decorations(false)
            .with_inner_size([1380.0, 900.0])
            .with_min_inner_size([860.0, 560.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        rust_notes::APP_NAME,
        options,
        Box::new(move |cc| {
            Ok(Box::new(app::NotesApp::new(
                cc,
                &root,
                started,
                startup_document,
            )))
        }),
    )
}
