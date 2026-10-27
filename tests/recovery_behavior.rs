use rustidian::recovery::{Config, History};
use std::{fs, path::Path};

fn history(base: &Path) -> History {
    History::at(base.join("history/snapshots.redb"))
}

#[test]
fn a_note_absent_when_selected_can_be_recreated_but_a_new_collision_is_preserved() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("vault");
    fs::create_dir(&root).unwrap();
    let mut vault = rustidian::vault::Vault::open(&root).unwrap();
    let store = history(base.path());
    let config = Config::default();
    let path = Path::new("Notes/Deleted.md");
    let original = "# Deleted\r\nسلام\r\n";
    store.record(path, original, 1_000, &config, true).unwrap();
    let id = store.versions(path).unwrap()[0].id;
    let expected = rustidian::vault::Document::new(path.to_owned());
    let restored = store
        .restore(&mut vault, &expected, id, 2_000, &config)
        .unwrap();
    assert_eq!(fs::read(root.join(path)).unwrap(), original.as_bytes());
    assert!(!restored.dirty());
    fs::write(root.join(path), "# Another file created meanwhile\n").unwrap();
    assert!(
        store
            .restore(&mut vault, &expected, id, 3_000, &config)
            .is_err()
    );
    assert_eq!(
        fs::read_to_string(root.join(path)).unwrap(),
        "# Another file created meanwhile\n"
    );
}

#[test]
fn settings_import_is_read_only_and_vault_history_locations_are_distinct() {
    let base = tempfile::tempdir().unwrap();
    let first = base.path().join("first");
    let second = base.path().join("second");
    fs::create_dir_all(first.join(".obsidian")).unwrap();
    fs::create_dir(&second).unwrap();
    let settings = first.join(".obsidian/file-recovery.json");
    let bytes = b"{\"intervalMinutes\":0.25,\"keepDays\":14,\"other\":true}";
    fs::write(&settings, bytes).unwrap();
    let config = Config::from_obsidian(&first).unwrap();
    assert_eq!(config.interval_minutes, 0.25);
    assert_eq!(config.keep_days, 14.0);
    assert_eq!(fs::read(&settings).unwrap(), bytes);
    assert_eq!(Config::from_obsidian(&second).unwrap(), Config::default());
    assert!(!second.join(".obsidian").exists());
    assert_ne!(
        rustidian::paths::state_file(&first, "recovery", "redb"),
        rustidian::paths::state_file(&second, "recovery", "redb")
    );
    fs::write(&settings, b"{\"intervalMinutes\":-1}").unwrap();
    assert!(Config::from_obsidian(&first).is_err());
    assert_eq!(fs::read(&settings).unwrap(), b"{\"intervalMinutes\":-1}");
    fs::write(&settings, b"{broken").unwrap();
    assert!(Config::from_obsidian(&first).is_err());
}

#[test]
fn lowering_the_interval_releases_a_deferred_capture_without_another_edit() {
    use std::time::{Duration, Instant};
    let base = tempfile::tempdir().unwrap();
    let store = history(base.path());
    let config = Config::default();
    let path = Path::new("Home.md");
    store
        .record(
            path,
            "first",
            chrono::Local::now().timestamp_millis(),
            &config,
            false,
        )
        .unwrap();
    let (sender, events) = std::sync::mpsc::channel();
    let mut recorder =
        rustidian::recovery_capture::Recorder::start(store.clone(), config.clone(), move || {
            let _ = sender.send(());
        });
    assert!(recorder.capture(path, "deferred", &config));
    let immediate = Config {
        interval_minutes: 0.0,
        ..config
    };
    assert!(recorder.configure(&immediate));
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let versions = store.versions(path).unwrap();
        if versions.len() == 2 {
            assert_eq!(store.content(path, versions[0].id).unwrap(), "deferred");
            break;
        }
        assert!(
            Instant::now() < until,
            "Changing the interval did not release a pending snapshot"
        );
        let _ = events.recv_timeout(Duration::from_millis(20));
    }
    assert!(recorder.errors.try_recv().is_err());
}

