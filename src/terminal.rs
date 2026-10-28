//! A coding agent's own terminal interface, run in a pseudo-terminal inside the
//! vault: which agents are installed, the environment they get, and one running
//! session with its screen.
use portable_pty::{CommandBuilder, MasterPty, PtySize};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    io::{ErrorKind, PipeReader, PipeWriter, Read, Write},
    os::fd::BorrowedFd,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError, mpsc},
    thread::JoinHandle,
    time::{Duration, Instant},
};

/// Lines kept above the screen for the mouse wheel.
pub const SCROLLBACK: usize = 1000;

/// How long a closing agent gets between SIGTERM and SIGKILL.
const GRACE: Duration = Duration::from_secs(2);

/// The agents the Agent tab runs, in menu order; the first installed one is the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    #[default]
    Elpis,
    Codex,
    Claude,
}

impl Backend {
    pub const ALL: [Self; 3] = [Self::Elpis, Self::Codex, Self::Claude];

    pub fn label(self) -> &'static str {
        match self {
            Self::Elpis => "Elpis",
            Self::Codex => "Codex",
            Self::Claude => "Claude",
        }
    }

    /// The command that starts the agent's terminal interface.
    pub fn program(self) -> &'static str {
        match self {
            Self::Elpis => "elpis",
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

/// The agents installed in `path` (a `PATH` value), in menu order, each with the
/// executable that runs it. Relative folders are skipped.
pub fn discover(path: &OsStr) -> Vec<(Backend, PathBuf)> {
    use std::os::unix::fs::PermissionsExt;
    let executable = |file: &Path| {
        std::fs::metadata(file)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    };
    Backend::ALL
        .into_iter()
        .filter_map(|backend| {
            std::env::split_paths(path)
                .filter(|folder| folder.is_absolute())
                .map(|folder| folder.join(backend.program()))
                .find(|file| executable(file))
                .map(|file| (backend, file))
        })
        .collect()
}

/// The environment an agent starts with: this process's, without the variables
/// of an agent session Rustidian may have been started from (an Elpis that
/// inherits them joins and locks that session's shared bridge), as a terminal
/// with 256 and true colours.
pub fn child_env(
    parent: impl IntoIterator<Item = (OsString, OsString)>,
) -> Vec<(OsString, OsString)> {
    let mut environment: Vec<_> = parent
        .into_iter()
        .filter(|(name, _)| {
            let name = name.as_encoded_bytes();
            !matches!(name, b"TERM" | b"COLORTERM")
                && !["CLAUDE", "AI_AGENT", "CODEX"]
                    .iter()
                    .any(|prefix| name.starts_with(prefix.as_bytes()))
        })
        .collect();
    environment.extend([
        ("TERM".into(), "xterm-256color".into()),
        ("COLORTERM".into(), "truecolor".into()),
    ]);
    environment
}

/// Answers to the queries terminal programs send at startup: where the cursor
/// is, what kind of terminal this is and its colours. Some wait seconds for them.
#[derive(Default)]
pub struct Replies {
    answer: Vec<u8>,
    /// The default foreground and background, reported to colour queries.
    pub colors: [[u8; 3]; 2],
}

impl vt100::Callbacks for Replies {
    fn unhandled_csi(
        &mut self,
        screen: &mut vt100::Screen,
        intermediate: Option<u8>,
        _: Option<u8>,
        params: &[&[u16]],
        command: char,
    ) {
        let first = params
            .first()
            .and_then(|param| param.first())
            .copied()
            .unwrap_or(0);
        match (intermediate, command, first) {
            (None, 'n', 5) => self.answer.extend_from_slice(b"\x1b[0n"),
            (None, 'n', 6) => {
                let (row, col) = screen.cursor_position();
                self.answer
                    .extend_from_slice(format!("\x1b[{};{}R", row + 1, col + 1).as_bytes());
            }
            // A VT220 with ANSI colour.
            (None, 'c', 0) => self.answer.extend_from_slice(b"\x1b[?62;22c"),
            _ => {}
        }
    }

    fn unhandled_osc(&mut self, _: &mut vt100::Screen, params: &[&[u8]]) {
        let index = match params {
            [b"10", b"?"] => 0,
            [b"11", b"?"] => 1,
            _ => return,
        };
        let [red, green, blue] = self.colors[index];
        self.answer.extend_from_slice(
            format!(
                "\x1b]{};rgb:{red:02x}{red:02x}/{green:02x}{green:02x}/{blue:02x}{blue:02x}\x1b\\",
                10 + index
            )
            .as_bytes(),
        );
    }
}

pub type Parser = vt100::Parser<Replies>;

/// One agent process on a pseudo-terminal. Dropping the session ends it: the
/// terminal closes, then SIGTERM, then SIGKILL two seconds later.
pub struct Session {
    pid: u32,
    size: (u16, u16),
    parser: Arc<Mutex<Parser>>,
    master: Option<Box<dyn MasterPty + Send>>,
    input: Option<mpsc::Sender<Vec<u8>>>,
    wake: Option<PipeWriter>,
    reader: Option<JoinHandle<()>>,
    writer: Option<JoinHandle<()>>,
    exited: mpsc::Receiver<u32>,
    code: Option<u32>,
}

impl Session {
    /// Start `program` in `folder` on a terminal of `size` (rows, columns).
    /// Other threads call `repaint` whenever the screen changes or the program ends.
    pub fn start(
        program: &Path,
        folder: &Path,
        size: (u16, u16),
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let pair = portable_pty::native_pty_system()
            .openpty(PtySize {
                rows: size.0,
                cols: size.1,
                ..Default::default()
            })
            .map_err(|error| format!("{error:#}"))?;
        let descriptor = pair
            .master
            .as_raw_fd()
            .ok_or("The terminal has no file descriptor.")?;
        // SAFETY: `pair.master` owns this open descriptor for the whole borrow.
        let borrowed = unsafe { BorrowedFd::borrow_raw(descriptor) };
        // Own handles, not portable-pty's writer: dropping that one types a
        // newline and EOF, which would submit an unsent draft as the agent closes.
        let duplicate = || {
            borrowed
                .try_clone_to_owned()
                .map(File::from)
                .map_err(|error| error.to_string())
        };
        let (output, keys) = (duplicate()?, duplicate()?);
        let (wake_reader, wake) = std::io::pipe().map_err(|error| error.to_string())?;
        let mut command = CommandBuilder::new(program);
        command.cwd(folder);
        command.env_clear();
        for (name, value) in child_env(std::env::vars_os()) {
            command.env(name, value);
        }
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("{error:#}"))?;
        // Only the agent and its children keep the terminal's other side open,
        // so the output ends when they do.
        drop(pair.slave);
        let pid = child.process_id().unwrap_or_default();
        let parser = Arc::new(Mutex::new(Parser::new_with_callbacks(
            size.0,
            size.1,
            SCROLLBACK,
            Replies::default(),
        )));
        let repaint = Arc::new(repaint);
        let (input, queue) = mpsc::channel::<Vec<u8>>();
        let writer = std::thread::spawn(move || {
            let mut keys = keys;
            for bytes in queue {
                if keys.write_all(&bytes).is_err() {
                    return;
                }
            }
        });
        let reader = {
            let (parser, replies, repaint) = (parser.clone(), input.clone(), repaint.clone());
            std::thread::spawn(move || read(&output, &wake_reader, &parser, &replies, &*repaint))
        };
        let (done, exited) = mpsc::channel();
        std::thread::spawn(move || {
            let code = child.wait().map_or(1, |status| status.exit_code());
            let _ = done.send(code);
            repaint();
        });
        Ok(Self {
            pid,
            size,
            parser,
            master: Some(pair.master),
            input: Some(input),
            wake: Some(wake),
            reader: Some(reader),
            writer: Some(writer),
            exited,
            code: None,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// The screen and its scrollback; hold it only while drawing.
    pub fn lock(&self) -> MutexGuard<'_, Parser> {
        self.parser.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Type into the agent; ignored once it has ended.
    pub fn write(&self, bytes: Vec<u8>) {
        if let Some(input) = &self.input
            && !bytes.is_empty()
        {
            let _ = input.send(bytes);
        }
    }

    /// Follow the panel's size; the agent gets SIGWINCH and redraws.
    pub fn resize(&mut self, size: (u16, u16)) {
        if size == self.size {
            return;
        }
        self.size = size;
        self.lock().screen_mut().set_size(size.0, size.1);
        if let Some(master) = &self.master {
            let _ = master.resize(PtySize {
                rows: size.0,
                cols: size.1,
                ..Default::default()
            });
        }
    }

    /// The exit code once the agent has ended.
    pub fn exit_code(&mut self) -> Option<u32> {
        if self.code.is_none() {
            self.code = self.exited.try_recv().ok();
        }
        self.code
    }

    /// Signal the agent's process group: the agent and the children it kept in it.
    fn signal(&self, signal: rustix::process::Signal) {
        if let Some(group) = i32::try_from(self.pid)
            .ok()
            .and_then(rustix::process::Pid::from_raw)
        {
            let _ = rustix::process::kill_process_group(group, signal);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Close the terminal: wake the reader so it lets go of its handle, end
        // the writer and drop the master. With every handle gone the kernel
        // hangs up the agent's terminal.
        drop(self.wake.take());
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
        drop(self.input.take());
        drop(self.master.take());
        if let Some(writer) = self.writer.take() {
            // A writer blocked on an agent that stopped reading ends with it.
            let deadline = Instant::now() + Duration::from_millis(100);
            while !writer.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        if self.exit_code().is_some() {
            return;
        }
        self.signal(rustix::process::Signal::TERM);
        if self.exited.recv_timeout(GRACE).is_err() {
            self.signal(rustix::process::Signal::KILL);
        }
    }
}

/// Feed the agent's output to the screen until it ends or the session closes,
/// answering its terminal queries.
fn read(
    mut output: &File,
    wake: &PipeReader,
    parser: &Mutex<Parser>,
    replies: &mpsc::Sender<Vec<u8>>,
    repaint: &dyn Fn(),
) {
    use rustix::event::{PollFd, PollFlags, poll};
    let mut buffer = vec![0; 1 << 16];
    loop {
        let mut ready = [
            PollFd::new(output, PollFlags::IN),
            PollFd::new(wake, PollFlags::IN),
        ];
        match poll(&mut ready, None) {
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return,
        }
        if !ready[1].revents().is_empty() {
            return;
        }
        let count = match output.read(&mut buffer) {
            Ok(0) => return,
            Ok(count) => count,
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            // EIO: nothing holds the terminal's other side any more.
            Err(_) => return,
        };
        let answer = {
            let mut parser = parser.lock().unwrap_or_else(PoisonError::into_inner);
            parser.process(&buffer[..count]);
            // New output brings the view back to the bottom.
            parser.screen_mut().set_scrollback(0);
            std::mem::take(&mut parser.callbacks_mut().answer)
        };
        if !answer.is_empty() {
            let _ = replies.send(answer);
        }
        repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_start_without_the_calling_sessions_variables_as_a_colour_terminal() {
        let parent = [
            ("PATH", "/usr/bin:/bin"),
            ("HOME", "/home/someone"),
            ("CLAUDE_CONFIG_DIR", "/home/someone/.claude-agent"),
            ("CLAUDECODE", "1"),
            ("AI_AGENT_SESSION", "fable"),
            ("CODEX_HOME", "/tmp/codex"),
            ("CODEX_SANDBOX", "seatbelt"),
            ("TERM", "dumb"),
            ("TERM_PROGRAM", "vscode"),
            ("MY_CLAUDE_NOTES", "kept"),
        ]
        .map(|(name, value)| (OsString::from(name), OsString::from(value)));
        let environment = child_env(parent);
        let names: Vec<_> = environment
            .iter()
            .map(|(name, _)| name.to_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "PATH",
                "HOME",
                "TERM_PROGRAM",
                "MY_CLAUDE_NOTES",
                "TERM",
                "COLORTERM"
            ]
        );
        let value = |wanted: &str| {
            environment
                .iter()
                .find(|(name, _)| name == wanted)
                .map(|(_, value)| value.to_str().unwrap())
        };
        assert_eq!(value("PATH"), Some("/usr/bin:/bin"));
        assert_eq!(value("TERM"), Some("xterm-256color"));
        assert_eq!(value("COLORTERM"), Some("truecolor"));
    }

    #[test]
    fn only_installed_agents_are_offered_in_menu_order() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let (first, second) = (root.path().join("first"), root.path().join("second"));
        for folder in [&first, &second] {
            std::fs::create_dir(folder).unwrap();
        }
        let install = |file: PathBuf, mode: u32| {
            std::fs::write(&file, "#!/bin/sh\n").unwrap();
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
        };
        install(first.join("claude"), 0o755);
        install(first.join("elpis"), 0o644);
        install(second.join("elpis"), 0o755);
        // A folder named like an agent is not a program.
        std::fs::create_dir(second.join("codex")).unwrap();
        let path = std::env::join_paths([
            Path::new("relative-folder"),
            first.as_path(),
            second.as_path(),
        ])
        .unwrap();
        assert_eq!(
            discover(&path),
            [
                (Backend::Elpis, second.join("elpis")),
                (Backend::Claude, first.join("claude"))
            ]
        );
        assert!(discover(OsStr::new("")).is_empty());
    }

    #[test]
    fn terminal_queries_get_the_answers_programs_wait_for() {
        let mut parser = Parser::new_with_callbacks(
            24,
            80,
            0,
            Replies {
                colors: [[0xe9, 0xe5, 0xdc], [0x11, 0x15, 0x17]],
                ..Default::default()
            },
        );
        parser.process(b"abc\x1b[6n\x1b[c\x1b]11;?\x1b\\\x1b[5n");
        assert_eq!(
            parser.callbacks().answer,
            b"\x1b[1;4R\x1b[?62;22c\x1b]11;rgb:1111/1515/1717\x1b\\\x1b[0n"
        );
    }

    /// A stand-in agent: a shell script in a private folder.
    fn script(folder: &Path, body: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let file = folder.join("agent");
        std::fs::write(&file, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        file
    }

    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(Instant::now() < deadline, "timed out waiting for {what}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_session_runs_the_agent_in_its_folder_on_a_sized_terminal() {
        let folder = tempfile::tempdir().unwrap();
        let program = script(
            folder.path(),
            "printf 'in %s as %s at %s\\n' \"$(pwd)\" \"$TERM\" \"$(stty size)\"\nread line\necho \"got $line\"\nexit 3\n",
        );
        let mut session = Session::start(&program, folder.path(), (12, 100), || {}).unwrap();
        let contents = |session: &Session| session.lock().screen().contents();
        let greeting = format!("in {} as xterm-256color at 12 100", folder.path().display());
        wait_until("the greeting", || contents(&session).contains(&greeting));
        session.write(b"hello\r".to_vec());
        wait_until("the reply", || contents(&session).contains("got hello"));
        wait_until("the exit", || session.exit_code().is_some());
        assert_eq!(session.exit_code(), Some(3));
    }

    #[test]
    fn closing_a_session_ends_an_agent_that_ignores_hangup_and_terminate() {
        let folder = tempfile::tempdir().unwrap();
        let program = script(
            folder.path(),
            "trap '' HUP TERM\necho ready\nwhile :; do sleep 1; done\n",
        );
        let session = Session::start(&program, folder.path(), (10, 40), || {}).unwrap();
        wait_until("the agent", || {
            session.lock().screen().contents().contains("ready")
        });
        let group = i32::try_from(session.pid())
            .ok()
            .and_then(rustix::process::Pid::from_raw)
            .unwrap();
        let started = Instant::now();
        drop(session);
        let waited = started.elapsed();
        assert!(
            waited >= Duration::from_millis(1900),
            "SIGKILL came before the grace period: {waited:?}"
        );
        wait_until("the process group to end", || {
            rustix::process::test_kill_process_group(group).is_err()
        });
    }
}
