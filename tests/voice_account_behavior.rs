//! Read aloud's OpenRouter account (provider-login contract, checks 1, 4 and 6): keys live in
//! a secret store, are sent only to the endpoint and account chosen, and never reach a file
//! or a message.
use rustidian::voice_account::{Engine, Memory, SaveError, Secrets, Settings, Stores, speak};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

const CANARY: &str = "sk-or-v1-CANARY-0123456789abcdef";

/// A one-endpoint stand-in for OpenRouter: answers each request and reports its
/// Authorization header. Key "bad" gets 401.
fn server() -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/api/v1/audio/speech",
        listener.local_addr().unwrap()
    );
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let (mut auth, mut length, mut line) = (String::new(), 0, String::new());
            loop {
                line.clear();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if lower.starts_with("authorization:") {
                    auth = line[14..].trim().to_owned();
                }
                if let Some(value) = lower.strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap_or(0);
                }
            }
            let mut body = vec![0; length];
            let _ = reader.read_exact(&mut body);
            let refused = auth == "Bearer bad";
            let _ = sender.send(auth);
            let reply = if refused {
                "HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
                    .to_owned()
            } else {
                "HTTP/1.1 200 OK\r\ncontent-type: audio/pcm\r\ncontent-length: 4\r\nconnection: close\r\n\r\n\u{1}\u{2}\u{3}\u{4}".to_owned()
            };
            let _ = stream.write_all(reply.as_bytes());
        }
    });
    (url, receiver)
}

struct Locked;
impl Secrets for Locked {
    fn get(&self, _: &str) -> Result<Option<String>, String> {
        Err("The system keyring is locked or not available.".into())
    }
    fn set(&self, _: &str, _: &str) -> Result<(), String> {
        Err("The system keyring is locked or not available.".into())
    }
    fn delete(&self, _: &str) -> Result<(), String> {
        Err("The system keyring is locked or not available.".into())
    }
}

fn settings(endpoint: &str) -> Settings {
    Settings {
        engine: Engine::OpenRouter,
        endpoint: endpoint.into(),
        ..Settings::default()
    }
}

/// Check 1: save, use, replace and disconnect a key; an invalid key gets a plain message;
/// a changed endpoint receives no old key.
#[test]
fn a_key_is_used_replaced_disconnected_and_never_sent_to_another_endpoint() {
    let (keyring, session) = (Memory::default(), Memory::default());
    let stores = Stores {
        keyring: &keyring,
        session: &session,
    };
    let (url, heard) = server();
    let mut voice = settings(&url);

    let id = voice
        .save_key(stores, None, "Mine", "sk-first", false)
        .unwrap();
    let audio = speak(&voice.route(stores).unwrap(), "Hello").unwrap();
    assert_eq!(&audio[..4], b"RIFF");
    assert_eq!(&audio[44..], &[1, 2, 3, 4]);
    assert_eq!(heard.recv().unwrap(), "Bearer sk-first");

    voice
        .save_key(stores, Some(&id), "", "sk-second", false)
        .unwrap();
    assert_eq!(voice.accounts.len(), 1);
    assert_eq!(voice.accounts[0].label, "Mine");
    speak(&voice.route(stores).unwrap(), "Hello").unwrap();
    assert_eq!(heard.recv().unwrap(), "Bearer sk-second");

    voice.save_key(stores, Some(&id), "", "bad", false).unwrap();
    let refused = speak(&voice.route(stores).unwrap(), "Hello").unwrap_err();
    assert!(refused.contains("refused this key"), "{refused}");
    assert!(!refused.contains("bad"));
    heard.recv().unwrap();

    let (other, other_heard) = server();
    voice.endpoint = other;
    assert!(
        voice.route(stores).is_err(),
        "A key followed the endpoint change"
    );
    assert!(other_heard.try_recv().is_err());

    voice.endpoint = url;
    voice.disconnect(stores, &id).unwrap();
    assert!(voice.route(stores).is_err());
    assert_eq!(keyring.get(&id).unwrap(), None);
}

