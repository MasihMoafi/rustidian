//! The Agent tab: a Codex conversation that works inside the vault, beside the open note.
use crate::{
    agent::{self, Access, ModelInfo, Step, StepKind},
    theme, ui_probe,
};
use eframe::egui::{
    self, Align, AtomExt, Key, KeyboardShortcut, Layout, Modifiers, RichText, ScrollArea, TextEdit,
};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};

const MAX_MESSAGES: usize = 60;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_TRANSCRIPT_BYTES: usize = 256 * 1024;
const MAX_STATE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_STEPS: usize = 64;

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
enum Role {
    User,
    Assistant,
    Activity,
    Notice,
}

#[derive(Clone, Serialize, Deserialize)]
struct Message {
    role: Role,
    text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "is_zero")]
    seconds: u64,
}

fn is_zero(value: &u64) -> bool {
    *value == 0
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    thread_id: Option<String>,
    messages: Vec<Message>,
    #[serde(default)]
    access: Access,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    models: Vec<ModelInfo>,
}

impl Saved {
    fn bound(&mut self) {
        if self
            .thread_id
            .as_ref()
            .is_some_and(|id| id.len() > 512 || id.chars().any(char::is_control))
        {
            self.thread_id = None;
        }
        self.models.truncate(64);
        for message in &mut self.messages {
            truncate(&mut message.text, MAX_MESSAGE_BYTES);
            message.steps.truncate(MAX_STEPS);
        }
        while self.messages.len() > MAX_MESSAGES
            || self
                .messages
                .iter()
                .map(|message| message.text.len() + message.steps.len() * 256)
                .sum::<usize>()
                > MAX_TRANSCRIPT_BYTES
        {
            self.messages.remove(0);
        }
    }

    fn push(&mut self, message: Message) {
        self.messages.push(message);
        self.bound();
    }
}

struct Turn {
    started_at: Instant,
    response: String,
    steps: Vec<Step>,
    started: bool,
}

#[derive(Default)]
pub struct Workspace {
    root: Option<PathBuf>,
    saved: Saved,
    runtime: agent::Agent,
    prompt: String,
    without_note: bool,
    turn: Option<Turn>,
    error: Option<String>,
    stopping: bool,
    focus: bool,
    markdown: CommonMarkCache,
}

pub enum Action {
    /// The user sent the draft; the app supplies the open note's snapshot.
    Send,
    Open(PathBuf),
}

impl Workspace {
    pub fn focus_input(&mut self) {
        self.focus = true;
    }

    /// Whether a sent message should carry the open note.
    pub fn wants_note(&self) -> bool {
        !self.without_note
    }