#[test]
fn line_changes_reconstruct_both_exact_inputs_including_unicode_and_crlf() {
    use rustidian::recovery::{Change, changes};
    for (before, after) in [
        ("", ""),
        ("same\r\n", "same\r\n"),
        ("# سلام\r\nold\nkept", "# سلام\r\nnew\nkept"),
        ("one\ntwo\nthree\n", "zero\none\nthree\nfour\n"),
    ] {
        let lines = changes(before, after);
        let original: String = lines
            .iter()
            .filter(|(kind, _)| *kind != Change::Added)
            .map(|(_, text)| text.as_str())
            .collect();
        let current: String = lines
            .iter()
            .filter(|(kind, _)| *kind != Change::Removed)
            .map(|(_, text)| text.as_str())
            .collect();
        assert_eq!(original, before);
        assert_eq!(current, after);
        if before == after {
            assert!(lines.iter().all(|(kind, _)| *kind == Change::Context));
        } else {
            assert!(lines.iter().any(|(kind, _)| *kind != Change::Context));
        }
    }
}

#[test]
fn restore_preserves_displaced_bytes_and_refuses_dirty_stale_or_missing_destinations() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("vault");
    fs::create_dir(&root).unwrap();
    let path = Path::new("Home.md");
    let disk = root.join(path);
    fs::write(&disk, "# Current\r\n").unwrap();
    let mut vault = rustidian::vault::Vault::open(&root).unwrap();
    let expected = vault.read(path).unwrap();
    let store = history(base.path());
    let config = Config::default();
    store
        .record(path, "# Older\r\n", 1_000, &config, true)
        .unwrap();
    let id = store.versions(path).unwrap()[0].id;
    let mut dirty = expected.clone();
    dirty.text.push_str("UNSAVED");
    assert!(
        store
            .restore(&mut vault, &dirty, id, 2_000, &config)
            .is_err()
    );
    assert_eq!(fs::read(&disk).unwrap(), b"# Current\r\n");
    fs::write(&disk, "# External\n").unwrap();
    assert!(
        store
            .restore(&mut vault, &expected, id, 2_000, &config)
            .is_err()
    );
    assert_eq!(fs::read_to_string(&disk).unwrap(), "# External\n");
    let current = vault.read(path).unwrap();
    let restored = store
        .restore(&mut vault, &current, id, 3_000, &config)
        .unwrap();
    assert!(!restored.dirty());
    assert_eq!(fs::read(&disk).unwrap(), b"# Older\r\n");
    assert!(
        store
            .versions(path)
            .unwrap()
            .iter()
            .any(|version| store.content(path, version.id).unwrap() == "# External\n")
    );
    fs::remove_file(&disk).unwrap();
    assert!(
        store
            .restore(&mut vault, &restored, id, 4_000, &config)
            .is_err()
    );
    assert!(!disk.exists());
    assert!(
        store
            .restore(&mut vault, &restored, u64::MAX, 4_000, &config)
            .is_err()
    );
}

#[test]
fn background_capture_coalesces_to_latest_saved_bytes_and_clear_cancels_deferred_versions() {
    use std::time::{Duration, Instant};
    let base = tempfile::tempdir().unwrap();
    let store = history(base.path());
    let config = Config::default();
    let (wake, events) = std::sync::mpsc::channel();
    let mut recorder =
        rustidian::recovery_capture::Recorder::start(store.clone(), config.clone(), move || {
            let _ = wake.send(());
        });
    let wait_for = |text: &str| {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if store
                .versions(Path::new("Home.md"))
                .unwrap()
                .first()
                .is_some_and(|v| store.content(Path::new("Home.md"), v.id).unwrap() == text)
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "Background snapshot was not stored: {text}"
            );
            let _ = events.recv_timeout(Duration::from_millis(20));
        }
    };
    assert!(recorder.capture(Path::new("Home.md"), "first", &config));
    wait_for("first");
    assert!(recorder.capture(Path::new("Home.md"), "superseded", &config));
    assert!(recorder.capture(Path::new("Home.md"), "latest\r\nسلام", &config));
    // Release the queued replacements explicitly, independent of CI scheduling
    // and database latency consuming a short snapshot interval.
    assert!(recorder.configure(&Config {
        interval_minutes: 0.0,
        ..config.clone()
    }));
    wait_for("latest\r\nسلام");
    assert_eq!(store.versions(Path::new("Home.md")).unwrap().len(), 2);
    let deferred_config = Config {
        interval_minutes: 0.01,
        ..config
    };
    assert!(recorder.capture(Path::new("Home.md"), "deferred", &deferred_config));
    recorder
        .clear()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    std::thread::sleep(Duration::from_millis(700));
    assert!(store.files().unwrap().is_empty());
    assert!(recorder.errors.try_recv().is_err());
}

