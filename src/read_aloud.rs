//! Read aloud with local Kokoro: short pieces feed one continuous PCM player. The next piece
//! is voiced while the current one plays, and Stop ends both at
//! once. The voice runs in a Python worker on the GPU (see `assets/read-aloud`), which keeps
//! the model loaded between reads and leaves after 30 idle minutes to free the GPU.
//!
//! Paths come from `RUSTIDIAN_KOKORO_PYTHON`, `RUSTIDIAN_KOKORO_MODEL` and
//! `RUSTIDIAN_KOKORO_VOICES`; the voice and speed from `RUSTIDIAN_KOKORO_VOICE` (af_heart)
//! and `RUSTIDIAN_KOKORO_SPEED`. Settings → Read aloud can choose a hosted voice instead: the
//! user's own OpenRouter key (`rustidian::voice_account`).
use base64::Engine as _;
use eframe::egui;
use rustidian::voice_account;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, mpsc};

#[path = "read_aloud/worker.rs"]
mod worker;
use std::time::{Duration, Instant};

pub const SPEEDS: [(f32, &str, &str); 7] = [
    (1.0, "Normal", "1.0×"),
    (1.25, "1.25×", "1.25×"),
    (1.3, "1.3×", "1.3×"),
    (1.4, "1.4×", "1.4×"),
    (1.5, "1.5×", "1.5×"),
    (1.75, "1.75×", "1.75×"),
    (2.0, "2.0×", "2.0×"),
];
static SPEED: AtomicU32 = AtomicU32::new(0);

pub fn set_speed(speed: Option<f32>) {
    SPEED.store(speed.map_or(0, f32::to_bits), Ordering::SeqCst);
}

fn effective_speed(speed: Option<f32>) -> f32 {
    speed
        .or_else(|| std::env::var("RUSTIDIAN_KOKORO_SPEED").ok()?.parse().ok())
        .unwrap_or(1.0)
}

pub fn speed_menu(ui: &mut egui::Ui, speed: &mut Option<f32>, id: &str) {
    let current = effective_speed(*speed);
    let label = SPEEDS
        .iter()
        .find(|(value, _, _)| *value == current)
        .map_or_else(
            || format!("{current}×"),
            |(_, _, label)| (*label).to_owned(),
        );
    let choices = |ui: &mut egui::Ui| {
        crate::theme::menu_style(ui);
        for (value, name, _) in SPEEDS {
            let response = ui.selectable_label(
                current == value,
                format!("{} {name}", if current == value { "✓" } else { "　" }),
            );
            crate::ui_probe::record(ui.ctx(), format!("{id}:{name}"), response.rect);
            if response.clicked() {
                *speed = Some(value);
                ui.close();
            }
        }
    };
    let response = if id == "read-aloud-speed" {
        let response = ui
            .push_id(id, |ui| crate::theme::compact_text_button(ui, &label))
            .inner;
        egui::Popup::menu(&response).show(choices);
        response
    } else {
        egui::ComboBox::from_id_salt(id)
            .selected_text(label)
            .show_ui(ui, choices)
            .response
    };
    crate::ui_probe::record(ui.ctx(), id, response.rect);
}

const WORKER: &str = include_str!("../assets/read-aloud/kokoro_worker.py");
/// The worker leaves after this long without a read, to give the GPU memory back.
const IDLE: Duration = Duration::from_secs(30 * 60);
/// How long a read-aloud error stays when the pointer is not on it.
const ERROR_TIME: Duration = Duration::from_secs(6);

/// What the player bar shows.
#[derive(Clone, Default)]
pub struct Status {
    pub active: bool,
    /// The sentence being heard (1-based) and how many there are.
    pub index: usize,
    pub total: usize,
    pub current: String,
    /// True while the first sentence is still being voiced (or the voice is loading).
    pub preparing: bool,
    pub error: Option<String>,
    /// "cuda" or "cpu", once the worker is up.
    pub device: Option<String>,
}

struct Shared {
    session: AtomicU64,
    status: Mutex<Status>,
}

