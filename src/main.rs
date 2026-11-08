mod app;
mod callout_ui;
mod code_ui;
mod diagrams;
mod dictation;
mod editor_ui;
mod explorer_selection;
mod file_move_ui;
mod file_tree;
mod graph_animation;
mod graph_ui;
mod image_view;
#[cfg(unix)]
mod instance;
mod math_inline;
mod math_ui;
mod media_ui;
mod note_embed_ui;
mod page_pan;
mod page_preview;
mod preview;
mod properties_ui;
mod read_aloud;
mod recovery_ui;
mod session;
mod svg_loader;
mod syntax;
mod table_layout;
mod table_ui;
mod terminal_ui;
mod theme;
mod title_ui;
mod ui_probe;
mod vault_manager;
mod window;
mod workspace_ui;

use rustidian::vaults::resolve_input;
use std::{path::PathBuf, time::Instant};

fn main() -> eframe::Result {
    let started = Instant::now();
    #[cfg(target_os = "linux")]
    if window::use_x11_for_drops(
        std::env::var_os("RUSTIDIAN_WAYLAND").is_some(),
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
    ) {
        // SAFETY: this runs first in `main`, before any thread starts.
        unsafe { std::env::remove_var("WAYLAND_DISPLAY") };
        eprintln!("backend: X11 (file drops reach only X11 windows)");
    }
    let mut arguments = std::env::args_os().skip(1);
    let first = arguments.next();
    if first
        .as_deref()
        .is_some_and(|arg| arg == "--version" || arg == "-V")
    {
        println!("{} {}", rustidian::APP_NAME, env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    if first
        .as_deref()
        .is_some_and(|arg| arg == "--help" || arg == "-h")
    {
        println!(
            "{} — native Markdown notebook and text editor\n\nUsage: rustidian [VAULT_FOLDER | FILE]\n       rustidian --check-vault VAULT_FOLDER\n\nOpen Markdown, plain text and supported source-code files. RUSTIDIAN_DEFAULT_VAULT selects the default vault; otherwise opens the included demo vault. Ctrl+P opens commands.",
            rustidian::APP_NAME
        );
        return Ok(());
    }
    if first.as_deref() == Some(std::ffi::OsStr::new("--check-vault")) {
        let Some(root) = arguments.next() else {
            eprintln!("Pass a vault folder after --check-vault");
            std::process::exit(2);
        };
        match rustidian::vault::Vault::open(&PathBuf::from(root)) {
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
    // Set once the window exists; launches handed over before then wait in
    // the queue for its first pass.
    #[cfg(unix)]
    let ui_context = std::sync::Arc::<std::sync::OnceLock<eframe::egui::Context>>::default();
    #[cfg(unix)]
    let launches = {
        let requested = first
            .as_ref()
            .map(|input| std::path::absolute(input).unwrap_or_else(|_| PathBuf::from(input)));
        match instance::launch(requested.as_deref()) {
            instance::Launch::HandedOff => return Ok(()),
            instance::Launch::Serve(instance) => {
                let ui_context = ui_context.clone();
                Some(instance.serve(move || {
                    let opened = ui_context.get();
                    if let Some(ctx) = opened {
                        ctx.request_repaint();
                    }
                    opened.is_some()
                }))
            }
            instance::Launch::Alone => None,
        }
    };
    let default_vault = rustidian::vaults::default_vault();
    // A launch without a file or folder reopens the vault Masih last read, and
    // its page; only when none is remembered does the default vault open.
    let remembered = first
        .is_none()
        .then(rustidian::vaults::last_opened)
        .flatten();
    let input = first
        .map(PathBuf::from)
        .or_else(|| remembered.as_ref().map(|(root, _)| root.clone()))
        .or_else(|| default_vault.clone())
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|executable| executable.parent().map(|parent| parent.join("demo-vault")))
                .filter(|vault| vault.is_dir())
                .unwrap_or_else(|| PathBuf::from("demo-vault"))
        });
    let registered = if input.is_file() {
        rustidian::vaults::Vaults::open().ok()
    } else {
        None
    };
    let roots = registered.as_ref().map_or(&[][..], |list| list.roots());
    let (root, startup_document) = match resolve_input(&input, default_vault.as_deref(), roots) {
        Ok((root, document)) => {
            let page = document.or_else(|| {
                remembered
                    .as_ref()
                    .filter(|(remembered_root, _)| *remembered_root == root)
                    .and_then(|(_, page)| page.clone())
            });
            (root, page)
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title(rustidian::APP_NAME)
            .with_app_id("rustidian")
            .with_decorations(false)
            .with_inner_size([1380.0, 900.0])
            .with_min_inner_size([860.0, 560.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    let result = eframe::run_native(
        rustidian::APP_NAME,
        options,
        Box::new(move |cc| {
            window::close_on_signals(cc.egui_ctx.clone());
            #[cfg(unix)]
            let _ = ui_context.set(cc.egui_ctx.clone());
            let mut app = app::NotesApp::new(cc, &root, started, startup_document);
            #[cfg(unix)]
            {
                app.launches = launches;
            }
            Ok(Box::new(app))
        }),
    );
    // The socket file stays: the next launch finds nobody listening on it and
    // replaces it. Removing it here could remove a newer window's socket.
    // The window is gone; an agent that ignores SIGTERM still gets its SIGKILL.
    rustidian::terminal::wait_for_closing();
    result
}
