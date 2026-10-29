//! One process per profile, as in Obsidian: a later launch hands its file or
//! folder to the running window and exits instead of opening another window.
use std::{
    ffi::OsStr,
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::Duration,
};

/// A later launch's file or folder (`None` brings the window forward), with
/// the activation token its launcher gave it.
pub struct Request {
    pub path: Option<PathBuf>,
    /// What lets the window raise itself on Wayland; see `window::bring_forward`.
    pub token: Option<String>,
}

/// Wayland's message buffer holds 4096 bytes by default, and a request that
/// does not fit is an error on the window's connection.
const LONGEST_TOKEN: usize = 1024;

/// The activation token this process was launched with: `XDG_ACTIVATION_TOKEN`
/// on Wayland, `DESKTOP_STARTUP_ID` on X11. A launch that hands over passes it
/// on and starts nothing. A launch that opens the window leaves it to eframe,
/// which gives it to the new window and takes both variables out of the
/// environment before the app, and so any child, starts.
fn activation_token() -> Option<String> {
    token_from(|name| std::env::var(name).ok())
}

fn token_from(variable: impl Fn(&str) -> Option<String>) -> Option<String> {
    ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"]
        .into_iter()
        .find_map(|name| variable(name).filter(|token| !token.is_empty()))
}

/// How long a launch waits for the running window to take its request. The
/// window answers from its own thread at once, even while its UI is busy or
/// hidden; one that does not answer is stopped or stuck.
const ANSWER_WITHIN: Duration = if cfg!(test) {
    Duration::from_millis(300)
} else {
    Duration::from_secs(5)
};

fn socket() -> Option<PathBuf> {
    rustidian::vaults::location().map(|catalog| catalog.with_file_name("instance.sock"))
}

/// What a launch does.
pub enum Launch {
    /// The running window took the file or folder; this launch exits.
    HandedOff,
    /// No window runs: this one owns the profile's socket and serves later launches.
    Serve(Instance),
    /// A window holds the socket but did not answer: this launch opens its own
    /// window and leaves that socket alone.
    Alone,
}

/// Hand `input` and this launch's activation token to the profile's running
/// window, or become that window.
pub fn launch(input: Option<&Path>) -> Launch {
    socket().map_or(Launch::Alone, |path| {
        launch_at(&path, input, activation_token().as_deref())
    })
}

fn launch_at(path: &Path, input: Option<&Path>, token: Option<&str>) -> Launch {
    // Twice: a launch that loses the race to bind hands its request to the winner.
    for _ in 0..2 {
        match hand_off(path, input, token) {
            Some(true) => return Launch::HandedOff,
            Some(false) => {
                eprintln!(
                    "instance: the running window did not answer within {ANSWER_WITHIN:?}; \
                     opening a separate window and leaving its socket"
                );
                return Launch::Alone;
            }
            None => {}
        }
        if let Some(instance) = Instance::claim(path) {
            return Launch::Serve(instance);
        }
    }
    Launch::Alone
}

/// Give `input` and `token` to the window listening at `path`: `None` when no
/// window listens there, otherwise whether it took the request in time.
fn hand_off(path: &Path, input: Option<&Path>, token: Option<&str>) -> Option<bool> {
    let mut stream = UnixStream::connect(path).ok()?;
    let _ = stream.set_read_timeout(Some(ANSWER_WITHIN));
    let mut reply = [0];
    match stream
        .write_all(&message(input, token))
        .and_then(|()| stream.read_exact(&mut reply))
    {
        Ok(()) => Some(reply == [1]),
        // The window closed as this launch connected; its socket is free now.
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => None,
        Err(_) => Some(false),
    }
}

/// What a launch sends: its path, then its token, each ended by NUL, which
/// neither can contain. Without a token it is the path alone, as before
/// 29 Oct; windows from before then read only the path. It goes in one write,
/// so the window finds the token buffered with the path.
fn message(input: Option<&Path>, token: Option<&str>) -> Vec<u8> {
    let mut message = input.map_or_else(Vec::new, |path| path.as_os_str().as_bytes().to_vec());
    message.push(0);
    if let Some(token) = token {
        message.extend_from_slice(token.as_bytes());
        message.push(0);
    }
    message
}

