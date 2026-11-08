use rustidian::{assets, html_layout, vault::Vault};
use std::{fs, path::Path};

#[test]
fn readme_html_keeps_cell_layout_media_and_unicode_without_interpreting_code() {
    let source = "---\nexample: '<video src=property.mp4></video>'\n---\n\n`<video src=inline.mp4></video>`\n\n$<video src=math.mp4></video>$\n\n```html\n<table><tr><td>literal</td></tr></table>\n```\n\n<table><tr><th width='25%' align=right>عنوان</th><td width=180 valign=top><a href='https://example.com/?a=1&amp;b=2'><img src='photo.png' width=72 height=36 alt='A > B'></a><br><sub>Caption</sub></td></tr></table>\n\n<video controls><source src='clip.webm' type='video/webm'></video>\n";
    let blocks = html_layout::blocks(source);
    assert_eq!(
        blocks
            .iter()
            .map(|block| block.tag.name.as_str())
            .collect::<Vec<_>>(),
        ["table", "video"]
    );
    let rows = html_layout::table_rows(&source[blocks[0].range.clone()]);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].len(), 2);
    assert_eq!(rows[0][0].source, "عنوان");
    assert_eq!(rows[0][0].tag.length("width", 800.0), Some(200.0));
    assert_eq!(rows[0][1].tag.attribute("valign"), Some("top"));
    let images = assets::embedded_images(source);
    assert_eq!(images.len(), 1);
    assert_eq!(
        (images[0].width, images[0].height),
        (Some(72.0), Some(36.0))
    );
    assert_eq!(images[0].alt, "A > B");
    assert!(source[images[0].range.clone()].starts_with("<img "));
    assert_eq!(
        html_layout::decode("&نام‌بلند‌فارسی; &amp; &#x1f980;"),
        "&نام‌بلند‌فارسی; & 🦀"
    );
}

#[test]
fn picture_chooses_the_matching_theme_and_keeps_fallback_dimensions_and_alt() {
    let picture = "<picture><source media='(prefers-color-scheme: light)' srcset='light.png 1x, light-2x.png 2x'><source media='(prefers-color-scheme: dark)' srcset='dark.png'><img src='fallback.png' width='240' height='120' alt='Notebook'></picture>";
    for (dark, expected) in [(false, "light.png"), (true, "dark.png")] {
        let image = html_layout::picture_image(picture, dark).unwrap();
        assert_eq!(image.attribute("src"), Some(expected));
        assert_eq!(image.attribute("alt"), Some("Notebook"));
        assert_eq!(image.pixels("height"), Some(120.0));
    }
    let fallback = html_layout::picture_image(
        "<picture><source media='print' srcset='print.png'><img src='fallback.png'></picture>",
        false,
    )
    .unwrap();
    assert_eq!(fallback.attribute("src"), Some("fallback.png"));
}

#[test]
fn images_and_videos_resolve_from_the_note_and_vault_without_path_escapes() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("Notes")).unwrap();
    fs::create_dir_all(root.path().join("Assets")).unwrap();
    for name in [
        "Notes/Local.png",
        "Assets/Movie #1.MP4",
        "Assets/Clip.webm",
        "Assets/Clip.mov",
    ] {
        fs::write(root.path().join(name), b"media").unwrap();
    }
    fs::write(root.path().join("Notes/Note.md"), "# Note").unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let from = Path::new("Notes/Note.md");
    assert_eq!(
        vault.resolve_attachment(from, "Local.png"),
        Some(root.path().join("Notes/Local.png"))
    );
    for name in ["../Assets/Movie%20%231.MP4", "Assets/Clip.webm", "Clip.mov"] {
        let uri = assets::video_uri(Some(&vault), from, name).unwrap();
        assert!(uri.starts_with("file://"));
        assert!(assets::image_uri(Some(&vault), from, name).is_none());
    }
    assert!(
        assets::video_uri(Some(&vault), from, "Assets/Movie%20%231.MP4")
            .unwrap()
            .ends_with("Movie%20%231.MP4")
    );
    for name in ["../../outside.mp4", "/outside.mov", "missing.webm"] {
        assert!(
            assets::video_uri(Some(&vault), from, name).is_none(),
            "{name}"
        );
    }
    let guest = tempfile::tempdir().unwrap();
    fs::write(guest.path().join("README.md"), "# Guest").unwrap();
    fs::write(guest.path().join("Sibling.png"), b"image").unwrap();
    assert!(
        assets::image_uri(Some(&vault), &guest.path().join("README.md"), "Sibling.png").is_some()
    );
    assert!(
        assets::image_uri(
            Some(&vault),
            from,
            &format!("file://{}", guest.path().join("Sibling.png").display())
        )
        .is_none()
    );
}

#[test]
fn image_import_obeys_the_attachment_folder_and_never_replaces_a_collision() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".obsidian")).unwrap();
    fs::create_dir(root.path().join("Notes")).unwrap();
    fs::write(root.path().join("Notes/Note.md"), "# Note").unwrap();
    fs::write(
        root.path().join(".obsidian/app.json"),
        r#"{"attachmentFolderPath":"Assets/Pictures"}"#,
    )
    .unwrap();
    let mut vault = Vault::open(root.path()).unwrap();
    let from = Path::new("Notes/Note.md");
    let first = vault
        .import_attachment_from(from, Path::new("Drop.png"), b"first")
        .unwrap();
    let second = vault
        .import_attachment_from(from, Path::new("Drop.png"), b"second")
        .unwrap();
    assert_eq!(first, Path::new("Assets/Pictures/Drop.png"));
    assert_eq!(second, Path::new("Assets/Pictures/Drop 1.png"));
    assert_eq!(fs::read(root.path().join(first)).unwrap(), b"first");
    assert_eq!(fs::read(root.path().join(second)).unwrap(), b"second");
    assert!(assets::image_uri(Some(&vault), from, "Assets/Pictures/Drop.png").is_some());
    fs::write(
        root.path().join(".obsidian/app.json"),
        r#"{"attachmentFolderPath":"./Images"}"#,
    )
    .unwrap();
    assert_eq!(
        vault
            .import_attachment_from(from, Path::new("Local.png"), b"local")
            .unwrap(),
        Path::new("Notes/Images/Local.png")
    );
    fs::write(
        root.path().join(".obsidian/app.json"),
        r#"{"attachmentFolderPath":"../outside"}"#,
    )
    .unwrap();
    assert!(
        vault
            .import_attachment_from(from, Path::new("Bad.png"), b"bad")
            .is_err()
    );
}