static SHARED: LazyLock<Shared> = LazyLock::new(|| Shared {
    session: AtomicU64::new(0),
    status: Mutex::new(Status::default()),
});
static SERVICE: OnceLock<Result<worker::Service, String>> = OnceLock::new();

fn service() -> Result<&'static worker::Service, String> {
    SERVICE
        .get_or_init(|| {
            worker::Service::start(
                worker_command,
                worker::Policy {
                    startup: Duration::from_secs(60),
                    reply: Duration::from_secs(60),
                    idle: IDLE,
                },
            )
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from)
}

fn configured(variable: &str, default: impl FnOnce() -> PathBuf) -> PathBuf {
    std::env::var_os(variable).map_or_else(default, PathBuf::from)
}

/// The Python that runs the voice: Rustidian's GPU environment, else Elpis's (CPU) one.
fn python() -> PathBuf {
    configured("RUSTIDIAN_KOKORO_PYTHON", || {
        let own = home().join(".local/share/rustidian/kokoro-venv/bin/python");
        if own.exists() {
            own
        } else {
            home().join(".local/share/elpis/kokoro-venv/bin/python")
        }
    })
}

fn model() -> PathBuf {
    configured("RUSTIDIAN_KOKORO_MODEL", || {
        home().join(".cache/hyperframes/tts/models/kokoro-v1.0.onnx")
    })
}

fn voices() -> PathBuf {
    configured("RUSTIDIAN_KOKORO_VOICES", || {
        home().join(".cache/hyperframes/tts/voices/voices-v1.0.bin")
    })
}

fn scratch() -> PathBuf {
    std::env::temp_dir().join(format!("rustidian-read-aloud-{}", std::process::id()))
}

/// Why read aloud cannot run here, if it cannot.
pub fn unavailable() -> Option<String> {
    let missing: Vec<String> = [python(), model(), voices()]
        .into_iter()
        .filter(|path| !path.exists())
        .map(|path| path.display().to_string())
        .collect();
    (!missing.is_empty())
        .then(|| format!("Read aloud needs Kokoro: missing {}", missing.join(", ")))
}

fn worker_command() -> Result<Command, String> {
    let folder = scratch();
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let script = folder.join("kokoro_worker.py");
    std::fs::write(&script, WORKER).map_err(|error| error.to_string())?;
    let mut command = Command::new(python());
    command
        .arg("-u")
        .arg(script)
        .arg(model())
        .arg(voices())
        .env_remove("RUSTIDIAN_KOKORO_LIBS")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    Ok(command)
}

/// Only the producer waits; cancellation and exit never wait on this receiver.
fn synthesize(text: &str, speed: Option<f32>, session: u64, epoch: u64) -> Result<Vec<u8>, String> {
    if !current(session) {
        return Err("Reading cancelled.".into());
    }
    let service = service()?;
    let mut request = serde_json::json!({"text": text});
    if let Some(speed) = speed {
        request["speed"] = speed.into();
    }
    let message = service
        .submit(epoch, Some(request))
        .recv()
        .map_err(|_| "Reading cancelled.")??;
    if let Some(device) = message["device"].as_str() {
        SHARED
            .status
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .device = Some(device.into());
    }
    if message["ok"] != true {
        return Err(message["error"]
            .as_str()
            .unwrap_or("The voice could not say this.")
            .into());
    }
    if message["rate"].as_u64() != Some(24_000) {
        return Err("The voice returned an unsupported sample rate.".into());
    }
    base64::engine::general_purpose::STANDARD
        .decode(message["pcm"].as_str().unwrap_or(""))
        .map_err(|error| error.to_string())
}

fn current(session: u64) -> bool {
    SHARED.session.load(Ordering::SeqCst) == session
}

fn update(ctx: &egui::Context, session: u64, change: impl FnOnce(&mut Status)) {
    if current(session) {
        change(&mut SHARED.status.lock().unwrap_or_else(|e| e.into_inner()));
        ctx.request_repaint();
    }
}

/// Start the model in the background when the pointer reaches a speaker, so the click that
/// follows does not wait for it; a note that is only read never loads it (about 1.3 GB).
pub fn warm_up() {
    if unavailable().is_none()
        && let Ok(service) = service()
    {
        let _ = service.submit(service.epoch(), None);
    }
}

