use rustidian::{
    html_import,
    tab::{Tab, View},
    vault::Vault,
};
use std::{fs, path::Path};
use tempfile::tempdir;

#[test]
fn html_becomes_editable_markdown_with_structure_and_without_page_markup() {
    let html = r#"<!doctype html><html><head><title>Hidden title</title>
        <style>body { color: red }</style><script>HIDDEN_SCRIPT</script></head>
        <body><main><h1>Research &amp; notes</h1><p>Hello <strong>world</strong> — فارسی.</p>
        <ul><li>First</li><li><em>Second</em></li></ul>
        <a href="../References.md">Reference</a><img src="image.png" alt="A figure">
        <table><tr><th>Method</th><th>Result</th></tr><tr><td>A</td><td>Works</td></tr></table>
        <pre><code class="language-rust">let x = 1 &lt; 2;</code></pre>
        </main></body></html>"#;
    let markdown = html_import::convert(html).unwrap();
    for expected in [
        "# Research & notes",
        "**world**",
        "فارسی",
        "Second",
        "[Reference](../References.md)",
        "![A figure](image.png)",
        "| Method",
        "| Result",
        "1 < 2",
    ] {
        assert!(
            markdown.contains(expected),
            "Missing {expected}: {markdown}"
        );
    }
    assert!(pulldown_cmark::Parser::new(&markdown).any(|event| matches!(
        event,
        pulldown_cmark::Event::Start(pulldown_cmark::Tag::Emphasis)
    )));
    for unwanted in [
        "<html",
        "<body",
        "<main",
        "<table",
        "<style",
        "<script",
        "Hidden title",
        "HIDDEN_SCRIPT",
        "color: red",
    ] {
        assert!(
            !markdown.contains(unwanted),
            "Page markup leaked: {markdown}"
        );
    }
}

#[test]
fn import_preserves_html_and_existing_notes_and_saves_edits_as_markdown() {
    let folder = tempdir().unwrap();
    fs::create_dir(folder.path().join("Notes")).unwrap();
    let source = Path::new("Notes/Research.HTML");
    let html = "<h1>Research</h1><p>Keep the original.</p>\r\n";
    fs::write(folder.path().join(source), html).unwrap();
    fs::write(folder.path().join("Notes/Research.md"), "KEEP_EXISTING\n").unwrap();
    let mut vault = Vault::open(folder.path()).unwrap();
    let mut original = vault.open_document(source).unwrap();
    original.text.push_str("<p>Current unsaved edit.</p>");
    let imported = vault.import_html(&original).unwrap();
    assert_eq!(imported.path, Path::new("Notes/Research 1.md"));
    assert!(imported.text.contains("Current unsaved edit."));
    assert!(!imported.dirty());
    assert!(!imported.is_plain_text());
    let mut tab = Tab::new(imported);
    assert_eq!(tab.view, View::Live);
    tab.document.text.push_str("\nAn editable paragraph.\n");
    vault.save(&mut tab.document).unwrap();
    assert!(
        fs::read_to_string(folder.path().join(&tab.document.path))
            .unwrap()
            .contains("An editable paragraph.")
    );
    assert_eq!(
        fs::read_to_string(folder.path().join(source)).unwrap(),
        html
    );
    assert_eq!(
        fs::read_to_string(folder.path().join("Notes/Research.md")).unwrap(),
        "KEEP_EXISTING\n"
    );
    assert!(
        original.dirty(),
        "Import must retain unsaved source edits in their source tab"
    );
}

#[test]
fn empty_or_non_html_imports_do_not_create_notes() {
    let folder = tempdir().unwrap();
    fs::write(
        folder.path().join("Only scripts.html"),
        "<html><head><script>hidden</script><style>p{}</style></head><body></body></html>",
    )
    .unwrap();
    fs::write(folder.path().join("Literal.txt"), "<p>Literal text</p>").unwrap();
    let mut vault = Vault::open(folder.path()).unwrap();
    for source in ["Only scripts.html", "Literal.txt"] {
        let document = vault.open_document(Path::new(source)).unwrap();
        assert!(vault.import_html(&document).is_err());
        assert!(
            !folder
                .path()
                .join(Path::new(source).with_extension("md"))
                .exists()
        );
    }
    assert!(html_import::is_html(Path::new("page.HTM")));
    assert!(html_import::is_html(Path::new("page.html")));
    assert!(!html_import::is_html(Path::new("page.md")));
}