    fn open(&mut self, root: &Path) {
        if self.root.as_deref() == Some(root) {
            return;
        }
        // Replacing the controller drops only its owned worker and connection.
        *self = Self {
            root: Some(root.to_owned()),
            focus: true,
            ..Default::default()
        };
        let Some(path) = rustidian::paths::state_file(root, "agent", "json") else {
            self.error = Some("Cannot locate local agent conversation storage.".into());
            return;
        };
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Ok(metadata)
                if metadata.is_file()
                    && !metadata.file_type().is_symlink()
                    && metadata.len() <= MAX_STATE_BYTES => {}
            Ok(_) => {
                self.error = Some("The saved conversation is not a small ordinary file.".into());
                return;
            }
            Err(error) => {
                self.error = Some(format!("Cannot read the saved conversation: {error}"));
                return;
            }
        }
        match std::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                serde_json::from_slice::<Saved>(&bytes).map_err(|error| error.to_string())
            }) {
            Ok(mut saved) => {
                saved.bound();
                self.saved = saved;
            }
            Err(error) => self.error = Some(format!("Cannot read the saved conversation: {error}")),
        }
    }

    fn persist(&mut self, root: &Path) {
        self.saved.bound();
        let save = || -> Result<(), String> {
            let path = rustidian::paths::state_file(root, "agent", "json")
                .ok_or("Cannot locate local agent conversation storage.")?;
            let parent = path.parent().ok_or("Cannot locate conversation storage.")?;
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            let bytes = serde_json::to_vec(&self.saved).map_err(|error| error.to_string())?;
            let mut file =
                tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
            file.write_all(&bytes)
                .and_then(|()| file.as_file().sync_all())
                .map_err(|error| error.to_string())?;
            file.persist(path).map_err(|error| error.to_string())?;
            Ok(())
        };
        if let Err(error) = save() {
            self.error = Some(format!("Cannot save the conversation locally: {error}"));
        }
    }

    pub fn poll(&mut self, ctx: &egui::Context, root: &Path) {
        self.open(root);
        let mut changed = false;
        for event in self.runtime.poll() {
            match event {
                agent::Event::Connected { thread_id } => {
                    self.saved.thread_id = Some(thread_id);
                    changed = true;
                }
                agent::Event::Models(models) => {
                    if !models.is_empty() {
                        self.saved.models = models;
                        changed = true;
                    }
                }
                agent::Event::Notice(text) => {
                    self.saved.push(Message {
                        role: Role::Notice,
                        text,
                        steps: Vec::new(),
                        seconds: 0,
                    });
                    changed = true;
                }
                agent::Event::TurnStarted { turn_id } => {
                    if let Some(turn) = &mut self.turn {
                        turn.started = !turn_id.is_empty();
                    }
                }
                agent::Event::Delta(delta) => {
                    if let Some(turn) = &mut self.turn {
                        turn.response.push_str(&delta);
                        truncate(&mut turn.response, MAX_MESSAGE_BYTES);
                    }
                }
                agent::Event::Step(step) => {
                    if let Some(turn) = &mut self.turn {
                        if let Some(existing) = turn.steps.iter_mut().find(|s| s.id == step.id) {
                            *existing = step;
                        } else if turn.steps.len() < MAX_STEPS {
                            turn.steps.push(step);
                        }
                    }
                }
                agent::Event::Completed { text, interrupted } => {
                    self.stopping = false;
                    if let Some(turn) = self.turn.take() {
                        let mut response = if text.is_empty() { turn.response } else { text };
                        if interrupted {
                            response.push_str("\n\nStopped.");
                        }
                        self.finish_turn(turn.steps, response, turn.started_at);
                    }
                    changed = true;
                }
                agent::Event::Error(error) => {
                    self.stopping = false;
                    if let Some(turn) = self.turn.take()
                        && (!turn.response.is_empty() || !turn.steps.is_empty())
                    {
                        self.finish_turn(turn.steps, turn.response, turn.started_at);
                    }
                    self.error = Some(error);
                    changed = true;
                }
            }
        }
        if changed {
            self.persist(root);
            ctx.request_repaint();
        }
    }

    fn finish_turn(&mut self, steps: Vec<Step>, response: String, started_at: Instant) {
        if !steps.is_empty() {
            self.saved.push(Message {
                role: Role::Activity,
                text: String::new(),
                steps,
                seconds: 0,
            });
        }
        self.saved.push(Message {
            role: Role::Assistant,
            text: response,
            steps: Vec::new(),
            seconds: started_at.elapsed().as_secs().max(1),
        });
    }

    /// Send the draft with the note snapshot the app resolved for this message.
    pub fn send(&mut self, ctx: &egui::Context, root: &Path, context: Option<agent::NoteContext>) {
        let request = agent::Request {
            prompt: self.prompt.clone(),
            context,
            access: self.saved.access,
            model: self.saved.model.clone(),
        };
        match self.runtime.send(
            ctx.clone(),
            root.to_owned(),
            self.saved.thread_id.clone(),
            request,
        ) {
            Ok(()) => {
                self.error = None;
                self.stopping = false;
                self.turn = Some(Turn {
                    started_at: Instant::now(),
                    response: String::new(),
                    steps: Vec::new(),
                    started: false,
                });
                let prompt = std::mem::take(&mut self.prompt);
                self.saved.push(Message {
                    role: Role::User,
                    text: prompt,
                    steps: Vec::new(),
                    seconds: 0,
                });
                self.persist(root);
                self.focus = true;
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// `note` is the open note's vault-relative path, when one is open.
    pub fn show(&mut self, ui: &mut egui::Ui, root: &Path, note: Option<&Path>) -> Option<Action> {
        self.open(root);
        theme::interface_style(ui);
        let mut action = None;
        let busy = self.runtime.busy();
        ui_probe::metric(ui.ctx(), "agent-busy", f32::from(u8::from(busy)));
        ui_probe::metric(
            ui.ctx(),
            "agent-has-context",
            f32::from(u8::from(note.is_some() && !self.without_note)),
        );
        ui_probe::metric(
            ui.ctx(),
            "agent-access-full",
            f32::from(u8::from(self.saved.access == Access::Full)),
        );
        ui_probe::metric(
            ui.ctx(),
            "agent-response-length",
            self.turn
                .as_ref()
                .map(|turn| turn.response.len())
                .or_else(|| {
                    self.saved
                        .messages
                        .iter()
                        .rev()
                        .find(|message| message.role == Role::Assistant)
                        .map(|message| message.text.len())
                })
                .unwrap_or(0) as f32,
        );
        let header = ui.horizontal(|ui| {
            ui.set_min_height(28.0);
            ui.label(RichText::new("Agent").size(14.0).strong());
            ui.label(RichText::new("Codex").small().weak());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    let new = theme::icon_button(ui, theme::Icon::NewTab, "New chat");
                    ui_probe::record(ui.ctx(), "agent-new", new.rect);
                    if new.clicked() {
                        let keep = Saved {
                            access: self.saved.access,
                            model: self.saved.model.clone(),
                            models: std::mem::take(&mut self.saved.models),
                            ..Default::default()
                        };
                        self.saved = keep;
                        self.turn = None;
                        self.prompt.clear();
                        self.error = None;
                        self.focus = true;
                        self.markdown = CommonMarkCache::default();
                        self.persist(root);
                    }
                });
            });
        });
        ui_probe::record(ui.ctx(), "agent-header", header.response.rect);
        ui.add_space(4.0);

        let input_height = ui.text_style_height(&egui::TextStyle::Body) * 2.4;
        let composer_height = input_height + 92.0;
        let footer = egui::Panel::bottom("agent-composer-panel")
            .default_size(composer_height)
            .min_size(0.0)
            .resizable(false)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin {
                top: 8,
                ..Default::default()
            }))
            .show(ui, |ui| {
                self.composer(ui, busy, input_height, note, root, &mut action)
            });
        ui_probe::record(ui.ctx(), "agent-composer", footer.response.rect);

        let transcript = ScrollArea::vertical()
            .id_salt("agent-transcript")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 8.0;
                if self.saved.messages.is_empty() && self.turn.is_none() {
                    ui.add_space(28.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("A little help with your notes").strong());
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new("Ask a question or describe a change. The agent works inside this vault.")
                                .weak(),
                        );
                    });
                }
                for (index, message) in self.saved.messages.iter().enumerate() {
                    ui.push_id(("agent-message", index), |ui| match message.role {
                        Role::User => bubble(ui, &message.text),
                        Role::Notice => {
                            let notice =
                                ui.label(RichText::new(&message.text).small().weak());
                            ui_probe::record(ui.ctx(), "agent-notice", notice.rect);
                        }
                        Role::Activity => {
                            if let Some(path) = activity(ui, &message.steps, false, index) {
                                action = Some(Action::Open(path));
                            }
                        }
                        Role::Assistant => {
                            let ctx = ui.ctx().clone();
                            ui_probe::scope(&ctx, &format!("agent-message:{index}:"), || {
                                markdown(ui, &mut self.markdown, &message.text);
                            });
                            if message.text.len() >= MAX_MESSAGE_BYTES {
                                ui.label(
                                    RichText::new("Long response shortened in local history.")
                                        .small()
                                        .weak(),
                                );
                            }
                            if message.seconds > 0 {
                                let worked = ui.label(
                                    RichText::new(format!(
                                        "Worked for {}",
                                        duration(message.seconds)
                                    ))
                                    .small()
                                    .weak(),
                                );
                                ui_probe::record(ui.ctx(), "agent-worked", worked.rect);
                            }
                        }
                    });
                    ui.add_space(4.0);
                }
                if let Some(turn) = &self.turn {
                    ui.push_id("agent-live", |ui| {
                        if let Some(path) = activity(ui, &turn.steps, true, usize::MAX) {
                            action = Some(Action::Open(path));
                        }
                        if !turn.response.is_empty() {
                            markdown(ui, &mut self.markdown, &turn.response);
                        }
                    });
                }
                if let Some(error) = &self.error {
                    egui::Frame::NONE
                        .fill(ui.visuals().error_fg_color.gamma_multiply(0.07))
                        .corner_radius(8)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.label(RichText::new("Couldn’t complete the request").strong());
                            let label = ui.add(
                                egui::Label::new(
                                    RichText::new(error).color(ui.visuals().error_fg_color),
                                )
                                .wrap()
                                .selectable(true),
                            );
                            ui_probe::record(
                                ui.ctx(),
                                "agent-error",
                                label.rect.intersect(ui.clip_rect()),
                            );
                        });
                }
            });
        ui_probe::record(ui.ctx(), "agent-transcript", transcript.inner_rect);
        action
    }

    #[allow(clippy::too_many_arguments)]
    fn composer(
        &mut self,
        ui: &mut egui::Ui,
        busy: bool,
        input_height: f32,
        note: Option<&Path>,
        root: &Path,
        action: &mut Option<Action>,
    ) {
        let input_id = ui.make_persistent_id("agent-message");
        let focused = ui.memory(|memory| memory.has_focus(input_id));
        let ready = !busy && !self.prompt.trim().is_empty();
        // Enter sends and Shift+Enter breaks the line, as in the Cloud Room and Elpis composers.
        // `consume_key` matches modifiers one-way, so filter exactly: Shift+Enter stays
        // with the text edit as its line break.
        let shortcut = focused
            && ui.input_mut(|input| {
                let mut pressed = false;
                input.events.retain(|event| {
                    let send = matches!(
                        event,
                        egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. }
                            if !modifiers.shift && !modifiers.alt
                    );
                    pressed |= send;
                    !send
                });
                pressed
            });
        let mut send = shortcut && ready;
        let stroke = if focused {
            theme::interactive(ui).gamma_multiply(0.6)
        } else {
            ui.visuals().widgets.noninteractive.bg_stroke.color
        };
        let card = egui::Frame::NONE
            .fill(ui.visuals().faint_bg_color)
            .stroke(egui::Stroke::new(1.0, stroke))
            .corner_radius(12)
            .inner_margin(egui::Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                ScrollArea::vertical()
                    .id_salt("agent-draft")
                    .max_height(input_height)
                    .min_scrolled_height(input_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let input = ui.add(
                            TextEdit::multiline(&mut self.prompt)
                                .id(input_id)
                                .frame(egui::Frame::NONE)
                                .desired_rows(2)
                                .desired_width(f32::INFINITY)
                                .char_limit(16 * 1024)
                                .return_key(Some(KeyboardShortcut::new(
                                    Modifiers::SHIFT,
                                    Key::Enter,
                                )))
                                .hint_text(if busy {
                                    "Write a follow-up…"
                                } else {
                                    "Ask about your notes, or describe a change…"
                                }),
                        );
                        ui_probe::record(
                            ui.ctx(),
                            "agent-input",
                            input.rect.intersect(ui.clip_rect()),
                        );
                        if self.focus {
                            input.request_focus();
                            self.focus = false;
                        }
                    });
                ui.horizontal(|ui| {
                    // Right to left so the round button keeps its place and the chip truncates.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if busy {
                            let stop = round_button(ui, !self.stopping, RoundIcon::Stop, "Stop");
                            ui_probe::record(ui.ctx(), "agent-stop", stop.rect);
                            if stop.clicked() {
                                self.runtime.stop();
                                self.stopping = true;
                            }
                        } else {
                            let button = round_button(ui, ready, RoundIcon::Send, "Send · Enter");
                            ui_probe::record(ui.ctx(), "agent-send", button.rect);
                            send |= button.clicked();
                        }
                        let model = self
                            .saved
                            .models
                            .iter()
                            .find(|model| Some(&model.id) == self.saved.model.as_ref())
                            .map(|model| model.name.clone())
                            .unwrap_or_else(|| "Default model".into());
                        let mut changed = false;
                        let width = (ui.available_width() * 0.45).clamp(56.0, 140.0);
                        // A child of fixed width makes the selected model truncate instead of
                        // squeezing the note chip.
                        ui.allocate_ui_with_layout(
                            egui::vec2(width, 0.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.add_enabled_ui(!busy, |ui| {
                                    let combo = egui::ComboBox::from_id_salt("agent-model")
                                        .icon(theme::combo_chevron)
                                        .width(width)
                                        .truncate()
                                        .selected_text(RichText::new(model).small())
                                        .show_ui(ui, |ui| {
                                            theme::menu_style(ui);
                                            ui.set_min_width(180.0);
                                            let item = ui.selectable_value(
                                                &mut self.saved.model,
                                                None,
                                                "Default model",
                                            );
                                            ui_probe::record(
                                                ui.ctx(),
                                                "agent-model:default",
                                                item.rect,
                                            );
                                            changed |= item.changed();
                                            if self.saved.models.is_empty() {
                                                ui.label(
                                                    RichText::new(
                                                        "Models appear after the first reply",
                                                    )
                                                    .small()
                                                    .weak(),
                                                );
                                            }
                                            for option in &self.saved.models {
                                                let item = ui.selectable_value(
                                                    &mut self.saved.model,
                                                    Some(option.id.clone()),
                                                    &option.name,
                                                );
                                                ui_probe::record(
                                                    ui.ctx(),
                                                    format!("agent-model:{}", option.id),
                                                    item.rect,
                                                );
                                                changed |= item.changed();
                                            }
                                        });
                                    ui_probe::record(ui.ctx(), "agent-model", combo.response.rect);
                                });
                            },
                        );
                        if changed {
                            self.persist(root);
                        }
                        let name = note
                            .and_then(Path::file_name)
                            .map(|name| name.to_string_lossy());
                        let chip = ui.add_enabled(
                            note.is_some() && !busy,
                            egui::Button::selectable(
                                note.is_some() && !self.without_note,
                                (
                                    theme::icon_image(
                                        theme::Icon::NewNote,
                                        ui.visuals().weak_text_color(),
                                    ),
                                    RichText::new(name.as_deref().unwrap_or("No note open"))
                                        .small()
                                        .atom_shrink(true),
                                ),
                            )
                            .corner_radius(8)
                            .truncate(),
                        );
                        let chip = chip.on_hover_text(if self.without_note {
                            "Include the open note with your message"
                        } else {
                            "The open note and selection go with your message"
                        });
                        ui_probe::record(ui.ctx(), "agent-note", chip.rect);
                        if chip.clicked() {
                            self.without_note = !self.without_note;
                        }
                    });
                });
            });
        ui_probe::record(ui.ctx(), "agent-composer-card", card.response.rect);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if let Some(turn) = &self.turn {
                    ui.add(egui::Spinner::new().size(12.0));
                    let state = if self.stopping {
                        "Stopping"
                    } else if !turn.started {
                        "Connecting"
                    } else {
                        "Working"
                    };
                    let status = ui.label(
                        RichText::new(format!(
                            "{state} · {}",
                            duration(turn.started_at.elapsed().as_secs())
                        ))
                        .small()
                        .weak(),
                    );
                    ui_probe::record(ui.ctx(), "agent-status", status.rect);
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(500));
                }
                let mut changed = false;
                ui.add_enabled_ui(!busy, |ui| {
                    let combo = egui::ComboBox::from_id_salt("agent-access")
                        .icon(theme::combo_chevron)
                        .selected_text(RichText::new(self.saved.access.label()).small())
                        .show_ui(ui, |ui| {
                            theme::menu_style(ui);
                            ui.set_min_width(240.0);
                            for option in Access::ALL {
                                let item = ui.selectable_value(
                                    &mut self.saved.access,
                                    option,
                                    option.label(),
                                );
                                ui_probe::record(
                                    ui.ctx(),
                                    format!("agent-access:{}", option.label()),
                                    item.rect,
                                );
                                ui.label(RichText::new(option.description()).small().weak());
                                changed |= item.changed();
                            }
                        });
                    ui_probe::record(ui.ctx(), "agent-access", combo.response.rect);
                });
                if changed {
                    self.persist(root);
                }
                ui.add_space(6.0);
                let vault = root
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let folder =
                    ui.add(egui::Label::new(RichText::new(vault).small().weak()).truncate());
                ui_probe::record(ui.ctx(), "agent-vault", folder.rect);
                ui.add(theme::icon_image(
                    theme::Icon::Files,
                    ui.visuals().weak_text_color(),
                ));
            });
        });
        ui_probe::record(ui.ctx(), "agent-composer-clip", ui.clip_rect());
        if send {
            *action = Some(Action::Send);
        }
    }
}

