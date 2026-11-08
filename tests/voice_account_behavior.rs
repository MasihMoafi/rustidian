//! Read aloud's OpenRouter account (provider-login contract, checks 1, 4 and 6): keys live in
//! a secret store, are sent only to the endpoint and account chosen, and never reach a file
//! or a message.
use rustidian::voice_account::{
    DictationBackend, Engine, Memory, SaveError, Secrets, Settings, Stores, VoiceChoice,
    choose_engine, speak, transcribe, transcription_models,
};
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
    let audio = speak(&voice.route(stores).unwrap(), "Hello", 1.0).unwrap();
    assert_eq!(&audio[..4], b"RIFF");
    assert_eq!(&audio[44..], &[1, 2, 3, 4]);
    assert_eq!(heard.recv().unwrap(), "Bearer sk-first");

    voice
        .save_key(stores, Some(&id), "", "sk-second", false)
        .unwrap();
    assert_eq!(voice.accounts.len(), 1);
    assert_eq!(voice.accounts[0].label, "Mine");
    speak(&voice.route(stores).unwrap(), "Hello", 1.0).unwrap();
    assert_eq!(heard.recv().unwrap(), "Bearer sk-second");

    voice.save_key(stores, Some(&id), "", "bad", false).unwrap();
    let refused = speak(&voice.route(stores).unwrap(), "Hello", 1.0).unwrap_err();
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
    assert!(!written.contains(CANARY) && written.contains("cdef"));
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

    speak(&voice.route(stores).unwrap(), "Hi", 1.0).unwrap();
    assert_eq!(heard.recv().unwrap(), "Bearer sk-home");
    voice.selected = Some(work.clone());
    voice.save(state.path()).unwrap();

    let restarted = Settings::load(state.path());
    assert_eq!(restarted.selected.as_deref(), Some(work.as_str()));
    speak(&restarted.route(stores).unwrap(), "Hi", 1.0).unwrap();
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
    let outage = speak(&offline.route(stores).unwrap(), "Hi", 1.0).unwrap_err();
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

#[test]
fn transcription_sends_wav_and_reads_the_returned_text() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/api/v1/audio/speech",
        listener.local_addr().unwrap()
    );
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        reader.read_line(&mut request).unwrap();
        assert!(request.starts_with("POST /api/v1/audio/transcriptions "));
        let mut length = 0;
        let mut auth = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
            let lower = line.to_ascii_lowercase();
            if let Some(value) = lower.strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
            if lower.starts_with("authorization:") {
                auth = line[14..].trim().to_owned();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({"model":"example/model", "language":"en", "input_audio":{"data":"UklGRg==", "format":"wav"}})
        );
        assert_eq!(auth, "Bearer sk-test");
        let body = r#"{"text":" hello "}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .unwrap();
    });
    let (keyring, session) = (Memory::default(), Memory::default());
    let stores = Stores {
        keyring: &keyring,
        session: &session,
    };
    let mut voice = settings(&endpoint);
    voice
        .save_key(stores, None, "Mine", "sk-test", false)
        .unwrap();
    assert_eq!(
        transcribe(&voice.route(stores).unwrap(), "example/model", b"RIFF").unwrap(),
        "hello"
    );
    server.join().unwrap();
}

#[test]
fn voice_choice_preserves_preferences_and_only_falls_back_when_local_is_missing() {
    use Engine::{Local, OpenRouter};
    use VoiceChoice::{Hosted, Local as UseLocal, Setup};
    for (preference, local_available, key_available, expected) in [
        (Local, true, true, UseLocal),
        (Local, true, false, UseLocal),
        (Local, false, true, Hosted),
        (Local, false, false, Setup),
        (OpenRouter, true, true, Hosted),
        (OpenRouter, false, true, Hosted),
        (OpenRouter, true, false, Setup),
        (OpenRouter, false, false, Setup),
    ] {
        assert_eq!(
            choose_engine(preference, local_available, key_available),
            expected
        );
    }
    let previous: Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(previous.engine, Local);
    assert_eq!(previous.dictation_backend, DictationBackend::Codex);
    assert!(!previous.transcription_model.is_empty());
}

/// A single response for a transcription or model-list request.
fn json_server(status: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/api/v1/audio/speech",
        listener.local_addr().unwrap()
    );
    let status = status.to_owned();
    let body = body.to_owned();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut request = String::new();
        reader.read_line(&mut request).unwrap();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut ignored = vec![0; length];
        reader.read_exact(&mut ignored).unwrap();
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        )
        .unwrap();
        request
    });
    (endpoint, handle)
}

