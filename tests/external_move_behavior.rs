use rustidian::vault::Vault;
use std::{fs, path::Path};

#[test]
fn a_note_moved_outside_the_app_is_found_by_name_text_and_path_ending() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let old = Path::new("sepehr/repo/specs/plan.md");
    fs::create_dir_all(root.join("hack/sepehr/repo/specs")).unwrap();
    fs::create_dir_all(root.join("hack/specs")).unwrap();
    // The moved copy and an identical copy elsewhere: the longer shared ending wins.
    fs::write(root.join("hack/sepehr/repo/specs/plan.md"), "# Plan\n").unwrap();
    fs::write(root.join("hack/specs/plan.md"), "# Plan\n").unwrap();
    fs::write(root.join("other.md"), "# Plan\n").unwrap();
    let vault = Vault::open(root).unwrap();
    assert_eq!(
        vault.find_moved(old, "# Plan\n").as_deref(),
        Some(Path::new("hack/sepehr/repo/specs/plan.md"))
    );
    // Different text is a different note, not the moved one.
    assert_eq!(vault.find_moved(old, "# Edited\n"), None);
    // Two equally good copies: no guess.
    assert_eq!(
        vault.find_moved(Path::new("gone/plan.md"), "# Plan\n"),
        None
    );
}