#[test]
fn empty_history_does_not_create_files_and_snapshots_preserve_exact_content() {
    let base = tempfile::tempdir().unwrap();
    let store = history(base.path());
    assert!(store.files().unwrap().is_empty());
    assert!(!base.path().join("history").exists());
    let text = "# یادداشت\r\n\r\n- [x] Checked\n==Yellow==\n";
    assert!(
        store
            .record(
                Path::new("Notes/سلام.md"),
                text,
                1_000,
                &Config::default(),
                false
            )
            .unwrap()
    );
    let versions = store.versions(Path::new("Notes/سلام.md")).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].timestamp_ms, 1_000);
    assert_eq!(versions[0].bytes, text.len() as u64);
    assert_eq!(
        store
            .content(Path::new("Notes/سلام.md"), versions[0].id)
            .unwrap(),
        text
    );
    assert_eq!(store.files().unwrap(), [Path::new("Notes/سلام.md")]);
    assert!(!base.path().join("Notes").exists());
    let reopened = history(base.path());
    assert_eq!(
        reopened
            .content(Path::new("Notes/سلام.md"), versions[0].id)
            .unwrap(),
        text
    );
}

#[test]
fn unchanged_and_early_versions_coalesce_but_zero_interval_records_distinct_edits() {
    let base = tempfile::tempdir().unwrap();
    let store = history(base.path());
    let path = Path::new("Home.md");
    let mut config = Config::default();
    assert!(store.record(path, "first", 0, &config, false).unwrap());
    assert!(!store.record(path, "early", 1_000, &config, false).unwrap());
    assert!(
        !store
            .record(path, "first", 300_000, &config, false)
            .unwrap()
    );
    assert!(
        store
            .record(path, "second", 300_000, &config, false)
            .unwrap()
    );
    config.interval_minutes = 0.0;
    assert!(
        store
            .record(path, "third", 300_001, &config, false)
            .unwrap()
    );
    assert!(!store.record(path, "third", 300_002, &config, true).unwrap());
    let versions = store.versions(path).unwrap();
    assert_eq!(versions.len(), 3);
    assert_eq!(store.content(path, versions[0].id).unwrap(), "third");
    assert_eq!(store.content(path, versions[2].id).unwrap(), "first");
}

#[test]
fn retention_prunes_expired_versions_and_clock_rollback_does_not_lose_an_edit() {
    let base = tempfile::tempdir().unwrap();
    let store = history(base.path());
    let path = Path::new("Home.md");
    let config = Config {
        interval_minutes: 0.0,
        keep_days: 1.0,
    };
    assert!(store.record(path, "old", 0, &config, false).unwrap());
    assert!(
        store
            .record(path, "recent", 86_400_000, &config, false)
            .unwrap()
    );
    store.prune(86_400_001, &config).unwrap();
    let versions = store.versions(path).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(store.content(path, versions[0].id).unwrap(), "recent");
    assert!(
        store
            .record(path, "clock moved back", 1_000, &Config::default(), false)
            .unwrap()
    );
    store.prune(200_000_000, &config).unwrap();
    assert!(store.versions(path).unwrap().is_empty());
    assert!(store.files().unwrap().is_empty());
    assert!(
        store
            .record(path, "current", 200_000_001, &config, false)
            .unwrap()
    );
}

#[test]
fn clear_never_changes_notes_and_does_not_reuse_a_selected_snapshot_id() {
    let base = tempfile::tempdir().unwrap();
    let note = base.path().join("Home.md");
    fs::write(&note, "# Keep my file\n").unwrap();
    let store = history(base.path());
    let path = Path::new("Home.md");
    store
        .record(path, "original", 1_000, &Config::default(), false)
        .unwrap();
    let id = store.versions(path).unwrap()[0].id;
    store.clear().unwrap();
    assert!(store.files().unwrap().is_empty());
    assert!(store.content(path, id).is_err());
    assert_eq!(fs::read(&note).unwrap(), b"# Keep my file\n");
    store
        .record(path, "different", 2_000, &Config::default(), false)
        .unwrap();
    assert!(store.versions(path).unwrap()[0].id > id);
    assert!(store.content(path, id).is_err());
}

