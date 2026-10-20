use rust_notes::vault::Vault;
use std::{fs, path::Path};

#[test]
fn live_images_keep_unicode_source_ranges_sizes_and_code_exclusions() {
    use rust_notes::assets::embedded_images;
    let source = "فارسی 🦀\n\n![[Assets/Image.png|180x90]]\n\n![Caption](../Image.png)\n\n`![[Inline.png]]`\n\\![[Escaped.png]]\n```md\n![[Code.png]]\n```\n";
    let images = embedded_images(source);
    assert_eq!(images.len(), 2);
    assert_eq!(
        &source[images[0].range.clone()],
        "![[Assets/Image.png|180x90]]"
    );
    assert_eq!(images[0].target, "Assets/Image.png");
    assert_eq!(images[0].width, Some(180.0));
    assert_eq!(images[0].height, Some(90.0));
    assert_eq!(images[1].target, "../Image.png");
    assert_eq!(images[1].alt, "Caption");
    assert!(images[1].width.is_none());
}

#[test]
fn image_display_rewrites_inline_and_reference_images_but_leaves_code_unchanged() {
    use rust_notes::assets::prepare_images;
    let source = "![Caption|180](Assets/image.png)\n\n![Reference][photo]\n\n[photo]: Assets/image.png\n\n`![Example](Assets/image.png)`\n\n```md\n![Literal](Assets/image.png)\n```";
    let display = prepare_images(source, |target| {
        (target == "Assets/image.png").then(|| "file:///vault/Assets/image.png".into())
    });
    assert!(display.contains("![Caption|180](<file:///vault/Assets/image.png>)"));
    assert!(display.contains("![Reference](<file:///vault/Assets/image.png>)"));
    assert!(display.contains("`![Example](Assets/image.png)`"));
    assert!(display.contains("```md\n![Literal](Assets/image.png)\n```"));
    let blocked = prepare_images("![Private](file:///etc/secret.png)", |_| None);
    assert!(blocked.contains("rust-missing-image:file:///etc/secret.png"));
    let remote = prepare_images("![Remote](https://example.com/image.png)", |_| None);
    assert!(remote.contains("<https://example.com/image.png>"));
}

#[test]
fn images_resolve_relative_root_and_unique_names_without_escaping_the_vault() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join("Assets")).unwrap();
    fs::create_dir_all(temp.path().join("Notes")).unwrap();
    fs::write(temp.path().join("Notes/Note.md"), "# Note").unwrap();
    fs::write(temp.path().join("Assets/Hello world.PNG"), "fixture").unwrap();
    fs::write(temp.path().join("Assets/.hidden.png"), "hidden").unwrap();
    let vault = Vault::open(temp.path()).unwrap();
    for target in [
        "../Assets/Hello%20world.PNG",
        "Assets/Hello world.PNG",
        "Hello world.PNG|300",
    ] {
        assert_eq!(
            vault
                .resolve_attachment(Path::new("Notes/Note.md"), target)
                .unwrap(),
            temp.path().join("Assets/Hello world.PNG")
        );
    }
    for target in [
        "../../outside.png",
        "/etc/hosts",
        "https://example.com/image.png",
        "Assets/.hidden.png",
        "Missing.png",
    ] {
        assert!(
            vault
                .resolve_attachment(Path::new("Notes/Note.md"), target)
                .is_none(),
            "{target}"
        );
    }
    fs::write(temp.path().join("Notes/Hello world.PNG"), "second").unwrap();
    let vault = Vault::open(temp.path()).unwrap();
    assert!(
        vault
            .resolve_attachment(Path::new("Other.md"), "Hello world.PNG")
            .is_none()
    );
    assert_eq!(
        vault
            .resolve_attachment(Path::new("Notes/Note.md"), "Hello world.PNG")
            .unwrap(),
        temp.path().join("Notes/Hello world.PNG")
    );
}

#[cfg(unix)]
#[test]
fn attachment_resolution_rechecks_symlinks_after_indexing() {
    let temp = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("Outside.png"), "outside").unwrap();
    fs::write(temp.path().join("Inside.png"), "inside").unwrap();
    let vault = Vault::open(temp.path()).unwrap();
    fs::remove_file(temp.path().join("Inside.png")).unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("Outside.png"),
        temp.path().join("Inside.png"),
    )
    .unwrap();
    assert!(
        vault
            .resolve_attachment(Path::new("Note.md"), "Inside.png")
            .is_none()
    );
}
