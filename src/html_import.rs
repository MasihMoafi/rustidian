//! Convert an HTML page into an ordinary, editable Markdown note.
use std::path::Path;

pub fn is_html(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("html") || extension.eq_ignore_ascii_case("htm")
    })
}

pub fn convert(html: &str) -> crate::vault::Result<String> {
    let markdown = htmd::HtmlToMarkdown::builder()
        .skip_tags(vec![
            "head", "script", "style", "template", "iframe", "object",
        ])
        .build()
        .convert(html)
        .map_err(|error| format!("Cannot convert HTML to Markdown: {error}"))?;
    if markdown.trim().is_empty() {
        return Err("This HTML file has no editable body content.".into());
    }
    Ok(format!("{}\n", markdown.trim()))
}
