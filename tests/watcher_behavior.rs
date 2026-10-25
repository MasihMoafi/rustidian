use std::{
    fs,
    path::Path,
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn vault_watcher_delivers_external_changes() {
    let vault = tempfile::tempdir().unwrap();
    let note = vault.path().join("note.md");
    fs::write(&note, "before").unwrap();
    // A native folder-create event precedes recursive watch registration.
    // The GUI folder workflow covers immediate population and vault rescanning.
    let folder = vault.path().join("nested");
    fs::create_dir(&folder).unwrap();
    let (sender, receiver) = mpsc::channel();
    let _watcher = rustidian::watcher::watch(vault.path(), move |event| {
        sender.send(event).unwrap();
    })
    .expect("vault monitoring must survive unavailable native watches");

    let wait_for = |path: &Path, kind: fn(notify::EventKind) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut observed = Vec::new();
        loop {
            let event = receiver
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!(
                        "External change at {} was not reported: {error}. Observed: {observed:?}",
                        path.display()
                    )
                })
                .unwrap();
            if kind(event.kind) && event.paths.contains(&path.to_owned()) {
                break;
            }
            observed.push(event);
        }
    };
    // Same-size writes within one timestamp tick still need to refresh the editor.
    fs::write(&note, "after!").unwrap();
    wait_for(&note, |kind| kind.is_modify());
    let added = folder.join("added.md");
    fs::write(&added, "new").unwrap();
    wait_for(&added, |kind| kind.is_create());
    let renamed = folder.join("renamed.md");
    fs::rename(&added, &renamed).unwrap();
    wait_for(&renamed, |kind| kind.is_modify() || kind.is_create());
    fs::remove_file(&renamed).unwrap();
    wait_for(&renamed, |kind| kind.is_remove());
    let created = vault.path().join("new folder");
    fs::create_dir(&created).unwrap();
    wait_for(&created, |kind| kind.is_create());
}
