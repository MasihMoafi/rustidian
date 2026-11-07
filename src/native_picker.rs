//! Native file dialogs and system file-manager actions.
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    #[default]
    Folder,
}

/// Select an item in its parent using the desktop file-manager interface.
/// https://wiki.freedesktop.org/www/Specifications/file-manager-interface/
pub fn reveal(path: &Path) -> Result<(), String> {
    const URI_PATH: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC.remove(b'/');
    // A comma is an array separator in dbus-send; encode it with the other URI bytes.
    let uri = format!(
        "file://{}",
        percent_encoding::percent_encode(path.as_os_str().as_encoded_bytes(), URI_PATH,)
    );
    let output = Command::new("dbus-send")
        .args([
            "--session",
            "--type=method_call",
            "--print-reply",
            "--reply-timeout=5000",
            "--dest=org.freedesktop.FileManager1",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("array:string:{uri}"),
            "string:",
        ])
        .output()
        .map_err(|error| format!("Cannot open the system file manager: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Cannot open the system file manager: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

type PickResult = Result<Option<PathBuf>, String>;

#[derive(Default)]
pub struct Picker {
    pending: Option<(mpsc::Receiver<PickResult>, async_channel::Sender<()>)>,
    kind: Kind,
}

impl Picker {
    pub fn active(&self) -> bool {
        self.pending.is_some()
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn start(&mut self, kind: Kind, initial: Option<&Path>, title: &str) -> Result<(), String> {
        if self.active() {
            return Err("A file browser is already open. Finish or cancel it first.".into());
        }
        let initial = initial
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_owned);
        let title = title.to_owned();
        let (result_tx, result_rx) = mpsc::channel();
        let (cancel_tx, cancel_rx) = async_channel::bounded(1);
        std::thread::Builder::new()
            .name("file-picker".into())
            .spawn(move || {
                let result = run_picker(kind, initial.as_deref(), &title, cancel_rx);
                let _ = result_tx.send(result);
            })
            .map_err(|e| format!("Cannot open the file browser: {e}"))?;
        self.kind = kind;
        self.pending = Some((result_rx, cancel_tx));
        Ok(())
    }

    /// None means still waiting; Ok(None) is an explicit user cancellation.
    pub fn poll(&mut self) -> Option<PickResult> {
        let result = match self.pending.as_ref()?.0.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err("The file browser stopped unexpectedly.".into())
            }
        };
        self.pending = None;
        Some(result)
    }

    pub fn cancel(&mut self) {
        if let Some((_, cancel)) = self.pending.take() {
            let _ = cancel.try_send(());
        }
    }
}

impl Drop for Picker {
    fn drop(&mut self) {
        self.cancel();
    }
}

// Only a failure before a response warrants the legacy chooser. Cancellation and
// response errors are final, so closing a portal dialog never opens another one.
enum PortalResult {
    Unavailable,
    Finished(PickResult),
}

fn run_picker(
    kind: Kind,
    initial: Option<&Path>,
    title: &str,
    cancel: async_channel::Receiver<()>,
) -> PickResult {
    #[cfg(target_os = "linux")]
    let result = futures_lite::future::block_on(portal(kind, initial, title, &cancel));
    #[cfg(not(target_os = "linux"))]
    let result = PortalResult::Unavailable;
    choose_backend(result, || zenity(kind, initial, title, &cancel))
}

fn choose_backend(portal: PortalResult, fallback: impl FnOnce() -> PickResult) -> PickResult {
    match portal {
        PortalResult::Unavailable => fallback(),
        PortalResult::Finished(result) => result,
    }
}

#[cfg(target_os = "linux")]
async fn portal(
    kind: Kind,
    initial: Option<&Path>,
    title: &str,
    cancel: &async_channel::Receiver<()>,
) -> PortalResult {
    use ashpd::{
        desktop::{file_chooser::SelectedFiles, ResponseError},
        Error,
    };
    let connection = match zbus::Connection::session().await {
        Ok(connection) => connection,
        Err(_) => return PortalResult::Unavailable,
    };
    let mut request = SelectedFiles::open_file()
        .title(title)
        .directory(kind == Kind::Folder)
        .connection(Some(connection.clone()));
    if let Some(folder) = initial {
        request = match request.current_folder(folder) {
            Ok(request) => request,
            Err(error) => {
                return PortalResult::Finished(Err(format!(
                    "Cannot set the initial folder: {error}"
                )))
            }
        };
    }
    let outcome = futures_lite::future::race(async { Some(request.send().await) }, async {
        let _ = cancel.recv().await;
        None
    })
    .await;
    // Closing this private bus connection dismisses any outstanding request.
    let _ = connection.close().await;
    match outcome {
        None => PortalResult::Finished(Ok(None)),
        Some(Ok(request)) => match request.response() {
            Ok(files) => PortalResult::Finished(
                files
                    .uris()
                    .first()
                    .ok_or_else(|| "The file browser returned no path.".to_owned())
                    .and_then(|uri| file_uri_to_path(uri.as_str()))
                    .map(Some),
            ),
            Err(Error::Response(ResponseError::Cancelled)) => PortalResult::Finished(Ok(None)),
            Err(error) => PortalResult::Finished(Err(format!("The file browser failed: {error}"))),
        },
        Some(Err(Error::NoResponse)) => {
            PortalResult::Finished(Err("The portal stopped responding.".into()))
        }
        Some(Err(_)) => PortalResult::Unavailable,
    }
}

#[cfg(target_os = "linux")]
fn file_uri_to_path(uri: &str) -> Result<PathBuf, String> {
    url::Url::parse(uri)
        .map_err(|e| format!("Invalid file URI: {e}"))?
        .to_file_path()
        .map_err(|_| "The file browser returned a non-local file URI.".into())
}

fn zenity(
    kind: Kind,
    initial: Option<&Path>,
    title: &str,
    cancel: &async_channel::Receiver<()>,
) -> PickResult {
    if cancel.try_recv().is_ok() {
        return Ok(None);
    }
    let mut command = Command::new("zenity");
    command.args(["--file-selection", "--title", title]);
    match kind {
        Kind::Folder => {
            command.arg("--directory");
        }
        Kind::File => {
            command.args([
            "--file-filter=Notes and source code | *.md *.MD *.txt *.TXT *.rs *.py *.js *.ts *.json *.toml *.yaml *.yml *.html *.css *.c *.cpp *.h *.sh",
            "--file-filter=All files | *",
        ]);
        }
    }
    if let Some(initial) = initial {
        command.arg("--filename").arg(initial.join(""));
    }
    run_zenity(&mut command, cancel)
}

fn run_zenity(command: &mut Command, cancel: &async_channel::Receiver<()>) -> PickResult {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot open the file browser: {e}"))?;
    let status = loop {
        if cancel.try_recv().is_ok() {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(None);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Cannot read the file browser: {error}"));
            }
        }
    };
    if status.code() == Some(1) {
        return Ok(None);
    }
    if !status.success() {
        return Err(format!("The file browser failed ({status})."));
    }
    let mut result = String::new();
    child
        .stdout
        .take()
        .ok_or("The file browser returned no path.")?
        .take(16_385)
        .read_to_string(&mut result)
        .map_err(|e| format!("Cannot read the selected path: {e}"))?;
    if result.len() > 16_384 {
        return Err("The selected path is too long.".into());
    }
    let path = result.strip_suffix('\n').unwrap_or(&result);
    if path.is_empty() {
        return Err("The file browser returned an empty path.".into());
    }
    Ok(Some(PathBuf::from(path)))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStrExt;

    #[test]
    fn legacy_chooser_preserves_paths_and_reports_cancellation_and_failure() {
        let (_sender, cancel) = async_channel::bounded(1);
        let expected = "/tmp/Notes سلام/ ending space \n";
        let mut command = Command::new("sh");
        command.args(["-c", "printf '%s\\n' \"$1\"", "picker-test", expected]);
        assert_eq!(
            run_zenity(&mut command, &cancel).unwrap(),
            Some(expected.into())
        );
        for (script, cancelled) in [("exit 1", true), ("exit 2", false), ("exit 0", false)] {
            let mut command = Command::new("sh");
            command.args(["-c", script]);
            let result = run_zenity(&mut command, &cancel);
            if cancelled {
                assert_eq!(result.unwrap(), None);
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn fallback_is_used_only_when_portal_is_unavailable() {
        let fallback = || Ok(Some(PathBuf::from("/fallback")));
        assert_eq!(
            choose_backend(PortalResult::Unavailable, fallback).unwrap(),
            Some(PathBuf::from("/fallback"))
        );
        assert_eq!(
            choose_backend(PortalResult::Finished(Ok(None)), fallback).unwrap(),
            None
        );
        assert!(choose_backend(
            PortalResult::Finished(Err("portal failed".into())),
            fallback
        )
        .is_err());
    }

    #[test]
    fn file_uris_preserve_path_bytes() {
        assert_eq!(
            file_uri_to_path("file:///tmp/a%20b%25.txt").unwrap(),
            PathBuf::from("/tmp/a b%.txt")
        );
        assert_eq!(
            file_uri_to_path("file:///tmp/%FF")
                .unwrap()
                .as_os_str()
                .as_bytes(),
            b"/tmp/\xff"
        );
        assert!(file_uri_to_path("https://example.org/file").is_err());
        assert!(file_uri_to_path("file://remote-host/file").is_err());
    }
}