/// Read a selection through one continuous mono PCM player.
pub fn speak(ctx: &egui::Context, chunks: Vec<String>) {
    let session = SHARED.session.fetch_add(1, Ordering::SeqCst) + 1;
    if status().active
        && let Some(Ok(service)) = SERVICE.get()
    {
        service.cancel();
    }
    let epoch = service().map_or(0, worker::Service::epoch);
    ctx.data_mut(|data| {
        data.remove::<(String, Instant)>(egui::Id::unique("read-aloud-error-seen"));
    });
    if chunks.is_empty() {
        return;
    }
    let settings = voice_account::state_base()
        .map(|base| voice_account::Settings::load(&base))
        .unwrap_or_default();
    set_speed(settings.speed);
    let choice = voice_account::choose_engine(
        settings.engine,
        unavailable().is_none(),
        settings.selected_account().is_some(),
    );
    if matches!(
        choice,
        voice_account::VoiceChoice::Setup | voice_account::VoiceChoice::OfferHosted
    ) {
        let mut status = SHARED.status.lock().unwrap_or_else(|e| e.into_inner());
        status.active = false;
        status.preparing = false;
        status.error = None;
        drop(status);
        ctx.data_mut(|data| match choice {
            voice_account::VoiceChoice::Setup => {
                data.insert_temp(egui::Id::unique("voice-setup"), true);
            }
            voice_account::VoiceChoice::OfferHosted => {
                data.insert_temp(egui::Id::unique("voice-fallback-speech"), chunks);
            }
            _ => unreachable!(),
        });
        ctx.request_repaint();
        return;
    }
    let hosted = choice == voice_account::VoiceChoice::Hosted;
    {
        let mut status = SHARED.status.lock().unwrap_or_else(|e| e.into_inner());
        let device = status.device.take();
        *status = Status {
            active: true,
            index: 1,
            total: chunks.len(),
            current: chunks[0].clone(),
            preparing: true,
            error: None,
            device,
        };
    }
    ctx.request_repaint();
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let result = (|| -> Result<(), String> {
            let route = if hosted {
                Some(settings.route(voice_account::Stores::app())?)
            } else {
                None
            };
            let chosen = std::env::var("RUSTIDIAN_READ_ALOUD_PLAYER").ok();
            let programs: Vec<&str> = chosen
                .as_deref()
                .map_or_else(|| vec!["pw-play", "paplay", "aplay"], |p| vec![p]);
            let mut player = programs
                .iter()
                .find_map(|program| {
                    let mut command = Command::new(program);
                    if chosen.is_some() {
                        command.args([
                            "--raw",
                            "--rate",
                            "24000",
                            "--channels",
                            "1",
                            "--format",
                            "s16le",
                        ]);
                    } else if *program == "pw-play" {
                        command.args([
                            "--raw",
                            "--rate",
                            "24000",
                            "--channels",
                            "1",
                            "--format",
                            "s16",
                        ]);
                        command.arg("-");
                    } else if *program == "paplay" {
                        command.args(["--raw", "--rate=24000", "--channels=1", "--format=s16le"]);
                    } else {
                        command.args(["-t", "raw", "-r", "24000", "-c", "1", "-f", "S16_LE"]);
                    }
                    command
                        .stdin(Stdio::piped())
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn()
                        .ok()
                })
                .ok_or("No audio player (pw-play, paplay or aplay) is installed.")?;
            let mut input = player.stdin.take().ok_or("The player has no input")?;
            let player = Arc::new(Mutex::new(player));
            let watcher = Arc::clone(&player);
            let done = Arc::new(AtomicBool::new(false));
            let watching = Arc::clone(&done);
            let monitor = std::thread::spawn(move || {
                while !watching.load(Ordering::SeqCst) {
                    if !current(session) {
                        let _ = watcher.lock().unwrap_or_else(|e| e.into_inner()).kill();
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            });
            let (sender, receiver) = mpsc::sync_channel(1);
            let producer = std::thread::spawn(move || {
                for (index, text) in chunks.iter().enumerate() {
                    let mut pieces = vec![text.as_str()];
                    if index == 0
                        && text.chars().count() > 80
                        && let Some(comma) = text.find(',')
                        && comma > 0
                        && comma + 1 < text.len()
                    {
                        pieces = vec![&text[..=comma], text[comma + 1..].trim_start()];
                    }
                    for piece in pieces {
                        if !current(session) {
                            return;
                        }
                        let bits = SPEED.load(Ordering::SeqCst);
                        let speed = (bits != 0).then(|| f32::from_bits(bits));
                        let pcm: Result<Vec<u8>, String> = (|| match &route {
                            Some(route) => {
                                let wav =
                                    voice_account::speak(route, piece, effective_speed(speed))?;
                                Ok(wav.get(44..).ok_or("Invalid hosted audio")?.to_vec())
                            }
                            None => synthesize(piece, speed, session, epoch),
                        })();
                        if !current(session) {
                            return;
                        }
                        let failed = pcm.is_err();
                        if sender.send((index, text.clone(), pcm)).is_err() || failed {
                            return;
                        }
                    }
                }
            });
            let played = (|| -> Result<(), String> {
                for (index, text, pcm) in receiver {
                    if !current(session) {
                        break;
                    }
                    let pcm = pcm?;
                    update(&ctx, session, |status| {
                        status.index = index + 1;
                        status.current = text;
                        status.preparing = false;
                    });
                    input
                        .write_all(&pcm)
                        .map_err(|error| format!("Audio player stopped: {error}"))?;
                }
                Ok(())
            })();
            drop(input);
            let _ = producer.join();
            // Poll instead of waiting under the mutex, so Stop can kill a draining player.
            let exit = loop {
                let result = player.lock().unwrap_or_else(|e| e.into_inner()).try_wait();
                match result {
                    Ok(Some(exit)) => break Ok(exit),
                    Ok(None) => std::thread::sleep(Duration::from_millis(20)),
                    Err(error) => break Err(error),
                }
            };
            done.store(true, Ordering::SeqCst);
            let _ = monitor.join();
            played?;
            if current(session) && !exit.map_err(|e| e.to_string())?.success() {
                return Err("The audio player failed.".into());
            }
            Ok(())
        })();
        update(&ctx, session, |status| {
            if let Err(error) = result {
                status.error = Some(error);
            }
            status.active = false;
            status.preparing = false;
        });
    });
}

