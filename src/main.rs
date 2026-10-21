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

use std::{
    path::{Path, PathBuf},
    time::Instant,
};

fn resolve_input(
    input: &Path,
    default_vault: Option<&Path>,
) -> Result<(PathBuf, Option<PathBuf>), String> {
    let absolute = input
        .canonicalize()
        .map_err(|error| format!("Cannot open {}: {error}", input.display()))?;
    if absolute.is_dir() {
        return Ok((absolute, None));
    }
    if !absolute.is_file() || !rust_notes::vault::is_text_document(&absolute) {
        return Err("Choose a vault folder, Markdown file (.md) or plain-text file (.txt).".into());
    }
    let root = default_vault
        .and_then(|vault| vault.canonicalize().ok())
        .filter(|vault| vault.is_dir() && absolute.starts_with(vault))
        .or_else(|| absolute.parent().map(Path::to_path_buf))
        .ok_or("Cannot determine the document's folder.")?;
    let document = absolute
        .strip_prefix(&root)
        .map_err(|error| error.to_string())?
        .to_owned();
    Ok((root, Some(document)))
}

fn main() -> eframe::Result {
    let started = Instant::now();
    let mut arguments = std::env::args_os().skip(1);
    let first = arguments.next();
    if first.as_deref() == Some(std::ffi::OsStr::new("--help")) {
        println!(
            "Rust Notes — native Markdown notebook\n\nUsage: rust-notes [VAULT_FOLDER | FILE.md | FILE.txt]\n       rust-notes --check-vault VAULT_FOLDER\n\nRUST_NOTES_DEFAULT_VAULT selects the default vault; otherwise opens the included demo vault. Ctrl+P opens commands."
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
    let (root, startup_document) = match resolve_input(&input, default_vault.as_deref()) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Rust Notes")
            .with_app_id("rust-notes")
            .with_inner_size([1380.0, 900.0])
            .with_min_inner_size([860.0, 560.0]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Rust Notes",
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

#[cfg(test)]
mod tests {
    use super::resolve_input;
    use std::{fs, path::Path};

    #[test]
    fn file_input_selects_the_requested_document_and_its_folder() {
        let folder = tempfile::tempdir().unwrap();
        for filename in ["Read me سلام.md", "Plain text.TXT"] {
            let path = folder.path().join(filename);
            fs::write(&path, "Original content").unwrap();
            let (root, document) = resolve_input(&path, None).unwrap();
            assert_eq!(root, folder.path().canonicalize().unwrap());
            assert_eq!(document.as_deref(), Some(Path::new(filename)));
            assert_eq!(fs::read_to_string(&path).unwrap(), "Original content");
        }
        assert!(resolve_input(&folder.path().join("Missing.md"), None).is_err());
        let unsupported = folder.path().join("Not a text document.pdf");
        fs::write(&unsupported, "%PDF-1.7").unwrap();
        assert!(resolve_input(&unsupported, None).is_err());
    }

    #[test]
    fn files_inside_the_default_vault_keep_that_root() {
        let folder = tempfile::tempdir().unwrap();
        fs::create_dir(folder.path().join("Nested")).unwrap();
        let path = folder.path().join("Nested/Note.md");
        fs::write(&path, "A note").unwrap();
        let (root, document) = resolve_input(&path, Some(folder.path())).unwrap();
        assert_eq!(root, folder.path().canonicalize().unwrap());
        assert_eq!(document.as_deref(), Some(Path::new("Nested/Note.md")));
        let outside = tempfile::tempdir().unwrap();
        let (_, document) = resolve_input(&path, Some(outside.path())).unwrap();
        assert_eq!(document.as_deref(), Some(Path::new("Note.md")));
        let (root, document) = resolve_input(folder.path(), None).unwrap();
        assert_eq!(root, folder.path().canonicalize().unwrap());
        assert!(document.is_none());
    }
}