/// One field of a request, without the NUL that ends it.
fn field(reader: &mut impl BufRead) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_until(0, &mut bytes).ok()?;
    (bytes.pop() == Some(0)).then_some(bytes)
}

/// The profile's socket, bound by this process.
pub struct Instance {
    listener: UnixListener,
}

impl Instance {
    /// Bind the socket at `path`. A socket file that refuses connections was
    /// left by a window that ended, so it is replaced; one that accepts them
    /// belongs to a running window and is never removed.
    fn claim(path: &Path) -> Option<Self> {
        std::fs::create_dir_all(path.parent()?).ok()?;
        let listener = match UnixListener::bind(path) {
            Ok(listener) => listener,
            Err(error)
                if error.kind() == ErrorKind::AddrInUse
                    && UnixStream::connect(path)
                        .is_err_and(|error| error.kind() == ErrorKind::ConnectionRefused) =>
            {
                std::fs::remove_file(path).ok()?;
                UnixListener::bind(path).ok()?
            }
            Err(_) => return None,
        };
        Some(Self { listener })
    }

    /// Answer later launches from a thread: each request goes to the returned
    /// receiver and `wake` runs, so the window takes it on its next pass. The
    /// answer does not wait for that pass, which a busy or hidden window delays.
    pub fn serve(self, wake: impl Fn() + Send + 'static) -> Receiver<Request> {
        let (sender, receiver) = mpsc::channel();
        let listener = self.listener;
        let _ = std::thread::Builder::new()
            .name("rustidian-instance".into())
            .spawn(move || {
                loop {
                    let Ok((stream, _)) = listener.accept() else {
                        continue;
                    };
                    let _ = stream.set_read_timeout(Some(ANSWER_WITHIN));
                    let mut reader = BufReader::new(&stream);
                    let Some(path) = field(&mut reader) else {
                        continue;
                    };
                    let path = (!path.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(&path)));
                    // A token came in the same write as the path. A launch
                    // without one sends the path alone and waits for the
                    // answer, so reading on would stall it.
                    let token = (!reader.buffer().is_empty())
                        .then(|| field(&mut reader))
                        .flatten()
                        .and_then(|token| String::from_utf8(token).ok())
                        .filter(|token| !token.is_empty() && token.len() <= LONGEST_TOKEN);
                    if sender.send(Request { path, token }).is_err() {
                        // The window has closed. Stop listening before this
                        // launch sees its connection end, so it finds the
                        // socket abandoned and takes it over.
                        drop(listener);
                        return;
                    }
                    wake();
                    let _ = (&stream).write_all(&[1]);
                }
            });
        receiver
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;

    fn inode(path: &Path) -> u64 {
        std::fs::metadata(path).unwrap().ino()
    }