/// Stop reading at once: the sentence playing and the ones being voiced.
pub fn stop(ctx: &egui::Context) {
    SHARED.session.fetch_add(1, Ordering::SeqCst);
    if let Some(Ok(service)) = SERVICE.get() {
        service.cancel();
    }
    let mut status = SHARED.status.lock().unwrap_or_else(|e| e.into_inner());
    status.active = false;
    status.preparing = false;
    ctx.request_repaint();
}

pub fn status() -> Status {
    SHARED
        .status
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

/// The exit callback only requests cancellation. Reaping stays outside the UI.
pub fn shutdown() {
    SHARED.session.fetch_add(1, Ordering::SeqCst);
    if let Some(Ok(service)) = SERVICE.get() {
        service.shutdown();
    }
}

pub fn wait_for_closing() {
    if let Some(Ok(service)) = SERVICE.get() {
        let _ = service.wait_shutdown(Duration::from_secs(2));
    }
    let _ = std::fs::remove_dir_all(scratch());
}

/// The text a settled label selection reported last (egui sends it once a drag ends).
static SELECTION: Mutex<String> = Mutex::new(String::new());

/// Watches egui's output for a settled text selection, so read aloud can take it without
/// touching the clipboard.
pub struct SelectionWatch;

impl egui::plugin::Plugin for SelectionWatch {
    fn debug_name(&self) -> &'static str {
        "rustidian-read-aloud-selection"
    }

    fn on_exit(&mut self, _ctx: &egui::Context) {
        shutdown();
    }

    fn output_hook(&mut self, _ctx: &egui::Context, output: &mut egui::FullOutput) {
        for command in &output.platform_output.commands {
            if let egui::OutputCommand::TextSelectionSettled(text) = command {
                text.clone_into(&mut SELECTION.lock().unwrap_or_else(|e| e.into_inner()));
            }
        }
    }
}

