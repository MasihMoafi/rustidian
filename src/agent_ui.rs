//! The vault's local conversation and explicit, reviewable note proposals.
use crate::{agent, theme, ui_probe};
use eframe::egui::{self, Key, RichText, ScrollArea, TextEdit};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::{Path, PathBuf},
};

const MAX_MESSAGES: usize = 40;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_TRANSCRIPT_BYTES: usize = 256 * 1024;
const MAX_STATE_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Copy, Serialize, Deserialize)]
enum Role {
    User,
    Assistant,
}

#[derive(Clone, Serialize, Deserialize)]
struct Message {
    role: Role,
    text: String,
}

#[derive(Default, Serialize, Deserialize)]
struct Saved {
    thread_id: Option<String>,
    messages: Vec<Message>,
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
        for message in &mut self.messages {
            truncate(&mut message.text, MAX_MESSAGE_BYTES);
        }
        while self.messages.len() > MAX_MESSAGES
            || self
                .messages
                .iter()
                .map(|message| message.text.len())
                .sum::<usize>()
                > MAX_TRANSCRIPT_BYTES
        {
            self.messages.remove(0);
        }
    }

    fn push(&mut self, role: Role, text: String) {
        self.messages.push(Message { role, text });
        self.bound();
    }
}

struct Turn {
    original: Option<agent::NoteContext>,
    propose_edit: bool,
    response: String,
    started: bool,
}

struct Review {
    proposal: agent::Proposal,
    original: agent::NoteContext,
}

#[derive(Default)]
pub struct Workspace {
    root: Option<PathBuf>,
    saved: Saved,
    runtime: agent::Agent,
    attached: Option<agent::NoteContext>,
    prompt: String,
    propose_edit: bool,
    turn: Option<Turn>,
    review: Option<Review>,
    error: Option<String>,
    stopping: bool,
    focus: bool,
}

pub enum Action {
    Attach,
    Apply {
        proposal: agent::Proposal,
        original: agent::NoteContext,
    },
}

impl Workspace {
    pub fn attach(&mut self, context: agent::NoteContext) {
        self.attached = Some(context);
        self.focus = true;
    }

    pub fn context(&self) -> Option<&agent::NoteContext> {
        self.attached.as_ref()
    }

