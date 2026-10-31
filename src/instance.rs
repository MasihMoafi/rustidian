//! One process per profile, as in Obsidian: a later launch hands its file or
//! folder to the running window and exits instead of opening another window.
//! A running window that cannot be seen still gets the request but gives up
//! the socket: the launch then opens its own window, so a launch always
//! shows one.
use std::{
    ffi::OsStr,
    io::{BufRead, BufReader, ErrorKind, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

/// A later launch's file or folder (`None` brings the window forward), with
/// the activation token its launcher gave it.
pub struct Request {
    pub path: Option<PathBuf>,
    /// What lets the window raise itself on Wayland; see `window::bring_forward`.
    pub token: Option<String>,
    /// The window sends on it once it has the focus, and the launch exits.
    pub shown: Sender<()>,
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

/// How long a launch waits for the running window's answer. The window
/// answers from its own thread within `SHOWN_WITHIN`, even while its UI is
/// busy or hidden; one that does not answer is stopped or stuck.
const ANSWER_WITHIN: Duration = if cfg!(test) {
    Duration::from_millis(1500)
} else {
    Duration::from_secs(2)
};

/// How long a window that took a launch waits for the focus. On Masih's GNOME
/// Wayland session the launch's activation token did not raise a window on
/// another workspace (29 Oct), and the launch showed nothing.
const SHOWN_WITHIN: Duration = if cfg!(test) {
    Duration::from_millis(500)
} else {
    Duration::from_secs(1)
};

/// The window's answer once it has the focus. Windows from before 29 Oct
/// give it at once.
const SHOWN: u8 = 1;
/// The window's answer when it got no focus within `SHOWN_WITHIN`; it has
/// stopped serving launches.
const HIDDEN: u8 = 2;

fn socket() -> Option<PathBuf> {
    rustidian::vaults::location().map(|catalog| catalog.with_file_name("instance.sock"))
}

/// What a launch does.
pub enum Launch {
    /// The running window took the file or folder and has the focus; this
    /// launch exits.
    HandedOff,
    /// This launch opens the window and serves later launches from the
    /// profile's socket: no window ran, or the running one cannot be seen.
    Serve(Instance),
    /// The profile's socket cannot be bound: this launch opens its own window
    /// and serves nothing.
    Alone,
}

/// What the running window answered a launch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    /// It took the request and has the focus.
    Shown,
    /// It took the request, got no focus and stopped serving.
    Hidden,
    /// Nothing within `ANSWER_WITHIN`: it is stopped or stuck.
    Silent,
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
            Some(Answer::Shown) => {
                eprintln!("hand-off: shown");
                return Launch::HandedOff;
            }
            Some(answer) => {
                // Nothing would show. This launch opens a window where it was
                // made and takes the socket, so later launches reach that one.
                let why = if answer == Answer::Hidden {
                    "hidden"
                } else {
                    "no answer"
                };
                eprintln!("hand-off: {why}, opening a window");
                return Instance::take_over(path).map_or(Launch::Alone, Launch::Serve);
            }
            None => {}
        }
        if let Some(instance) = Instance::claim(path) {
            return Launch::Serve(instance);
        }
    }
    Launch::Alone
}