/// Check 4: a locked keyring refuses the save and the key can be kept for this session
/// only; the canary key never reaches the settings file or a message.
#[test]
fn a_locked_keyring_offers_session_only_and_the_key_stays_out_of_files() {
    let state = tempfile::tempdir().unwrap();
    let session = Memory::default();
    let locked = Stores {
        keyring: &Locked,
        session: &session,
    };
    let mut voice = settings("https://openrouter.ai/api/v1/audio/speech");

    match voice.save_key(locked, None, "Mine", CANARY, false) {
        Err(SaveError::Keyring(reason)) => assert!(!reason.contains(CANARY)),
        other => panic!("expected a keyring refusal, got {other:?}"),
    }
    assert!(voice.accounts.is_empty());
    voice.save_key(locked, None, "Mine", CANARY, true).unwrap();
    assert!(voice.route(locked).is_ok());
    voice.save(state.path()).unwrap();

    let file = Settings::file(state.path());
    let written = std::fs::read_to_string(&file).unwrap();
    assert!(!written.contains(CANARY) && written.contains("0def"));
    assert!(!format!("{voice:?}{:?}", voice.route(locked).unwrap()).contains(CANARY));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&file).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "settings file readable by others");
    }
    // The session ends: its key is gone, and so is its account.
    assert!(Settings::load(state.path()).accounts.is_empty());

    // A symlinked settings file is never written through.
    let target = state.path().join("elsewhere.json");
    std::fs::write(&target, "{}").unwrap();
    std::fs::remove_file(&file).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&target, &file).unwrap();
        assert!(voice.save(state.path()).is_err());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "{}");
    }
}

/// Check 6: two accounts, no cross-use; the selection survives a restart; a cloud outage
/// is a plain message, not a fallback to another key.
#[test]
fn two_accounts_stay_apart_and_the_choice_survives_a_restart() {
    let state = tempfile::tempdir().unwrap();
    let (keyring, session) = (Memory::default(), Memory::default());
    let stores = Stores {
        keyring: &keyring,
        session: &session,
    };
    let (url, heard) = server();
    let mut voice = settings(&url);
    let work = voice
        .save_key(stores, None, "Work", "sk-work", false)
        .unwrap();
    let home = voice
        .save_key(stores, None, "Home", "sk-home", false)
        .unwrap();

    speak(&voice.route(stores).unwrap(), "Hi").unwrap();
    assert_eq!(heard.recv().unwrap(), "Bearer sk-home");
    voice.selected = Some(work.clone());
    voice.save(state.path()).unwrap();

    let restarted = Settings::load(state.path());
    assert_eq!(restarted.selected.as_deref(), Some(work.as_str()));
    speak(&restarted.route(stores).unwrap(), "Hi").unwrap();
    assert_eq!(heard.recv().unwrap(), "Bearer sk-work");

    // The selected key is missing from the keyring: no fallback to the other account.
    keyring.delete(&work).unwrap();
    assert!(restarted.route(stores).unwrap_err().contains("Work"));
    let _ = home;

    // Nothing listens here: an outage, said plainly.
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let dead = format!(
        "http://{}/api/v1/audio/speech",
        closed.local_addr().unwrap()
    );
    drop(closed);
    let mut offline = settings(&dead);
    offline
        .save_key(stores, None, "Home", "sk-home", false)
        .unwrap();
    let outage = speak(&offline.route(stores).unwrap(), "Hi").unwrap_err();
    assert!(outage.contains("cannot be reached"), "{outage}");
}

#[test]
fn only_https_or_this_machine_may_receive_a_key() {
    let (keyring, session) = (Memory::default(), Memory::default());
    let stores = Stores {
        keyring: &keyring,
        session: &session,
    };
    let mut voice = settings("http://example.com/speech");
    assert!(matches!(
        voice.save_key(stores, None, "", "sk-x", false),
        Err(SaveError::Invalid(_))
    ));
}