fn bubble(ui: &mut egui::Ui, text: &str) {
    ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width() * 0.92, 0.0),
            Layout::top_down(Align::Min),
            |ui| {
                egui::Frame::NONE
                    .fill(theme::hover_fill(ui))
                    .corner_radius(12)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.add(egui::Label::new(text).wrap().selectable(true));
                    });
            },
        );
    });
}

/// One collapsible row per turn summarising what the agent ran and changed.
fn activity(ui: &mut egui::Ui, steps: &[Step], live: bool, index: usize) -> Option<PathBuf> {
    if steps.is_empty() {
        return None;
    }
    let mut open = None;
    let id = ui.make_persistent_id(("agent-activity", index));
    let (_, header, _) =
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, live)
            .show_header(ui, |ui| {
                // A plain CollapsingHeader never truncates; a long summary would widen the sidebar.
                ui.add(
                    egui::Label::new(RichText::new(summary(steps)).small())
                        .truncate()
                        .sense(egui::Sense::click()),
                )
            })
            .body(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                for step in steps {
                    ui.horizontal(|ui| {
                        if !step.done {
                            ui.add(egui::Spinner::new().size(10.0));
                        }
                        match &step.kind {
                            StepKind::Command { command, exit_code } => {
                                let color = if step.failed {
                                    ui.visuals().error_fg_color
                                } else {
                                    ui.visuals().weak_text_color()
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(command.lines().next().unwrap_or_default())
                                            .monospace()
                                            .size(12.0)
                                            .color(color),
                                    )
                                    .truncate(),
                                )
                                .on_hover_text(command);
                                if let Some(code) = exit_code.filter(|code| *code != 0) {
                                    ui.label(
                                        RichText::new(format!("exit {code}"))
                                            .small()
                                            .color(ui.visuals().error_fg_color),
                                    );
                                }
                            }
                            StepKind::Files { changes } => {
                                ui.vertical(|ui| {
                                    for change in changes {
                                        let name = Path::new(&change.path)
                                            .file_name()
                                            .map(|name| name.to_string_lossy().into_owned())
                                            .unwrap_or_else(|| change.path.clone());
                                        let verb = match change.kind.as_str() {
                                            "add" => "Created",
                                            "delete" => "Deleted",
                                            _ => "Edited",
                                        };
                                        let row = ui.add(
                                            egui::Button::new(
                                                RichText::new(format!("{verb} {name}"))
                                                    .size(13.0)
                                                    .color(theme::accent(ui)),
                                            )
                                            .frame_when_inactive(false)
                                            .truncate(),
                                        );
                                        ui_probe::record(
                                            ui.ctx(),
                                            format!("agent-file:{name}"),
                                            row.rect,
                                        );
                                        if row.clicked() && change.kind != "delete" {
                                            open = Some(PathBuf::from(&change.path));
                                        }
                                    }
                                });
                            }
                        }
                    });
                }
            });
    // The text toggles the row as well as the arrow.
    if header.inner.clicked()
        && let Some(mut state) = egui::collapsing_header::CollapsingState::load(ui.ctx(), id)
    {
        state.toggle(ui);
        state.store(ui.ctx());
    }
    ui_probe::record(
        ui.ctx(),
        if live {
            "agent-activity:live".to_owned()
        } else {
            format!("agent-activity:{index}")
        },
        header.response.rect,
    );
    open
}

