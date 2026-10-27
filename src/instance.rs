//! One process per profile, as in Obsidian: a later launch hands its file or
//! folder to the running window and exits instead of opening another window.
use eframe::egui;
use std::{
    ffi::OsStr,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        ffi::OsStrExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::Duration,
};

/// A later launch's file or folder, or `None` to bring the window forward.
pub type Request = Option<PathBuf>;

fn socket() -> Option<PathBuf> {
    rustidian::vaults::location().map(|catalog| catalog.with_file_name("instance.sock"))
}

/// Give `input` to the running window; returns whether one accepted it.
pub fn hand_off(input: Option<&Path>) -> bool {
    let Some(mut stream) = socket().and_then(|path| UnixStream::connect(path).ok()) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    // Paths cannot contain NUL, so it ends the message.
    let mut message = input.map_or_else(Vec::new, |path| path.as_os_str().as_bytes().to_vec());
    message.push(0);
    let mut reply = [0];
    stream.write_all(&message).is_ok() && stream.read_exact(&mut reply).is_ok() && reply == [1]
}

pub struct Instance {
    pub path: PathBuf,
    listener: UnixListener,
}

impl Instance {
    /// Own the profile's socket. Only a crashed process leaves one that
    /// refused the hand-off, so it is replaced.
    pub fn claim() -> Option<Self> {
        let path = socket()?;
        std::fs::create_dir_all(path.parent()?).ok()?;
        let listener = UnixListener::bind(&path).ok().or_else(|| {
            std::fs::remove_file(&path).ok()?;
            UnixListener::bind(&path).ok()
        })?;
        Some(Self { path, listener })
    }

    /// Deliver later launches' requests to the window, waking it for each.
    pub fn serve(self, ctx: egui::Context) -> Receiver<Request> {
        let (sender, receiver) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("rustidian-instance".into())
            .spawn(move || {
                for stream in self.listener.incoming().filter_map(Result::ok) {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let mut message = Vec::new();
                    if BufReader::new(&stream).read_until(0, &mut message).is_err()
                        || message.pop() != Some(0)
                    {
                        continue;
                    }
                    let request =
                        (!message.is_empty()).then(|| PathBuf::from(OsStr::from_bytes(&message)));
                    if sender.send(request).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                    let _ = (&stream).write_all(&[1]);
                }
            });
        receiver
    }
}
