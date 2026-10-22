//! One owned native chooser for both the vault manager and system Open actions.
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum Kind {
    File,
    #[default]
    Folder,
}

#[derive(Default)]
pub struct Picker {
    child: Option<Child>,
    kind: Kind,
}

impl Picker {
    pub fn active(&self) -> bool {
        self.child.is_some()
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn start(&mut self, kind: Kind, initial: Option<&Path>, title: &str) -> Result<(), String> {
        let mut command = Command::new("zenity");
        command.args(["--file-selection", "--title", title]);
        match kind {
            Kind::Folder => {
                command.arg("--directory");
            }
            Kind::File => {
                command.args([
                    "--file-filter=Markdown and text | *.md *.MD *.txt *.TXT",
                    "--file-filter=All files | *",
                ]);
            }
        }
        if let Some(initial) = initial.filter(|path| !path.as_os_str().is_empty()) {
            command.arg("--filename").arg(initial.join(""));
        }
        self.spawn(kind, &mut command)
    }

    fn spawn(&mut self, kind: Kind, command: &mut Command) -> Result<(), String> {
        if self.active() {
            return Err("A file browser is already open. Finish or cancel it first.".into());
        }
        self.child = Some(
            command
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| format!("Cannot open the file browser: {error}"))?,
        );
        self.kind = kind;
        Ok(())
    }

    /// None means still waiting; Ok(None) is an explicit user cancellation.
    pub fn poll(&mut self) -> Option<Result<Option<PathBuf>, String>> {
        let status = match self.child.as_mut()?.try_wait() {
            Ok(Some(status)) => status,
            Ok(None) => return None,
            Err(error) => {
                self.cancel();
                return Some(Err(format!("Cannot read the file browser: {error}")));
            }
        };
        let mut child = self.child.take()?;
        if status.code() == Some(1) {
            return Some(Ok(None));
        }
        if !status.success() {
            return Some(Err(format!("The file browser failed ({status}).")));
        }
        Some((|| {
            let mut result = String::new();
            child
                .stdout
                .take()
                .ok_or("The file browser returned no path.")?
                .take(16_385)
                .read_to_string(&mut result)
                .map_err(|error| format!("Cannot read the selected path: {error}"))?;
            if result.len() > 16_384 {
                return Err("The selected path is too long.".into());
            }
            let path = result.strip_suffix('\n').unwrap_or(&result);
            if path.is_empty() {
                return Err("The file browser returned an empty path.".into());
            }
            Ok(Some(PathBuf::from(path)))
        })())
    }

    pub fn cancel(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for Picker {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn finished(picker: &mut Picker) -> Result<Option<PathBuf>, String> {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = picker.poll() {
                assert!(!picker.active());
                return result;
            }
            assert!(Instant::now() < deadline, "chooser failed to finish");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn chooser_preserves_unicode_spaces_and_a_filename_newline() {
        let mut picker = Picker::default();
        let expected = "/tmp/Notes سلام/ ending space \n";
        picker
            .spawn(
                Kind::File,
                Command::new("sh").args(["-c", "printf '%s\\n' \"$1\"", "picker-test", expected]),
            )
            .unwrap();
        assert_eq!(picker.kind(), Kind::File);
        assert_eq!(finished(&mut picker).unwrap(), Some(expected.into()));
        assert!(picker.poll().is_none());
    }

    #[test]
    fn cancelled_failed_and_empty_choosers_cannot_select_a_path() {
        for (script, cancelled) in [("exit 1", true), ("exit 2", false), ("exit 0", false)] {
            let mut picker = Picker::default();
            picker
                .spawn(Kind::Folder, Command::new("sh").args(["-c", script]))
                .unwrap();
            let result = finished(&mut picker);
            if cancelled {
                assert_eq!(result.unwrap(), None);
            } else {
                assert!(result.is_err());
            }
        }
    }

    #[test]
    fn pending_chooser_is_polled_without_waiting_and_cancel_reaps_it() {
        let mut picker = Picker::default();
        picker
            .spawn(Kind::Folder, Command::new("sleep").arg("30"))
            .unwrap();
        let pid = picker.child.as_ref().unwrap().id();
        assert!(picker.poll().is_none());
        assert!(
            picker
                .spawn(Kind::File, Command::new("sh").args(["-c", "exit 0"]))
                .is_err()
        );
        assert_eq!(picker.child.as_ref().unwrap().id(), pid);
        picker.cancel();
        assert!(!picker.active());
        assert!(!Path::new(&format!("/proc/{pid}")).exists());
    }
}