/// "Ran 2 commands (1 failed), edited Note.md" in the Elpis desktop wording.
fn summary(steps: &[Step]) -> String {
    let mut commands = 0;
    let mut failed = 0;
    let mut files: Vec<String> = Vec::new();
    for step in steps {
        match &step.kind {
            StepKind::Command { .. } => {
                commands += 1;
                failed += usize::from(step.failed);
            }
            StepKind::Files { changes } => {
                for change in changes {
                    let name = Path::new(&change.path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| change.path.clone());
                    if !files.contains(&name) {
                        files.push(name);
                    }
                }
            }
        }
    }
    let mut parts = Vec::new();
    if commands > 0 {
        let mut text = if commands == 1 {
            "Ran a command".to_owned()
        } else {
            format!("Ran {commands} commands")
        };
        if failed > 0 {
            text.push_str(&format!(" ({failed} failed)"));
        }
        parts.push(text);
    }
    if !files.is_empty() {
        let shown = files.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
        let more = files.len().saturating_sub(3);
        let verb = if parts.is_empty() { "Edited" } else { "edited" };
        parts.push(if more > 0 {
            format!("{verb} {shown} and {more} more")
        } else {
            format!("{verb} {shown}")
        });
    }
    if parts.is_empty() {
        "Working".into()
    } else {
        parts.join(", ")
    }
}