/// The selected text, if a label selection is still there.
pub fn selection(ctx: &egui::Context) -> Option<String> {
    let selected = ctx
        .plugin_opt::<egui::text_selection::LabelSelectionState>()
        .is_some_and(|plugin| plugin.lock().has_selection());
    let text = SELECTION.lock().unwrap_or_else(|e| e.into_inner()).clone();
    (selected && !text.trim().is_empty()).then_some(text)
}

/// A speaker: a small box, a cone and two sound waves.
pub fn paint_speaker(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let c = rect.center();
    let s = rect.height().min(rect.width()) / 16.0;
    let body = vec![
        c + egui::vec2(-6.0, -2.5) * s,
        c + egui::vec2(-3.0, -2.5) * s,
        c + egui::vec2(1.0, -6.0) * s,
        c + egui::vec2(1.0, 6.0) * s,
        c + egui::vec2(-3.0, 2.5) * s,
        c + egui::vec2(-6.0, 2.5) * s,
    ];
    painter.add(egui::Shape::convex_polygon(body, color, egui::Stroke::NONE));
    for radius in [4.0, 7.0] {
        let points: Vec<egui::Pos2> = (0..=8)
            .map(|step| {
                let angle = -0.8 + 1.6 * step as f32 / 8.0;
                c + egui::vec2(1.0 + radius * angle.cos(), radius * angle.sin()) * s
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            egui::Stroke::new(1.4 * s.max(0.8), color),
        ));
    }
}

/// A press belongs to the hit-tested speaker from the previous pass, not a newly shown one.
fn speaker_pressed(ui: &egui::Ui, response: &egui::Response, rect: egui::Rect) -> bool {
    ui.is_enabled()
        && ui.is_visible()
        && response.enabled()
        && response.visible()
        && (response.is_pointer_button_down_on() || response.clicked())
        && ui.input(|input| {
            input.pointer.primary_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| rect.contains(pos) && ui.clip_rect().contains(pos))
        })
}

/// Take a selection saved when its speaker was drawn, only if that speaker received the press.
pub fn selection_speaker_pressed(
    ui: &egui::Ui,
    shown: egui::Id,
    speaker: egui::Id,
) -> Option<String> {
    let (rect, text) = ui
        .ctx()
        .data(|data| data.get_temp::<(egui::Rect, String)>(shown))?;
    let response = ui.ctx().read_response(speaker)?;
    if !speaker_pressed(ui, &response, rect) {
        return None;
    }
    ui.ctx()
        .data_mut(|data| data.remove::<(egui::Rect, String)>(shown));
    Some(text)
}

/// A round speaker that acts on press, since the press itself clears the text selection.
pub fn speaker_button(ui: &mut egui::Ui, rect: egui::Rect, id: egui::Id, hint: &str) -> bool {
    let response = ui.interact(rect, id, egui::Sense::click());
    let hovered = response.hovered();
    if hovered {
        warm_up();
    }
    let fill = if hovered {
        crate::theme::interactive(ui)
    } else {
        ui.visuals().window_fill()
    };
    let painter = ui.painter();
    painter.circle_filled(rect.center(), rect.width() / 2.0, fill);
    painter.circle_stroke(
        rect.center(),
        rect.width() / 2.0,
        ui.visuals().window_stroke(),
    );
    let ink = if hovered {
        ui.visuals().window_fill()
    } else {
        crate::theme::interactive(ui)
    };
    paint_speaker(painter, rect.shrink(rect.width() * 0.22), ink);
    let pressed = speaker_pressed(ui, &response, rect);
    response.on_hover_text(hint);
    pressed
}

/// Height reserved under the document while the player shows, so it never covers note text.
pub const PLAYER_STRIP: f32 = 52.0;

/// Whether the player shows this frame (a reading or its error).
pub fn player_visible() -> bool {
    let status = status();
    status.active || status.error.is_some()
}

