//! Microphone recording and WhisperType transcription, kept off the UI thread.
use eframe::egui;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use tempfile::NamedTempFile;

pub struct Recording {
    child: Child,
    wav: NamedTempFile,
}

impl Drop for Recording {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = trash::delete(self.wav.path());
    }
}

impl Recording {
    pub fn start() -> Result<Self, String> {
        let wav = tempfile::Builder::new()
            .suffix(".wav")
            .tempfile()
            .map_err(|_| "Could not create a recording file.")?;
        let override_command = std::env::var_os("RUSTIDIAN_DICTATION_RECORDER");
        let mut commands = Vec::new();
        if let Some(program) = override_command {
            let mut command = Command::new(program);
            command.arg(wav.path());
            commands.push(command);
        } else {
            let mut pipewire = Command::new("pw-record");
            pipewire
                .args(["--rate", "16000", "--channels", "1", "--format", "s16"])
                .arg(wav.path());
            commands.push(pipewire);
            let mut alsa = Command::new("arecord");
            alsa.args(["-r", "16000", "-c", "1", "-f", "S16_LE", "-t", "wav"])
                .arg(wav.path());
            commands.push(alsa);
        }
        for mut command in commands {
            if let Ok(child) = command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                return Ok(Self { child, wav });
            }
        }
        Err("Could not start a microphone recorder.".into())
    }

    pub fn finish(
        mut self,
        backend: &'static str,
        ctx: egui::Context,
    ) -> Receiver<Result<String, String>> {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                if self
                    .child
                    .try_wait()
                    .map_err(|_| "Microphone recording failed.")?
                    .is_some()
                {
                    return Err("Microphone recording stopped. Check the microphone.".into());
                }
                #[cfg(unix)]
                if let Some(pid) = i32::try_from(self.child.id())
                    .ok()
                    .and_then(rustix::process::Pid::from_raw)
                {
                    rustix::process::kill_process(pid, rustix::process::Signal::INT)
                        .map_err(|_| "Could not stop the microphone.")?;
                }
                #[cfg(not(unix))]
                self.child
                    .kill()
                    .map_err(|_| "Could not stop the microphone.")?;
                self.child
                    .wait()
                    .map_err(|_| "Microphone recording failed.")?;
                if backend != "codex" && std::env::var_os("RUSTIDIAN_DICTATION_COMMAND").is_none() {
                    return transcribe_local(self.wav.path(), backend);
                }
                let program = std::env::var_os("RUSTIDIAN_DICTATION_COMMAND")
                    .unwrap_or_else(|| "whispertype".into());
                let output = Command::new(program)
                    .arg("transcribe")
                    .arg(self.wav.path())
                    .env("WHISPERTYPE_BACKEND", backend)
                    .output()
                    .map_err(|_| "Could not start WhisperType.")?;
                if !output.status.success() {
                    return Err(
                        "Transcription failed. Check WhisperType and its sign-in or model.".into(),
                    );
                }
                parse(&output.stdout)
            })();
            let _ = sender.send(result);
            ctx.request_repaint();
        });
        receiver
    }
}

// WhisperType's `transcribe FILE` currently routes non-Codex backends to
// OpenRouter, so use the same local engines and models as its recorder.
fn transcribe_local(wav: &std::path::Path, backend: &str) -> Result<String, String> {
    let executable = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|dir| dir.join("whispertype"))
        .find(|path| path.is_file())
        .and_then(|path| path.canonicalize().ok())
        .ok_or("WhisperType was not found.")?;
    let root = executable.parent().ok_or("WhisperType was not found.")?;
    let (mut command, libraries) = if backend == "local" {
        let base = root.join("whisper.cpp");
        let build = [base.join("build-whispertype"), base.join("build")]
            .into_iter()
            .find(|dir| dir.join("bin/whisper-cli").is_file())
            .ok_or("Local Whisper is not installed.")?;
        let mut command = Command::new(build.join("bin/whisper-cli"));
        command
            .arg("-m")
            .arg(base.join("models/ggml-medium.en.bin"))
            .arg("-f")
            .arg(wav)
            .args(["-nt", "-l", "en"]);
        let libs = ["bin", "src", "ggml/src", "ggml/src/ggml-cuda"].map(|part| build.join(part));
        (command, libs.to_vec())
    } else {
        let base = root.join("parakeet.cpp");
        let executable = [
            base.join("build/bin/parakeet-cli"),
            base.join("build/examples/cli/parakeet-cli"),
        ]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or("Local Parakeet is not installed.")?;
        let mut command = Command::new(executable);
        command
            .arg("transcribe")
            .arg("--model")
            .arg(base.join("models/tdt-0.6b-v2-f16.gguf"))
            .arg("--input")
            .arg(wav)
            .env("PARAKEET_DEVICE", "CUDA0");
        (
            command,
            vec![
                base.join("build/third_party/ggml/src"),
                base.join("build/third_party/ggml/src/ggml-cuda"),
            ],
        )
    };
    let mut paths = libraries;
    paths.extend(std::env::split_paths(
        &std::env::var_os("LD_LIBRARY_PATH").unwrap_or_default(),
    ));
    if let Ok(paths) = std::env::join_paths(paths) {
        command.env("LD_LIBRARY_PATH", paths);
    }
    let output = command
        .env("CUDA_VISIBLE_DEVICES", "0")
        .output()
        .map_err(|_| "Could not start the local transcription model.")?;
    if !output.status.success() {
        return Err("Local transcription failed. Check the WhisperType model.".into());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let cleaned = text
        .lines()
        .map(str::trim)
        .filter(|line| {
            !line.is_empty()
                && !["ggml_", "[parakeet]", "pk::", "system_info:"]
                    .iter()
                    .any(|prefix| line.starts_with(prefix))
        })
        .collect::<Vec<_>>()
        .join(" ");
    if cleaned.is_empty() {
        Err("No speech was transcribed.".into())
    } else {
        Ok(cleaned)
    }
}

fn parse(bytes: &[u8]) -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| "WhisperType returned an invalid transcript.")?;
    value
        .get("text")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "No speech was transcribed.".into())
}

#[cfg(test)]
mod tests {
    use super::parse;
    #[test]
    fn parses_transcript_and_rejects_bad_or_empty_results() {
        assert_eq!(
            parse("{\"text\":\"Hello, دنیا!\"}".as_bytes()).unwrap(),
            "Hello, دنیا!"
        );
        assert!(parse(br#"{"error":"not signed in"}"#).is_err());
        assert!(parse(br#"{"text":"  "}"#).is_err());
        assert!(parse(b"not json").is_err());
    }
}