#[test]
fn concurrent_windows_preserve_each_others_files_and_versions() {
    let base = tempfile::tempdir().unwrap();
    let location = base.path().join("history/snapshots.redb");
    let threads: Vec<_> = (0..3)
        .map(|index| {
            let location = location.clone();
            std::thread::spawn(move || {
                let store = History::at(location);
                store
                    .record(
                        Path::new(&format!("Window {index}.md")),
                        &format!("text {index}"),
                        1_000,
                        &Config::default(),
                        false,
                    )
                    .unwrap();
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    let store = History::at(location);
    assert_eq!(store.files().unwrap().len(), 3);
    for index in 0..3 {
        let path = format!("Window {index}.md");
        let versions = store.versions(Path::new(&path)).unwrap();
        assert_eq!(
            store.content(Path::new(&path), versions[0].id).unwrap(),
            format!("text {index}")
        );
    }
}

#[test]
fn invalid_paths_settings_and_corrupt_storage_preserve_existing_bytes() {
    let base = tempfile::tempdir().unwrap();
    let store = history(base.path());
    for path in ["../Private.md", "/tmp/Private.md", "", "File.png"] {
        assert!(
            store
                .record(Path::new(path), "private", 0, &Config::default(), false)
                .is_err()
        );
    }
    assert!(!base.path().join("history").exists());
    let invalid = Config {
        interval_minutes: f64::NAN,
        keep_days: 7.0,
    };
    assert!(
        store
            .record(Path::new("Home.md"), "text", 0, &invalid, false)
            .is_err()
    );
    fs::create_dir(base.path().join("history")).unwrap();
    let location = base.path().join("history/snapshots.redb");
    fs::write(&location, b"Not a database; keep these bytes.").unwrap();
    assert!(store.files().is_err());
    assert!(
        store
            .record(Path::new("Home.md"), "text", 0, &Config::default(), false)
            .is_err()
    );
    assert_eq!(
        fs::read(location).unwrap(),
        b"Not a database; keep these bytes."
    );
}

#[cfg(unix)]
#[test]
fn snapshot_data_is_private_and_linked_storage_cannot_replace_a_note() {
    use std::os::unix::{fs::PermissionsExt, fs::symlink};
    let base = tempfile::tempdir().unwrap();
    let store = history(base.path());
    store
        .record(
            Path::new("Board.canvas"),
            "{\"nodes\":[],\"edges\":[]}",
            0,
            &Config::default(),
            false,
        )
        .unwrap();
    assert_eq!(
        fs::metadata(base.path().join("history"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let original = base.path().join("Keep.md");
    fs::write(&original, "# Keep\n").unwrap();
    let linked = base.path().join("linked.redb");
    symlink(&original, &linked).unwrap();
    let linked_store = History::at(linked);
    assert!(linked_store.files().is_err());
    assert!(
        linked_store
            .record(
                Path::new("Home.md"),
                "private",
                1,
                &Config::default(),
                false
            )
            .is_err()
    );
    assert_eq!(fs::read(original).unwrap(), b"# Keep\n");
}

#[test]
fn closing_the_recorder_writes_the_snapshot_waiting_for_its_interval() {
    use std::time::{Duration, Instant};
    let base = tempfile::tempdir().unwrap();
    let store = history(base.path());
    let config = Config::default();
    let path = Path::new("Home.md");
    store
        .record(
            path,
            "first",
            chrono::Local::now().timestamp_millis(),
            &config,
            false,
        )
        .unwrap();
    {
        let recorder =
            rustidian::recovery_capture::Recorder::start(store.clone(), config.clone(), || {});
        assert!(recorder.capture(path, "deferred", &config));
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(store.versions(path).unwrap().len(), 1);
    }
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let versions = store.versions(path).unwrap();
        if versions.len() == 2 {
            assert_eq!(store.content(path, versions[0].id).unwrap(), "deferred");
            break;
        }
        assert!(
            Instant::now() < until,
            "Closing the recorder lost the deferred snapshot"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
