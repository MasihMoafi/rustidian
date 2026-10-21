use chrono::{DateTime, FixedOffset};
use rust_notes::{
    note_aids::{Config, moment_format},
    vault::Vault,
};
use std::{fs, path::Path};

fn clock() -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339("2026-10-21T13:05:09.123+03:30").unwrap()
}

#[test]
fn date_formats_match_the_observed_obsidian_clock() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/obsidian-date-formats.json")).unwrap();
    assert_eq!(fixture["reference"], "1.13.7");
    let mut checked = 0;
    for sample in fixture["samples"].as_array().unwrap() {
        let now = DateTime::parse_from_rfc3339(sample["input"].as_str().unwrap()).unwrap();
        for format in sample["formats"].as_array().unwrap() {
            let pattern = format["pattern"].as_str().unwrap();
            assert_eq!(
                moment_format(now, pattern, "YYYY-MM-DD"),
                format["expected"].as_str().unwrap(),
                "{} / {pattern}",
                sample["input"]
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 50,
        "The reference matrix must not silently become empty"
    );
}

#[test]
fn template_expansion_preserves_unicode_crlf_and_unknown_variables() {
    let config = Config::default();
    let source = "# {{title}}\r\n{{date}} {{time}} {{time:YYYY/MM/DD}}\r\n{{content}} {{title:custom}} {{unclosed\n";
    assert_eq!(
        config.expand(source, "یادداشت {{date}}", clock()),
        "# یادداشت {{date}}\r\n2026-10-21 13:05 2026/10/21\r\n{{content}} {{title:custom}} {{unclosed\n"
    );
}

#[test]
fn daily_note_defaults_to_empty_and_never_replaces_an_existing_file() {
    let base = tempfile::tempdir().unwrap();
    let mut vault = Vault::open(base.path()).unwrap();
    let config = Config::default();
    let path = config.create_daily(&mut vault, clock()).unwrap();
    assert_eq!(path, Path::new("2026-10-21.md"));
    assert_eq!(fs::read(base.path().join(&path)).unwrap(), b"");
    fs::write(base.path().join(&path), "# Already written\nسلام\n").unwrap();
    assert_eq!(config.create_daily(&mut vault, clock()).unwrap(), path);
    assert_eq!(
        fs::read_to_string(base.path().join(&path)).unwrap(),
        "# Already written\nسلام\n"
    );
}

#[test]
fn daily_note_uses_custom_subfolders_and_the_template_without_rewriting_it() {
    let base = tempfile::tempdir().unwrap();
    fs::create_dir(base.path().join("Templates")).unwrap();
    let original = "# {{title}}\n\n{{date:dddd, MMMM Do}}\n\n- [ ] Review\n";
    fs::write(base.path().join("Templates/Daily.md"), original).unwrap();
    let mut vault = Vault::open(base.path()).unwrap();
    let mut config = Config::default();
    config.daily.folder = "یادداشت‌ها".into();
    config.daily.format = "YYYY/MMMM/YYYY-MMM-DD".into();
    config.daily.template = "Templates/Daily".into();
    let path = config.create_daily(&mut vault, clock()).unwrap();
    assert_eq!(path, Path::new("یادداشت‌ها/2026/October/2026-Oct-21.md"));
    assert_eq!(
        fs::read_to_string(base.path().join(&path)).unwrap(),
        "# 2026-Oct-21\n\nWednesday, October 21st\n\n- [ ] Review\n"
    );
    assert_eq!(
        fs::read_to_string(base.path().join("Templates/Daily.md")).unwrap(),
        original
    );
}

#[test]
fn missing_template_and_unsafe_names_cannot_create_a_daily_note() {
    let base = tempfile::tempdir().unwrap();
    let mut vault = Vault::open(base.path()).unwrap();
    let mut config = Config::default();
    config.daily.folder = "Journal".into();
    config.daily.template = "Templates/Missing".into();
    assert!(config.create_daily(&mut vault, clock()).is_err());
    assert!(!base.path().join("Journal").exists());
    config.daily.template.clear();
    for name in [
        "../Outside",
        "/absolute",
        ".hidden",
        "target",
        "__pycache__",
    ] {
        config.daily.folder = name.into();
        assert!(config.create_daily(&mut vault, clock()).is_err(), "{name}");
    }
    config.daily.folder.clear();
    config.daily.format = "[../Outside]".into();
    assert!(config.create_daily(&mut vault, clock()).is_err());
}

#[test]
fn template_list_is_bounded_to_the_configured_folder() {
    let base = tempfile::tempdir().unwrap();
    for folder in ["Templates", "Templates/Nested", "Templates other"] {
        fs::create_dir_all(base.path().join(folder)).unwrap();
    }
    for path in [
        "Home.md",
        "Templates/One.md",
        "Templates/Nested/Two.md",
        "Templates other/Outside.md",
    ] {
        fs::write(base.path().join(path), "Template").unwrap();
    }
    let vault = Vault::open(base.path()).unwrap();
    let mut config = Config::default();
    assert!(config.template_paths(&vault).is_err());
    config.templates.folder = "Templates".into();
    assert_eq!(
        config.template_paths(&vault).unwrap(),
        vec![
            Path::new("Templates/Nested/Two.md").to_owned(),
            Path::new("Templates/One.md").to_owned()
        ]
    );
    config.templates.folder = "../Templates".into();
    assert!(config.template_paths(&vault).is_err());
}

#[test]
fn existing_obsidian_settings_are_imported_without_writing_to_the_vault() {
    let base = tempfile::tempdir().unwrap();
    fs::create_dir(base.path().join(".obsidian")).unwrap();
    let template = r#"{"folder":"Templates","dateFormat":"DD/MM/YYYY","timeFormat":"h:mm A"}"#;
    let daily = r#"{"folder":"Journal","format":"YYYY/MM/DD","template":"Templates/Daily","autorun":false}"#;
    fs::write(base.path().join(".obsidian/templates.json"), template).unwrap();
    fs::write(base.path().join(".obsidian/daily-notes.json"), daily).unwrap();
    let config = Config::from_obsidian(base.path()).unwrap();
    assert_eq!(config.templates.folder, "Templates");
    assert_eq!(config.daily.template, "Templates/Daily");
    assert_eq!(
        config.expand("{{date}} {{time}}", "Home", clock()),
        "21/10/2026 1:05 PM"
    );
    assert_eq!(
        fs::read_to_string(base.path().join(".obsidian/templates.json")).unwrap(),
        template
    );
    assert_eq!(
        fs::read_to_string(base.path().join(".obsidian/daily-notes.json")).unwrap(),
        daily
    );
    fs::write(base.path().join(".obsidian/templates.json"), "broken").unwrap();
    assert!(Config::from_obsidian(base.path()).is_err());
}

#[cfg(unix)]
#[test]
fn linked_settings_and_template_folders_cannot_read_outside_the_vault() {
    use std::os::unix::fs::symlink;
    let base = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    fs::write(outside.path().join("Secret.md"), "Private").unwrap();
    symlink(outside.path(), base.path().join(".obsidian")).unwrap();
    assert!(Config::from_obsidian(base.path()).is_err());
    symlink(outside.path(), base.path().join("Templates")).unwrap();
    let mut vault = Vault::open(base.path()).unwrap();
    let mut config = Config::default();
    config.templates.folder = "Templates".into();
    assert!(config.template_paths(&vault).is_err());
    config.daily.template = "Templates/Secret".into();
    assert!(config.create_daily(&mut vault, clock()).is_err());
    assert!(!base.path().join("2026-10-21.md").exists());
    assert_eq!(
        fs::read_to_string(outside.path().join("Secret.md")).unwrap(),
        "Private"
    );
}
