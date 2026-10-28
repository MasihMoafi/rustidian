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

/// A later launch's file or folder (`None` brings the window forward).
pub struct Request {
    pub path: Option<PathBuf>,
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

/// Hand `input` to the profile's running window, or become that window.
pub fn launch(input: Option<&Path>) -> Launch {
    socket().map_or(Launch::Alone, |path| launch_at(&path, input))
}

fn launch_at(path: &Path, input: Option<&Path>) -> Launch {
    // Twice: a launch that loses the race to bind hands its request to the winner.
    for _ in 0..2 {
        match hand_off(path, input) {
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

/// Give `input` to the window listening at `path`: `None` when no window
/// listens there, otherwise whether it took the request in time.
fn hand_off(path: &Path, input: Option<&Path>) -> Option<bool> {
    let mut stream = UnixStream::connect(path).ok()?;
    let _ = stream.set_read_timeout(Some(ANSWER_WITHIN));
    // Paths cannot contain NUL, so it ends the message.
    let mut message = input.map_or_else(Vec::new, |path| path.as_os_str().as_bytes().to_vec());
    message.push(0);
    let mut reply = [0];
    match stream
        .write_all(&message)
        .and_then(|()| stream.read_exact(&mut reply))
    {
        Ok(()) => Some(reply == [1]),
        // The window closed as this launch connected; its socket is free now.
        Err(error) if error.kind() == ErrorKind::UnexpectedEof => None,
        Err(_) => Some(false),
    }
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
                    let mut message = Vec::new();
                    if BufReader::new(&stream).read_until(0, &mut message).is_err()
                        || message.pop() != Some(0)
                    {
                        continue;
                    }
                    let path =
                        (!message.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(&message)));
                    if sender.send(Request { path }).is_err() {
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
        let Launch::Serve(instance) = launch_at(&path, None) else {
            panic!("The first launch must own the socket");
        };
        // The window's UI thread is busy: nothing reads its requests meanwhile.
        let requests = instance.serve(|| {});
        let note = Path::new("/vault/Note.md");
        assert!(
            matches!(launch_at(&path, Some(note)), Launch::HandedOff),
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
            matches!(launch_at(&path, None), Launch::Alone),
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
        assert!(matches!(launch_at(&path, None), Launch::Serve(_)));
    }

    #[test]
    fn a_launch_during_a_close_takes_over_the_socket() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("instance.sock");
        let Launch::Serve(instance) = launch_at(&path, None) else {
            panic!("The first launch must own the socket");
        };
        // The window has closed; its process is still ending.
        drop(instance.serve(|| {}));
        assert!(
            matches!(launch_at(&path, None), Launch::Serve(_)),
            "A launch while a window closes must become the next window"
        );
    }
}