    #[test]
    fn a_busy_window_answers_a_hand_off_at_once() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        let Launch::Serve(instance) = launch_at(&path, None, None) else {
            panic!("The first launch must own the socket");
        };
        // The window's UI thread is busy: nothing reads its requests meanwhile.
        let requests = instance.serve(|| {});
        let note = Path::new("/vault/Note.md");
        assert!(
            matches!(launch_at(&path, Some(note), None), Launch::HandedOff),
            "A busy window must still take the hand-off"
        );
        assert_eq!(requests.try_recv().unwrap().path.as_deref(), Some(note));
    }

    #[test]
    fn a_launch_beside_a_silent_window_leaves_its_socket() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        // A running window that cannot answer: its socket accepts, nothing replies.
        let listener = UnixListener::bind(&path).unwrap();
        let before = inode(&path);
        assert!(
            matches!(launch_at(&path, None, None), Launch::Alone),
            "A launch must open its own window beside a window that does not answer"
        );
        assert_eq!(inode(&path), before, "The live socket was replaced");
        listener.set_nonblocking(true).unwrap();
        assert!(
            listener.accept().is_ok(),
            "The silent window's listener lost the launch's connection"
        );
    }

    #[test]
    fn a_socket_left_by_an_ended_window_is_replaced() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        // A window killed outright leaves its socket file with nobody listening.
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());
        assert!(matches!(launch_at(&path, None, None), Launch::Serve(_)));
    }

    #[test]
    fn a_launch_during_a_close_takes_over_the_socket() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        let Launch::Serve(instance) = launch_at(&path, None, None) else {
            panic!("The first launch must own the socket");
        };
        // The window has closed; its process is still ending.
        drop(instance.serve(|| {}));
        assert!(
            matches!(launch_at(&path, None, None), Launch::Serve(_)),
            "A launch while a window closes must become the next window"
        );
    }

    #[test]
    fn the_token_comes_from_xdg_activation_token_then_desktop_startup_id() {
        fn token(variables: &[(&str, &str)]) -> Option<String> {
            token_from(|name| {
                variables
                    .iter()
                    .find(|(variable, _)| *variable == name)
                    .map(|(_, value)| (*value).to_owned())
            })
        }
        let both = [
            ("XDG_ACTIVATION_TOKEN", "wayland"),
            ("DESKTOP_STARTUP_ID", "x11"),
        ];
        assert_eq!(token(&both).as_deref(), Some("wayland"));
        assert_eq!(token(&both[1..]).as_deref(), Some("x11"));
        let empty = [("XDG_ACTIVATION_TOKEN", ""), ("DESKTOP_STARTUP_ID", "x11")];
        assert_eq!(token(&empty).as_deref(), Some("x11"));
        assert_eq!(token(&[]), None);
    }

    #[test]
    fn a_token_follows_the_path_and_a_request_without_one_is_unchanged() {
        let note = Path::new("/vault/Note.md");
        for (token, sent) in [
            (None, &b"/vault/Note.md\0"[..]),
            (Some("token-1"), &b"/vault/Note.md\0token-1\0"[..]),
        ] {
            let folder = tempfile::tempdir().unwrap();
            let path = folder.path().join("instance.sock");
            let listener = UnixListener::bind(&path).unwrap();
            // A window from before 29 Oct: it reads up to the first NUL and answers.
            let window = std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(&stream);
                let mut received = Vec::new();
                reader.read_until(0, &mut received).unwrap();
                (&stream).write_all(&[1]).unwrap();
                // The launch closes its end once answered.
                reader.read_to_end(&mut received).unwrap();
                received
            });
            assert_eq!(hand_off(&path, Some(note), token), Some(true));
            assert_eq!(window.join().unwrap(), sent);
        }
    }

    #[test]
    fn a_hand_off_carries_the_launch_token() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        let Launch::Serve(instance) = launch_at(&path, None, None) else {
            panic!("The first launch must own the socket");
        };
        let requests = instance.serve(|| {});
        let note = Path::new("/vault/Note.md");
        for (input, token) in [
            (Some(note), Some("token-1")),
            (None, Some("token-2")),
            (Some(note), None),
        ] {
            assert!(matches!(launch_at(&path, input, token), Launch::HandedOff));
            let request = requests.try_recv().unwrap();
            assert_eq!(
                (request.path.as_deref(), request.token.as_deref()),
                (input, token)
            );
        }
    }

    /// A launch that opens the window leaves its token to eframe, which takes
    /// it out of the environment as it creates the window, before the app
    /// exists to start a child (fonts, gsettings, the picker, the Agent
    /// terminal). A launch that hands over starts no child.
    #[cfg(target_os = "linux")]
    #[test]
    fn creating_the_window_takes_the_token_out_of_the_environment() {
        const CHILD: &str = "RUSTIDIAN_TOKEN_TEST";
        let variables = ["XDG_ACTIVATION_TOKEN", "DESKTOP_STARTUP_ID"];
        if std::env::var_os(CHILD).is_some() {
            let _ = egui_winit::create_winit_window_attributes(
                &eframe::egui::Context::default(),
                eframe::egui::ViewportBuilder::default(),
            );
            assert_eq!(variables.map(std::env::var_os), [None, None]);
            println!("{CHILD}: taken");
            return;
        }
        // A process of its own: every test thread shares the environment.
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "instance::tests::creating_the_window_takes_the_token_out_of_the_environment",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env(variables[0], "wayland-token")
            .env(variables[1], "x11-token")
            .output()
            .unwrap();
        assert!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains(&format!("{CHILD}: taken")),
            "{output:?}"
        );
    }
}
