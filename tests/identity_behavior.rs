use rustidian::paths::app_directory;
use std::fs;
#[cfg(feature = "desktop")]
use std::process::Command;

#[test]
fn profile_namespace_keeps_recovery_sessions_and_existing_rustidian_profiles() {
    let base = tempfile::tempdir().unwrap();
    let current = base.path().join("rustidian");
    let previous = base.path().join("rust-notes");
    assert_eq!(app_directory(base.path()), current);
    assert!(
        !current.exists(),
        "Looking up a new profile must not create files"
    );

    fs::create_dir_all(previous.join("recovery")).unwrap();
    for (name, bytes) in [
        ("vaults.json", b"{\"roots\":[\"/notes\"]}".as_slice()),
        ("session.json", b"{\"tabs\":[\"Note.md\"]}".as_slice()),
        (
            "recovery/history.redb",
            b"\x00saved recovery bytes\xff".as_slice(),
        ),
    ] {
        fs::write(previous.join(name), bytes).unwrap();
    }
    assert_eq!(app_directory(base.path()), current);
    assert_eq!(
        fs::read(current.join("recovery/history.redb")).unwrap(),
        b"\x00saved recovery bytes\xff"
    );
    assert_eq!(
        fs::read(current.join("session.json")).unwrap(),
        b"{\"tabs\":[\"Note.md\"]}"
    );
    assert_eq!(
        fs::read(current.join("vaults.json")).unwrap(),
        b"{\"roots\":[\"/notes\"]}"
    );
    #[cfg(unix)]
    {
        assert!(previous.is_symlink());
        fs::write(previous.join("session.json"), b"older window saved").unwrap();
        assert_eq!(
            fs::read(current.join("session.json")).unwrap(),
            b"older window saved"
        );
    }

    let collision = tempfile::tempdir().unwrap();
    for name in ["rustidian", "rust-notes"] {
        fs::create_dir(collision.path().join(name)).unwrap();
        fs::write(collision.path().join(name).join("vaults.json"), name).unwrap();
    }
    assert_eq!(
        app_directory(collision.path()),
        collision.path().join("rustidian")
    );
    for name in ["rustidian", "rust-notes"] {
        assert_eq!(
            fs::read_to_string(collision.path().join(name).join("vaults.json")).unwrap(),
            name
        );
    }
}

#[test]
#[cfg(feature = "desktop")]
fn cli_identifies_the_app_without_a_display_and_rejects_a_missing_vault() {
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_rustidian"))
            .args(args)
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .unwrap()
    };
    for flag in ["--version", "-V"] {
        let result = run(&[flag]);
        assert!(result.status.success(), "{:?}", result);
        assert_eq!(
            String::from_utf8(result.stdout).unwrap(),
            format!("Rustidian {}\n", env!("CARGO_PKG_VERSION"))
        );
    }
    for flag in ["--help", "-h"] {
        let result = run(&[flag]);
        assert!(result.status.success());
        let help = String::from_utf8(result.stdout).unwrap();
        assert!(help.contains("Usage: rustidian "));
        assert!(help.contains("RUSTIDIAN_DEFAULT_VAULT"));
    }
    let folder = tempfile::tempdir().unwrap();
    let missing = folder.path().join("Missing vault");
    let result = run(&["--check-vault", missing.to_str().unwrap()]);
    assert!(!result.status.success());
    assert!(!missing.exists());
    assert!(!result.stderr.is_empty());
}