/// Give `input` and `token` to the window listening at `path` and wait for
/// its answer; `None` when no window listens there.
fn hand_off(path: &Path, input: Option<&Path>, token: Option<&str>) -> Option<Answer> {
    let mut stream = UnixStream::connect(path).ok()?;
    let _ = stream.set_read_timeout(Some(ANSWER_WITHIN));
    let mut reply = [0];
    match stream
        .write_all(&message(input, token))
        .and_then(|()| stream.read_exact(&mut reply))
    {
        Ok(()) if reply == [SHOWN] => Some(Answer::Shown),
        Ok(()) => Some(Answer::Hidden),
        // The window stopped listening as this launch connected: it closed,
        // or it gave its socket up to an earlier launch. The socket is free.
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::UnexpectedEof | ErrorKind::ConnectionReset | ErrorKind::BrokenPipe
            ) =>
        {
            None
        }
        Err(_) => Some(Answer::Silent),
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

/// Where `Instance::take_over` binds before it moves the socket to `path`.
fn fresh(path: &Path) -> PathBuf {
    path.with_extension(format!("{}.sock", std::process::id()))
}

/// The profile's socket, bound by this process.
pub struct Instance {
    listener: UnixListener,
}

impl Instance {
    /// Bind the socket at `path`. A socket file that refuses connections was
    /// left by a window that ended, so it is replaced; one that accepts them
    /// belongs to a running window, which gets the launch first.
    fn claim(path: &Path) -> Option<Self> {
        std::fs::create_dir_all(path.parent()?).ok()?;
        match UnixListener::bind(path) {
            Ok(listener) => Some(Self { listener }),
            Err(error)
                if error.kind() == ErrorKind::AddrInUse
                    && UnixStream::connect(path)
                        .is_err_and(|error| error.kind() == ErrorKind::ConnectionRefused) =>
            {
                Self::take_over(path)
            }
            Err(_) => None,
        }
    }

    /// Bind a socket of this process's own beside `path`, then move it over
    /// `path`, so later launches reach this window. The file there goes only
    /// once the new socket is bound: a failed bind leaves it as it was. A
    /// window still listening on it keeps running as an ordinary window.
    fn take_over(path: &Path) -> Option<Self> {
        let beside = fresh(path);
        // Named after this process, so no other running process owns it.
        let _ = std::fs::remove_file(&beside);
        let listener = UnixListener::bind(&beside).ok()?;
        if std::fs::rename(&beside, path).is_err() {
            let _ = std::fs::remove_file(&beside);
            return None;
        }
        Some(Self { listener })
    }

    /// Answer later launches from a thread. Each request goes to the returned
    /// receiver and `wake` runs, so the window takes it on its next pass; the
    /// launch exits once the window says through `Request::shown` that it has
    /// the focus. A window that gets none within `SHOWN_WITHIN`, busy or out
    /// of sight, answers `HIDDEN` and stops serving: the launch opens its own
    /// window and takes the socket. `wake` returns false while the window
    /// does not exist yet; a window still opening will be seen, so it answers
    /// `SHOWN` at once and takes the request on its first pass.
    pub fn serve(self, wake: impl Fn() -> bool + Send + 'static) -> Receiver<Request> {
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
                    let (shown, focused) = mpsc::channel();
                    if sender.send(Request { path, token, shown }).is_err() {
                        // The window has closed. Stop listening before this
                        // launch sees its connection end, so it finds the
                        // socket abandoned and takes it over.
                        drop(listener);
                        return;
                    }
                    let opening = !wake();
                    if opening || focused.recv_timeout(SHOWN_WITHIN).is_ok() {
                        let _ = (&stream).write_all(&[SHOWN]);
                        continue;
                    }
                    // Nobody sees this window. Stop listening first, so a
                    // launch that comes meanwhile finds the socket abandoned
                    // instead of waiting here; then this launch takes it.
                    drop(listener);
                    let _ = (&stream).write_all(&[HIDDEN]);
                    return;
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

    fn first_window(path: &Path) -> Instance {
        let Launch::Serve(instance) = launch_at(path, None, None) else {
            panic!("The first launch must own the socket");
        };
        instance
    }

    /// A window's UI with the focus: it passes on what each launch sent, then
    /// tells the launch it is shown.
    fn focused_window(requests: Receiver<Request>) -> Receiver<(Option<PathBuf>, Option<String>)> {
        let (taken, received) = mpsc::channel();
        std::thread::spawn(move || {
            for Request { path, token, shown } in requests {
                let _ = taken.send((path, token));
                let _ = shown.send(());
            }
        });
        received
    }

    #[test]
    fn a_launch_exits_once_the_window_has_the_focus() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        let instance = first_window(&path);
        let before = inode(&path);
        let received = focused_window(instance.serve(|| true));
        let note = Path::new("/vault/Note.md");
        for (input, token) in [
            (Some(note), Some("token-1")),
            (None, Some("token-2")),
            (Some(note), None),
        ] {
            assert!(
                matches!(launch_at(&path, input, token), Launch::HandedOff),
                "A window with the focus must take the launch"
            );
            assert_eq!(
                received.recv_timeout(Duration::from_secs(5)).unwrap(),
                (input.map(Path::to_path_buf), token.map(str::to_owned))
            );
        }
        assert_eq!(inode(&path), before, "A shown hand-off replaced the socket");
    }

    #[test]
    fn a_window_that_gets_no_focus_gives_the_launch_its_socket() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        let instance = first_window(&path);
        let before = inode(&path);
        // Out of sight, on another workspace, or busy: nothing reads the
        // requests, so the window never says it has the focus.
        let unseen = instance.serve(|| true);
        let note = Path::new("/vault/Note.md");
        let Launch::Serve(second) = launch_at(&path, Some(note), Some("token-1")) else {
            panic!(
                "A launch whose window got no focus must open its own window and own the socket"
            );
        };
        assert_eq!(
            unseen.try_recv().unwrap().path.as_deref(),
            Some(note),
            "The window that got no focus must still get the request"
        );
        assert_ne!(inode(&path), before, "The new window must own the socket");
        let received = focused_window(second.serve(|| true));
        assert!(
            matches!(launch_at(&path, None, Some("token-2")), Launch::HandedOff),
            "A later launch must reach the new window"
        );
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            (None, Some("token-2".to_owned()))
        );
        assert!(
            unseen.try_recv().is_err(),
            "The window that got no focus kept serving"
        );
    }

    #[test]
    fn a_launch_beside_a_silent_window_takes_its_socket() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        // A running window that cannot answer: its socket accepts, nothing replies.
        let silent = UnixListener::bind(&path).unwrap();
        let before = inode(&path);
        let Launch::Serve(instance) = launch_at(&path, None, None) else {
            panic!("A launch beside a window that does not answer must open its own window");
        };
        assert_ne!(inode(&path), before, "The new window must own the socket");
        silent.set_nonblocking(true).unwrap();
        assert!(
            silent.accept().is_ok(),
            "The silent window's listener lost the launch's connection"
        );
        let received = focused_window(instance.serve(|| true));
        assert!(
            matches!(launch_at(&path, None, None), Launch::HandedOff),
            "A later launch must reach the new window"
        );
        assert!(received.recv_timeout(Duration::from_secs(5)).is_ok());
        assert!(
            silent.accept().is_err(),
            "A later launch went to the silent window"
        );
    }

    #[test]
    fn a_socket_file_stays_until_its_replacement_is_bound() {
        for silent in [false, true] {
            let folder = tempfile::tempdir().unwrap();
            let path = folder.path().join("instance.sock");
            let listener = UnixListener::bind(&path).unwrap();
            // Left by a window that ended, or held by one that does not answer.
            let _held = silent.then_some(listener);
            let before = inode(&path);
            // A folder holds the place where the replacement would be bound.
            std::fs::create_dir(fresh(&path)).unwrap();
            assert!(matches!(launch_at(&path, None, None), Launch::Alone));
            assert_eq!(
                inode(&path),
                before,
                "The socket file went before its replacement was bound (silent window: {silent})"
            );
        }
    }

    #[test]
    fn a_socket_left_by_an_ended_window_is_replaced() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        // A window killed outright leaves its socket file with nobody listening.
        drop(UnixListener::bind(&path).unwrap());
        let before = inode(&path);
        assert!(matches!(launch_at(&path, None, None), Launch::Serve(_)));
        assert_ne!(inode(&path), before);
        assert!(
            !fresh(&path).exists(),
            "The replacement was left where it was bound"
        );
    }

    #[test]
    fn a_launch_during_a_close_takes_over_the_socket() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        let instance = first_window(&path);
        // The window has closed; its process is still ending.
        drop(instance.serve(|| true));
        assert!(
            matches!(launch_at(&path, None, None), Launch::Serve(_)),
            "A launch while a window closes must become the next window"
        );
    }

    #[test]
    fn a_window_still_opening_takes_a_launch_for_its_first_pass() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        // No window exists yet, so nothing reads the requests.
        let requests = first_window(&path).serve(|| false);
        let note = Path::new("/vault/Note.md");
        assert!(
            matches!(launch_at(&path, Some(note), None), Launch::HandedOff),
            "A window still opening will be seen, so the launch must hand over"
        );
        assert_eq!(requests.try_recv().unwrap().path.as_deref(), Some(note));
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
            assert_eq!(hand_off(&path, Some(note), token), Some(Answer::Shown));
            assert_eq!(window.join().unwrap(), sent);
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