fn duration(seconds: u64) -> String {
    if seconds >= 60 {
        format!("{}m {}s", seconds / 60, seconds % 60)
    } else {
        format!("{seconds}s")
    }
}

#[derive(Clone, Copy)]
enum RoundIcon {
    Send,
    Stop,
}

/// The composer's one round action button: Send (up arrow) or Stop (square).
fn round_button(
    ui: &mut egui::Ui,
    enabled: bool,
    icon: RoundIcon,
    tooltip: &str,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, tooltip));
        if ui.is_rect_visible(rect) {
            let (fill, mark) = if enabled {
                (
                    theme::interactive(ui).gamma_multiply(if response.hovered() {
                        1.0
                    } else {
                        0.9
                    }),
                    ui.visuals().extreme_bg_color,
                )
            } else {
                (
                    ui.visuals().widgets.inactive.bg_fill,
                    ui.visuals().weak_text_color(),
                )
            };
            let painter = ui.painter();
            let center = rect.center();
            painter.circle_filled(center, 14.0, fill);
            let stroke = egui::Stroke::new(2.0, mark);
            match icon {
                RoundIcon::Send => {
                    let top = center - egui::vec2(0.0, 5.0);
                    painter.line_segment([center + egui::vec2(0.0, 5.0), top], stroke);
                    painter.line_segment([top, top + egui::vec2(-4.5, 4.5)], stroke);
                    painter.line_segment([top, top + egui::vec2(4.5, 4.5)], stroke);
                }
                RoundIcon::Stop => {
                    painter.rect_filled(
                        egui::Rect::from_center_size(center, egui::vec2(9.0, 9.0)),
                        2.0,
                        mark,
                    );
                }
            }
        }
        response.on_hover_text(tooltip)
    })
    .inner
}

