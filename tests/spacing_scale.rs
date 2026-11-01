//! The app chrome uses one spacing scale (`theme::space`). A raw number in a gap,
//! margin, padding or control height is a visual inconsistency waiting to happen, so it fails here.
//! Note text follows Obsidian's own metrics; those files, and lines marked
//! `// obsidian metric`, are exempt.
use std::fs;

const DOCUMENT: &[&str] = &[
    "preview.rs",
    "properties_ui.rs",
    "note_embed_ui.rs",
    "table_ui.rs",
    "table_layout.rs",
    "theme.rs",
];

#[test]
fn chrome_spacing_comes_from_the_shared_scale() {
    let raw = [
        r"add_space\(\s*[0-9]",
        r"Margin::(symmetric|same)\(\s*[0-9]",
        r"inner_margin\(\s*[1-9]",
        r"(item_spacing|button_padding)\s*=\s*egui::vec2\(\s*[0-9.]*[1-9]",
        r"(item_spacing|button_padding)\.[xy]\s*=\s*[0-9.]*[1-9]",
        // Control and row heights: theme::space::CONTROL / ROW, not a raw number.
        r"min_size\(egui::vec2\([^,]+,\s*[1-9][0-9]?(\.[0-9]+)?\)\)",
        r"add_sized\(\s*\[[^,]+,\s*[1-9][0-9]?(\.[0-9]+)?\]",
    ]
    .map(|pattern| regex::Regex::new(pattern).unwrap());
    let mut found = Vec::new();
    for entry in fs::read_dir("src").unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".rs") || DOCUMENT.contains(&name.as_str()) {
            continue;
        }
        for (number, line) in fs::read_to_string(&path).unwrap().lines().enumerate() {
            if !line.contains("// obsidian metric") && raw.iter().any(|re| re.is_match(line)) {
                found.push(format!("src/{name}:{}: {}", number + 1, line.trim()));
            }
        }
    }
    assert!(
        found.is_empty(),
        "Use theme::space for these:\n{}",
        found.join("\n")
    );
}