#[test]
fn transcription_reports_provider_and_response_failures_without_secrets() {
    let (keyring, session) = (Memory::default(), Memory::default());
    let stores = Stores {
        keyring: &keyring,
        session: &session,
    };
    assert!(
        Settings::default()
            .route(stores)
            .unwrap_err()
            .contains("Add an OpenRouter key")
    );
    for (status, body, expected) in [
        ("401 Unauthorized", CANARY, "refused this key"),
        ("402 Payment Required", CANARY, "no credit"),
        ("500 Internal Server Error", CANARY, "HTTP 500"),
        ("200 OK", "not json", "invalid transcript"),
        ("200 OK", r#"{"text":"  "}"#, "No speech"),
        ("200 OK", r#"{"error":"missing"}"#, "No speech"),
    ] {
        let (endpoint, server) = json_server(status, body);
        let mut voice = settings(&endpoint);
        voice.save_key(stores, None, "Mine", CANARY, false).unwrap();
        let error = transcribe(&voice.route(stores).unwrap(), "test/model", b"wav").unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert!(!error.contains(CANARY));
        assert!(
            server
                .join()
                .unwrap()
                .starts_with("POST /api/v1/audio/transcriptions ")
        );
    }

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/api/v1/audio/speech",
        listener.local_addr().unwrap()
    );
    drop(listener);
    let mut voice = settings(&endpoint);
    voice.save_key(stores, None, "Mine", CANARY, false).unwrap();
    let error = transcribe(&voice.route(stores).unwrap(), "test/model", b"wav").unwrap_err();
    assert!(error.contains("cannot be reached") && !error.contains(CANARY));
}

#[test]
fn transcription_does_not_forward_a_key_on_redirect_or_to_an_unrelated_origin() {
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    destination.set_nonblocking(true).unwrap();
    let target = format!("http://{}/stolen", destination.local_addr().unwrap());
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/api/v1/audio/speech",
        source.local_addr().unwrap()
    );
    let server = std::thread::spawn(move || {
        let (mut stream, _) = source.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut ignored = vec![0; length];
        reader.read_exact(&mut ignored).unwrap();
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let (keyring, session) = (Memory::default(), Memory::default());
    let stores = Stores {
        keyring: &keyring,
        session: &session,
    };
    let mut voice = settings(&endpoint);
    voice.save_key(stores, None, "Mine", CANARY, false).unwrap();
    let error = transcribe(&voice.route(stores).unwrap(), "test/model", b"wav").unwrap_err();
    assert!(!error.contains(CANARY));
    server.join().unwrap();
    assert!(
        matches!(destination.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );

    let mut other = settings("https://unrelated.example/api/v1/audio/speech");
    other
        .save_key(stores, None, "Other", CANARY, false)
        .unwrap();
    assert!(transcribe(&other.route(stores).unwrap(), "test/model", b"wav").is_err());
}

#[test]
fn transcription_model_menu_uses_the_filtered_models_api() {
    let (endpoint, server) = json_server(
        "200 OK",
        r#"{"data":[{"id":"z/model"},{"id":"a/model"},{"id":"a/model"},{"name":"no id"}]}"#,
    );
    assert_eq!(
        transcription_models(&endpoint).unwrap(),
        ["a/model", "z/model"]
    );
    assert!(
        server
            .join()
            .unwrap()
            .starts_with("GET /api/v1/models?output_modalities=transcription ")
    );
}

#[test]
fn testing_a_key_reports_provider_results_without_exposing_the_key() {
    let candidate = format!("example-{:016x}", fastrand::u64(..));
    for (status, body, expected) in [
        ("200 OK", r#"{"data":{}}"#, None),
        ("401 Unauthorized", candidate.as_str(), Some("refused")),
        ("402 Payment Required", candidate.as_str(), Some("credit")),
        (
            "500 Internal Server Error",
            candidate.as_str(),
            Some("HTTP 500"),
        ),
        ("200 OK", "not json", Some("invalid")),
        ("302 Found", r#"{"data":{}}"#, Some("HTTP 302")),
    ] {
        let (endpoint, server) = json_server(status, body);
        let result = rustidian::voice_account::test_key(&endpoint, &candidate);
        if let Some(expected) = expected {
            let error = result.unwrap_err();
            assert!(error.contains(expected), "{error}");
            assert!(!error.contains(&candidate));
        } else {
            assert!(result.is_ok());
        }
        assert!(server.join().unwrap().starts_with("GET /api/v1/key "));
    }
    assert!(
        rustidian::voice_account::test_key("https://unrelated.example/speech", &candidate).is_err()
    );
    assert!(rustidian::voice_account::test_key("http://127.0.0.1:1/speech", " ").is_err());
}

#[test]
fn speech_does_not_forward_a_key_across_redirects() {
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    destination.set_nonblocking(true).unwrap();
    let target = format!("http://{}/stolen", destination.local_addr().unwrap());
    let source = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!(
        "http://{}/api/v1/audio/speech",
        source.local_addr().unwrap()
    );
    let server = std::thread::spawn(move || {
        let (mut stream, _) = source.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse().unwrap();
            }
        }
        let mut ignored = vec![0; length];
        reader.read_exact(&mut ignored).unwrap();
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let (keyring, session) = (Memory::default(), Memory::default());
    let stores = Stores {
        keyring: &keyring,
        session: &session,
    };
    let mut voice = settings(&endpoint);
    let candidate = format!("example-{:016x}", fastrand::u64(..));
    voice
        .save_key(stores, None, "Mine", &candidate, false)
        .unwrap();
    assert!(speak(&voice.route(stores).unwrap(), "Hello", 1.0).is_err());
    server.join().unwrap();
    assert!(
        matches!(destination.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
    );
}