fn markdown(ui: &mut egui::Ui, cache: &mut CommonMarkCache, source: &str) {
    // Model-provided images are explicit links; displaying a reply never fetches them.
    let images: Vec<_> = Parser::new_ext(source, Options::all())
        .into_offset_iter()
        .filter_map(|(event, range)| {
            matches!(event, Event::Start(Tag::Image { .. })).then_some(range.start)
        })
        .collect();
    let text: String = source
        .char_indices()
        .filter_map(|(index, ch)| (!images.contains(&index) || ch != '!').then_some(ch))
        .collect();
    let mut previous = 0;
    let mut block: Option<(usize, String, String)> = None;
    let mut ordinal = 0;
    for (event, range) in Parser::new_ext(&text, Options::all()).into_offset_iter() {
        match event {
            Event::Start(Tag::CodeBlock(kind)) => {
                let language = match kind {
                    CodeBlockKind::Fenced(language) => language
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    CodeBlockKind::Indented => String::new(),
                };
                block = Some((range.start, language, String::new()));
            }
            Event::Text(body) => {
                if let Some((_, _, code)) = &mut block {
                    code.push_str(&body);
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((start, language, code)) = block.take() {
                    CommonMarkViewer::new().show(ui, cache, &text[previous..start]);
                    if !language.is_empty() {
                        ui.label(RichText::new(&language).small().weak());
                    }
                    crate::code_ui::reading(ui, &code, &language, ordinal);
                    ordinal += 1;
                    previous = range.end;
                }
            }
            _ => {}
        }
    }
    CommonMarkViewer::new().show(ui, cache, &text[previous..]);
}

fn truncate(text: &mut String, limit: usize) {
    let end = clipped(text, limit).len();
    text.truncate(end);
}

fn clipped(text: &str, limit: usize) -> &str {
    let mut end = text.len().min(limit);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::FileChange;

    fn command(id: &str, failed: bool) -> Step {
        Step {
            id: id.into(),
            kind: StepKind::Command {
                command: "rg -n TODO".into(),
                exit_code: Some(i32::from(failed)),
            },
            done: true,
            failed,
        }
    }

    fn files(paths: &[&str]) -> Step {
        Step {
            id: "edit".into(),
            kind: StepKind::Files {
                changes: paths
                    .iter()
                    .map(|path| FileChange {
                        path: (*path).into(),
                        kind: "update".into(),
                    })
                    .collect(),
            },
            done: true,
            failed: false,
        }
    }

    #[test]
    fn activity_summary_reads_like_the_elpis_desktop() {
        assert_eq!(summary(&[command("a", false)]), "Ran a command");
        assert_eq!(
            summary(&[command("a", false), command("b", true)]),
            "Ran 2 commands (1 failed)"
        );
        assert_eq!(
            summary(&[command("a", false), files(&["Notes/Home.md"])]),
            "Ran a command, edited Home.md"
        );
        assert_eq!(
            summary(&[files(&["a.md", "b.md", "c.md", "d.md"])]),
            "Edited a.md, b.md, c.md and 1 more"
        );
        assert_eq!(duration(75), "1m 15s");
    }

    #[test]
    fn older_saved_conversations_still_load() {
        let saved: Saved = serde_json::from_str(
            r#"{"thread_id":"t","messages":[{"role":"User","text":"hi"},{"role":"Assistant","text":"hello"}]}"#,
        )
        .unwrap();
        assert_eq!(saved.messages.len(), 2);
        assert_eq!(saved.access, Access::Full);
        assert!(saved.models.is_empty());
    }
}
