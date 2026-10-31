use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, mpsc},
    time::{Duration, Instant},
};

// Tests here share one process, so watch counts are taken one test at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn wait_for(
    receiver: &mpsc::Receiver<notify::Result<notify::Event>>,
    path: &Path,
    kind: fn(notify::EventKind) -> bool,
) {
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
}

#[test]
fn vault_watcher_delivers_external_changes() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("note.md"), "before").unwrap();
    // A native folder-create event precedes recursive watch registration.
    // The GUI folder workflow covers immediate population and vault rescanning.
    fs::create_dir(dir.path().join("nested")).unwrap();
    // Watch the indexed folders the way the app does after opening a vault.
    let vault = rustidian::vault::Vault::open(dir.path()).unwrap();
    let (sender, receiver) = mpsc::channel();
    let mut watch = rustidian::watcher::Watch::new(&vault.root, move |event| {
        sender.send(event).unwrap();
    })
    .expect("the vault must be watched");
    watch
        .update(&vault.folders, || false)
        .expect("the vault must be watched");
    let wait_for =
        |path: &Path, kind: fn(notify::EventKind) -> bool| wait_for(&receiver, path, kind);
    let note = vault.root.join("note.md");
    // Same-size writes within one timestamp tick still need to refresh the editor.
    fs::write(&note, "after!").unwrap();
    wait_for(&note, |kind| kind.is_modify());
    let folder = vault.root.join("nested");
    let added = folder.join("added.md");
    fs::write(&added, "new").unwrap();
    wait_for(&added, |kind| kind.is_create());
    let renamed = folder.join("renamed.md");
    fs::rename(&added, &renamed).unwrap();
    wait_for(&renamed, |kind| kind.is_modify() || kind.is_create());
    fs::remove_file(&renamed).unwrap();
    wait_for(&renamed, |kind| kind.is_remove());
    let created = vault.root.join("new folder");
    fs::create_dir(&created).unwrap();
    wait_for(&created, |kind| kind.is_create());
}

/// Native watches this process holds; Linux lists them per inotify descriptor.
#[cfg(target_os = "linux")]
fn native_watches() -> usize {
    fs::read_dir("/proc/self/fdinfo")
        .unwrap()
        .filter_map(|entry| fs::read_to_string(entry.ok()?.path()).ok())
        .map(|info| {
            info.lines()
                .filter(|line| line.starts_with("inotify wd:"))
                .count()
        })
        .sum()
}

#[cfg(target_os = "linux")]
fn settle_watches(expected: usize, message: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while native_watches() != expected {
        assert!(
            Instant::now() < deadline,
            "{message}: {} watches, expected {expected}",
            native_watches()
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// 27 Oct: re-registering a 44,104-folder vault on each folder change hit the
/// system's watch limit, and the fallback re-read every file every 2 seconds.
#[cfg(target_os = "linux")]
#[test]
fn folder_changes_update_the_existing_watches() {
    let _serial = SERIAL.lock().unwrap_or_else(|error| error.into_inner());
    let vault = tempfile::tempdir().unwrap();
    let root = vault.path();
    fs::create_dir(root.join("kept")).unwrap();
    let before = native_watches();
    let (sender, receiver) = mpsc::channel();
    let mut watch = rustidian::watcher::Watch::new(root, move |event| {
        let _ = sender.send(event);
    })
    .unwrap();
    let folders = |names: &[&str]| names.iter().map(PathBuf::from).collect::<Vec<_>>();

    watch
        .update(&folders(&["kept", "deleted since indexing"]), || false)
        .unwrap();
    settle_watches(
        before + 2,
        "The root and the existing folder must be watched",
    );
    fs::create_dir(root.join("added")).unwrap();
    watch
        .update(&folders(&["kept", "added"]), || false)
        .unwrap();
    settle_watches(
        before + 3,
        "A new folder must add one watch to the existing ones",
    );
    let note = root.join("added/note.md");
    fs::write(&note, "new").unwrap();
    wait_for(&receiver, &note, |kind| kind.is_create());

    fs::remove_dir(root.join("kept")).unwrap();
    fs::create_dir(root.join("kept")).unwrap();
    wait_for(&receiver, &root.join("kept"), |kind| kind.is_create());
    watch
        .update(&folders(&["kept", "added"]), || false)
        .unwrap();
    let recreated = root.join("kept/note.md");
    fs::write(&recreated, "recreated").unwrap();
    wait_for(&receiver, &recreated, |kind| kind.is_create());

    fs::remove_file(&note).unwrap();
    fs::remove_dir(root.join("added")).unwrap();
    watch.update(&folders(&["kept"]), || false).unwrap();
    settle_watches(before + 2, "A removed folder must release its watch");
}