/// The player: Stop, which sentence is playing, and its words, in the strip under `area`.
pub fn player_bar(ctx: &egui::Context, area: egui::Rect, speed: &mut Option<f32>) -> bool {
    let before = *speed;
    let status = status();
    if !status.active && status.error.is_none() {
        return false;
    }
    let width = area.width().clamp(
        crate::theme::space::PLAYER_MIN_WIDTH,
        crate::theme::space::PLAYER_MAX_WIDTH,
    );
    let position = egui::pos2(area.center().x - width / 2.0, area.bottom() + 6.0);
    let shown = egui::Area::new(egui::Id::unique("read-aloud-player"))
        .order(egui::Order::Foreground)
        .fixed_pos(position)
        .show(ctx, |ui| {
            let frame = crate::theme::notice_frame(ctx).inner_margin(crate::theme::space::margin(
                crate::theme::space::M,
                crate::theme::space::XS,
            ));
            let content_width = width - frame.total_margin().sum().x;
            let response = frame.show(ui, |ui| {
                ui.set_width(content_width);
                ui.horizontal(|ui| {
                    if status.active {
                        let (rect, stop) = ui.allocate_exact_size(
                            egui::Vec2::splat(crate::theme::space::ROW),
                            egui::Sense::click(),
                        );
                        let fill = if stop.hovered() {
                            crate::theme::interactive(ui)
                        } else {
                            crate::theme::interactive(ui).gamma_multiply(0.8)
                        };
                        ui.painter().circle_filled(
                            rect.center(),
                            crate::theme::space::ROW / 2.0,
                            fill,
                        );
                        ui.painter().rect_filled(
                            egui::Rect::from_center_size(rect.center(), egui::vec2(10.0, 10.0)),
                            2,
                            ui.visuals().window_fill(),
                        );
                        crate::ui_probe::record(ui.ctx(), "read-aloud-stop", rect);
                        if stop.on_hover_text("Stop reading").clicked() {
                            self::stop(ui.ctx());
                        }
                        speed_menu(ui, speed, "read-aloud-speed");
                        let label = if status.preparing {
                            format!("Preparing · {} sentences", status.total)
                        } else {
                            format!("{} of {}", status.index, status.total)
                        };
                        ui.label(egui::RichText::new(label).small().weak());
                        ui.add(
                            egui::Label::new(egui::RichText::new(&status.current).small())
                                .truncate(),
                        );
                    } else if let Some(error) = &status.error {
                        ui.label(
                            egui::RichText::new(error)
                                .small()
                                .color(ui.visuals().error_fg_color),
                        );
                        let close = ui.small_button("Close");
                        crate::ui_probe::record(ctx, "read-aloud-error-close", close.rect);
                        if close.clicked() {
                            SHARED
                                .status
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .error = None;
                            ctx.data_mut(|data| {
                                data.remove::<(String, Instant)>(egui::Id::unique(
                                    "read-aloud-error-seen",
                                ));
                            });
                        }
                    }
                });
            });
            crate::ui_probe::record(ctx, "read-aloud-player", response.response.rect);
            crate::ui_probe::metric(ctx, "read-aloud-index", status.index as f32);
            response.response.contains_pointer()
        });
    // An error fades like the app's notices: 6 s after it shows, kept while hovered.
    if !status.active
        && let Some(error) = status.error
    {
        let id = egui::Id::unique("read-aloud-error-seen");
        let now = Instant::now();
        let seen = ctx.data(|data| data.get_temp::<(String, Instant)>(id));
        let since = match seen {
            Some((shown_error, at)) if shown_error == error && !shown.inner => at,
            _ => {
                ctx.data_mut(|data| data.insert_temp(id, (error, now)));
                now
            }
        };
        let left = ERROR_TIME.saturating_sub(now.duration_since(since));
        if left.is_zero() {
            SHARED
                .status
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .error = None;
            ctx.data_mut(|data| data.remove::<(String, Instant)>(id));
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(left.min(Duration::from_millis(250)));
        }
    }
    if *speed != before {
        set_speed(*speed);
    }
    *speed != before
}
