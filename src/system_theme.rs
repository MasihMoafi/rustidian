use super::Appearance;
use eframe::egui;

/// Native integrations supply this on macOS/Windows. Winit's Linux backend does
/// not report GNOME appearance, so listen to its settings without frame polling.
#[derive(Default)]
pub struct SystemTheme {
    #[cfg(target_os = "linux")]
    linux: linux::Settings,
}

impl SystemTheme {
    pub fn update(
        &mut self,
        ctx: &egui::Context,
        raw: &mut egui::RawInput,
        appearance: Appearance,
    ) {
        #[cfg(target_os = "linux")]
        self.linux.update(ctx, raw, appearance);
        #[cfg(not(target_os = "linux"))]
        let _ = (ctx, raw, appearance);
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Appearance, egui};
    use std::{
        io::{BufRead, BufReader, Read},
        process::{Child, ChildStdout, Command, Stdio},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, Ordering},
            mpsc::{self, Receiver, TryRecvError},
        },
        thread::JoinHandle,
        time::{Duration, Instant},
    };

    const SCHEMA: &str = "org.gnome.desktop.interface";

    #[derive(Default)]
    pub(super) struct Settings {
        attempted: bool,
        watcher: Option<Watcher>,
        theme: Option<egui::Theme>,
    }

    impl Settings {
        pub(super) fn update(
            &mut self,
            ctx: &egui::Context,
            raw: &mut egui::RawInput,
            appearance: Appearance,
        ) {
            if appearance != Appearance::System {
                *self = Self::default();
                return;
            }
            if raw.system_theme.is_some() {
                return;
            }
            if !self.attempted {
                self.attempted = true;
                self.watcher = Watcher::start(ctx.clone()).ok();
            }
            if let Some(watcher) = &self.watcher {
                loop {
                    match watcher.events.try_recv() {
                        Ok(theme) => self.theme = theme,
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            self.watcher = None;
                            break;
                        }
                    }
                }
            }
            raw.system_theme = self.theme;
        }
    }

    struct Watcher {
        events: Receiver<Option<egui::Theme>>,
        stopped: Arc<AtomicBool>,
        children: Arc<Children>,
        worker: Option<JoinHandle<()>>,
    }

    impl Watcher {
        fn start(ctx: egui::Context) -> std::io::Result<Self> {
            let (sender, events) = mpsc::channel();
            let stopped = Arc::new(AtomicBool::new(false));
            let children = Arc::new(Children::default());
            let worker_stopped = stopped.clone();
            let worker_children = children.clone();
            let worker = std::thread::Builder::new()
                .name("system-appearance".into())
                .spawn(move || {
                    let Some(stdout) = worker_children.spawn(
                        0,
                        Command::new("gsettings").args(["monitor", SCHEMA]),
                        &worker_stopped,
                    ) else {
                        return;
                    };
                    let read = |key| worker_children.read_setting(key, &worker_stopped);
                    let mut color_scheme = read("color-scheme");
                    let mut gtk_theme = read("gtk-theme");
                    let mut previous = theme(&color_scheme, &gtk_theme);
                    if sender.send(previous).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                        if worker_stopped.load(Ordering::Acquire) {
                            break;
                        }
                        if let Some(value) = line.strip_prefix("color-scheme: ") {
                            color_scheme = value.into();
                        } else if let Some(value) = line.strip_prefix("gtk-theme: ") {
                            gtk_theme = value.into();
                        } else {
                            continue;
                        }
                        let current = theme(&color_scheme, &gtk_theme);
                        if current != previous {
                            previous = current;
                            if sender.send(current).is_err() {
                                return;
                            }
                            ctx.request_repaint();
                        }
                    }
                    worker_children.stop();
                })?;
            Ok(Self {
                events,
                stopped,
                children,
                worker: Some(worker),
            })
        }
    }

    impl Drop for Watcher {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::Release);
            self.children.stop();
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
        }
    }

    /// Slot 0 owns the long-lived monitor; slot 1 owns a bounded initial read.
    /// Sharing ownership lets cancellation interrupt either child immediately.
    #[derive(Default)]
    struct Children(Mutex<[Option<Child>; 2]>);

    impl Children {
        fn spawn(
            &self,
            slot: usize,
            command: &mut Command,
            stopped: &AtomicBool,
        ) -> Option<ChildStdout> {
            if stopped.load(Ordering::Acquire) {
                return None;
            }
            let mut child = command
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
            let stdout = child.stdout.take();
            let mut children = self.0.lock().expect("Owned appearance children");
            if stopped.load(Ordering::Acquire) {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            children[slot] = Some(child);
            stdout
        }

        fn read_setting(&self, key: &str, stopped: &AtomicBool) -> String {
            let Some(stdout) = self.spawn(
                1,
                Command::new("gsettings").args(["get", SCHEMA, key]),
                stopped,
            ) else {
                return String::new();
            };
            let started = Instant::now();
            let success = loop {
                let status = self.0.lock().expect("Owned appearance children")[1]
                    .as_mut()
                    .and_then(|child| child.try_wait().ok().flatten());
                if let Some(status) = status {
                    break status.success();
                }
                if stopped.load(Ordering::Acquire) || started.elapsed() >= Duration::from_secs(2) {
                    break false;
                }
                std::thread::sleep(Duration::from_millis(10));
            };
            if let Some(mut child) = self.0.lock().expect("Owned appearance children")[1].take() {
                let _ = child.kill();
                let _ = child.wait();
            }
            let mut value = String::new();
            if success {
                let _ = stdout.take(4096).read_to_string(&mut value);
            }
            value
        }

        fn stop(&self) {
            let children = std::mem::take(&mut *self.0.lock().expect("Owned appearance children"));
            for mut child in children.into_iter().flatten() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn theme(color_scheme: &str, gtk_theme: &str) -> Option<egui::Theme> {
        let value = |text: &str| text.trim().trim_matches('\'').to_lowercase();
        match value(color_scheme).as_str() {
            "prefer-dark" => Some(egui::Theme::Dark),
            "prefer-light" => Some(egui::Theme::Light),
            _ => {
                let gtk_theme = value(gtk_theme);
                (!gtk_theme.is_empty()).then(|| {
                    if gtk_theme.ends_with("dark") {
                        egui::Theme::Dark
                    } else {
                        egui::Theme::Light
                    }
                })
            }
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn desktop_color_scheme_takes_precedence_over_legacy_gtk_theme() {
            use eframe::egui::Theme::{Dark, Light};
            for (scheme, gtk, expected) in [
                ("'prefer-dark'\n", "'Adwaita'\n", Some(Dark)),
                ("'prefer-light'", "'Adwaita-dark'", Some(Light)),
                ("'default'", "'Yaru-dark'", Some(Dark)),
                ("'default'", "'Adwaita'", Some(Light)),
                ("", "", None),
            ] {
                assert_eq!(super::theme(scheme, gtk), expected);
            }
        }
    }
}