    fn open(&mut self, root: &Path) {
        if self.root.as_deref() == Some(root) {
            return;
        }
        // Replacing the controller drops only its owned worker and connection.
        let attached = self.root.is_none().then(|| self.attached.take()).flatten();
        *self = Self {
            root: Some(root.to_owned()),
            attached,
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
                agent::Event::Completed {
                    text,
                    proposal,
                    interrupted,
                } => {
                    self.stopping = false;
                    if let Some(turn) = self.turn.take() {
                        if let Some(proposal) = proposal.filter(|_| !interrupted) {
                            if let Some(original) = turn
                                .original
                                .filter(|original| original.path == proposal.path)
                            {
                                self.saved.push(
                                    Role::Assistant,
                                    format!(
                                        "Proposed changes to {}. Review before applying.",
                                        original.path
                                    ),
                                );
                                self.review = Some(Review { proposal, original });
                            } else {
                                self.error = Some("The proposed change does not match the attached note. No changes were applied.".into());
                            }
                        } else if turn.propose_edit {
                            self.saved.push(
                                Role::Assistant,
                                if interrupted {
                                    "Proposal stopped."
                                } else {
                                    "No valid note proposal was returned."
                                }
                                .into(),
                            );
                        } else {
                            let mut response = if text.is_empty() { turn.response } else { text };
                            if interrupted {
                                response.push_str("\n\nStopped.");
                            }
                            self.saved.push(Role::Assistant, response);
                        }
                    }
                    changed = true;
                }
                agent::Event::Error(error) => {
                    self.stopping = false;
                    if let Some(turn) = self.turn.take()
                        && !turn.propose_edit
                        && !turn.response.is_empty()
                    {
                        self.saved.push(Role::Assistant, turn.response);
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

    fn send(&mut self, ctx: &egui::Context, root: &Path) {
        let request = agent::Request {
            prompt: self.prompt.clone(),
            context: self.attached.clone(),
            propose_edit: self.propose_edit,
        };
        let original = request.context.clone();
        match self.runtime.send(
            ctx.clone(),
            root.to_owned(),
            self.saved.thread_id.clone(),
            request,
        ) {
            Ok(()) => {
                self.error = None;
                self.review = None;
                self.stopping = false;
                self.turn = Some(Turn {
                    original,
                    propose_edit: self.propose_edit,
                    response: String::new(),
                    started: false,
                });
                self.saved
                    .push(Role::User, std::mem::take(&mut self.prompt));
                self.persist(root);
                self.focus = true;
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, root: &Path) -> Option<Action> {
        self.open(root);
        theme::interface_style(ui);
        let mut action = None;
        let mut clear_review = false;
        let busy = self.runtime.busy();
        ui_probe::metric(ui.ctx(), "agent-busy", f32::from(u8::from(busy)));
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
                        .find(|message| matches!(message.role, Role::Assistant))
                        .map(|message| message.text.len())
                })
                .unwrap_or(0) as f32,
        );
        ui.horizontal(|ui| {
            ui.label(RichText::new("Agent").strong().size(18.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let new = ui.add_enabled(!busy, egui::Button::new("New conversation"));
                ui_probe::record(ui.ctx(), "agent-new", new.rect);
                if new.clicked() {
                    self.saved = Saved::default();
                    self.turn = None;
                    self.review = None;
                    self.prompt.clear();
                    self.error = None;
                    self.focus = true;
                    self.persist(root);
                }
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let attach = ui.add_enabled(
                !busy,
                egui::Button::new(if self.attached.is_some() {
                    "Update note context"
                } else {
                    "Attach note"
                }),
            );
            ui_probe::record(ui.ctx(), "agent-attach", attach.rect);
            if attach.clicked() {
                action = Some(Action::Attach);
            }
            if let Some(context) = &self.attached {
                ui.label(RichText::new(&context.path).strong());
                if ui.add_enabled(!busy, egui::Button::new("Detach")).clicked() {
                    self.attached = None;
                    self.propose_edit = false;
                }
            } else {
                ui.weak("No note attached");
            }
        });
        if let Some(context) = &self.attached {
            if let Some(selection) = context
                .selection
                .as_deref()
                .filter(|selection| !selection.is_empty())
            {
                ui.label(RichText::new("Selection shared with the agent:").small());
                ui.add(
                    egui::Label::new(clipped(selection, 512))
                        .wrap()
                        .selectable(true),
                );
            }
            ui.collapsing("Attached note content", |ui| {
                ScrollArea::vertical().id_salt("agent-context").max_height(100.0).show(ui, |ui| {
                    ui.add(egui::Label::new(clipped(&context.text, 4096)).wrap().selectable(true));
                    if context.text.len() > 4096 { ui.weak("Preview shortened; the complete attached note is shared when you send."); }
                });
            });
        }
        ui.separator();
        let transcript_height = (ui.available_height() - 180.0).max(100.0);
        ScrollArea::vertical()
            .id_salt("agent-transcript")
            .max_height(transcript_height)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                if self.saved.messages.is_empty() && self.turn.is_none() {
                    ui.weak("Ask about a note, or propose a change to review.");
                }
                for message in &self.saved.messages {
                    ui.label(
                        RichText::new(match message.role {
                            Role::User => "You",
                            Role::Assistant => "Agent",
                        })
                        .strong(),
                    );
                    ui.add(egui::Label::new(&message.text).wrap().selectable(true));
                    if message.text.len() >= MAX_MESSAGE_BYTES {
                        ui.weak("This long response was shortened in local history.");
                    }
                    ui.add_space(10.0);
                }
                if let Some(turn) = &self.turn {
                    ui.label(RichText::new("Agent").strong());
                    if turn.propose_edit {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(if self.stopping {
                                "Stopping…"
                            } else if !turn.started {
                                "Connecting…"
                            } else {
                                "Preparing changes…"
                            });
                        });
                    } else {
                        if !turn.response.is_empty() {
                            ui.add(egui::Label::new(&turn.response).wrap().selectable(true));
                        }
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(if self.stopping {
                                "Stopping…"
                            } else if !turn.started {
                                "Connecting…"
                            } else {
                                "Responding…"
                            });
                        });
                    }
                }
                if let Some(review) = &self.review {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Review changes to {}", review.original.path))
                            .strong(),
                    );
                    ui.columns(2, |columns| {
                        preview(&mut columns[0], "Before", &review.original.text);
                        preview(&mut columns[1], "After", &review.proposal.text);
                    });
                    ui.horizontal(|ui| {
                        let apply = ui.add_enabled(!busy, egui::Button::new("Apply to note"));
                        let dismiss = ui.add_enabled(!busy, egui::Button::new("Dismiss"));
                        ui_probe::record(ui.ctx(), "agent-apply", apply.rect);
                        ui_probe::record(ui.ctx(), "agent-dismiss", dismiss.rect);
                        if apply.clicked() {
                            action = Some(Action::Apply {
                                proposal: review.proposal.clone(),
                                original: review.original.clone(),
                            });
                        }
                        if dismiss.clicked() {
                            action = None;
                        }
                        clear_review = apply.clicked() || dismiss.clicked();
                    });
                }
            });
        if clear_review {
            self.review = None;
        }
        if let Some(error) = &self.error {
            let label = ui.colored_label(ui.visuals().error_fg_color, error);
            ui_probe::record(ui.ctx(), "agent-error", label.rect);
        }
        ui.separator();
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!busy, |ui| {
                ui.selectable_value(&mut self.propose_edit, false, "Ask");
                let propose = ui.add_enabled(
                    self.attached.is_some(),
                    egui::Button::selectable(self.propose_edit, "Propose edit"),
                );
                ui_probe::record(ui.ctx(), "agent-propose", propose.rect);
                if propose.clicked() {
                    self.propose_edit = true;
                }
            });
        });
        let input_id = ui.make_persistent_id("agent-message");
        let shortcut = !busy
            && ui.memory(|memory| memory.has_focus(input_id))
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, Key::Enter));
        let input = ui.add_enabled(
            !busy,
            TextEdit::multiline(&mut self.prompt)
                .id(input_id)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .char_limit(16 * 1024)
                .hint_text("Ask about your notes…"),
        );
        ui_probe::record(ui.ctx(), "agent-input", input.rect);
        if self.focus && !busy {
            input.request_focus();
            self.focus = false;
        }
        let mut send = shortcut;
        ui.horizontal(|ui| {
            ui.weak("Ctrl+Enter to send");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if busy {
                    let stop = ui.add_enabled(!self.stopping, egui::Button::new("Stop"));
                    ui_probe::record(ui.ctx(), "agent-stop", stop.rect);
                    if stop.clicked() {
                        self.runtime.stop();
                        self.stopping = true;
                    }
                } else {
                    let response =
                        ui.add_enabled(!self.prompt.trim().is_empty(), egui::Button::new("Send"));
                    ui_probe::record(ui.ctx(), "agent-send", response.rect);
                    send |= response.clicked();
                }
            });
        });
        if send && !busy {
            self.send(ui.ctx(), root);
        }
        action
    }
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

fn preview(ui: &mut egui::Ui, title: &str, text: &str) {
    ui.label(RichText::new(title).strong());
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ScrollArea::vertical()
            .id_salt(("agent-review", title))
            .max_height(180.0)
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(RichText::new(clipped(text, MAX_MESSAGE_BYTES)).monospace())
                        .wrap()
                        .selectable(true),
                );
                if text.len() > MAX_MESSAGE_BYTES {
                    ui.weak("Review preview shortened for this long note.");
                }
            });
    });
}
