//! Settings → Read aloud: choose the local voice or the user's own OpenRouter account,
//! and add, select, replace or disconnect OpenRouter keys (`rustidian::voice_account`).
use super::*;
use rustidian::voice_account::{DictationBackend, Engine, SaveError, Settings, Stores};

/// What the Read aloud page is typing or saying.
#[derive(Default)]
pub(crate) struct VoiceForm {
    pub label: String,
    /// The key being typed; shown masked and cleared once saved.
    pub key: String,
    /// The account whose key is being replaced.
    pub replacing: Option<String>,
    message: Option<Feedback>,
    /// The keyring failed: offer to keep the typed key for this session only.
    pub offer_session: bool,
    pub setup: bool,
    pub focus_key: bool,
    models: Vec<String>,
    models_endpoint: Option<String>,
    model_error: Option<Feedback>,
    model_request: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
    key_test: Option<std::sync::mpsc::Receiver<Result<(), String>>>,
}

struct Feedback {
    text: String,
}

impl Feedback {
    fn new(text: String) -> Self {
        Self { text }
    }
}

impl VoiceForm {
    fn message(&mut self, text: impl Into<String>) {
        self.message = Some(Feedback::new(text.into()));
    }
}

fn feedback_row(ui: &mut egui::Ui, feedback: &mut Option<Feedback>) {
    if let Some(feedback) = feedback.take() {
        crate::notices::report(ui.ctx(), feedback.text);
    }
}

impl NotesApp {
    pub(super) fn voice_settings(&mut self, ui: &mut egui::Ui) {
        let before = self.voice.clone();
        settings_ui::voice_row(
            ui,
            "Dictation",
            "Choose WhisperType or an OpenRouter transcription model.",
            |ui| {
                let mut backend = self.voice.dictation_backend;
                let label = |backend| match backend {
                    DictationBackend::Codex => "Codex / ChatGPT",
                    DictationBackend::Whisper => "Local Whisper",
                    DictationBackend::Parakeet => "Local Parakeet",
                    DictationBackend::OpenRouter => "OpenRouter",
                };
                let response = egui::ComboBox::from_id_salt("settings-dictation-backend")
                    .icon(theme::combo_chevron)
                    .selected_text(label(backend))
                    .show_ui(ui, |ui| {
                        theme::menu_style(ui);
                        for value in [
                            DictationBackend::Codex,
                            DictationBackend::Whisper,
                            DictationBackend::Parakeet,
                            DictationBackend::OpenRouter,
                        ] {
                            let option = ui.selectable_value(&mut backend, value, label(value));
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("settings-dictation-backend:{}", label(value)),
                                option.rect,
                            );
                        }
                    });
                crate::ui_probe::record(
                    ui.ctx(),
                    "settings-dictation-backend",
                    response.response.rect,
                );
                self.voice.dictation_backend = backend;
                false
            },
        );
        let hosted_dictation = self.voice.dictation_backend == DictationBackend::OpenRouter;
        if hosted_dictation || self.voice_form.setup {
            ui.separator();
            self.transcription_model_picker(ui);
        }
        ui.separator();
        settings_ui::voice_row(
            ui,
            "Voice",
            "Kokoro or Qwen on this computer, or a voice through OpenRouter.",
            |ui| {
                let mut engine = self.voice.engine;
                let response = egui::ComboBox::from_id_salt("settings-voice-engine")
                    .icon(theme::combo_chevron)
                    .selected_text(match engine {
                        Engine::Local => "Local (Kokoro)",
                        Engine::Qwen => "Local (Qwen)",
                        Engine::OpenRouter => "OpenRouter",
                    })
                    .show_ui(ui, |ui| {
                        theme::menu_style(ui);
                        for (value, label) in [
                            (Engine::Local, "Local (Kokoro)"),
                            (Engine::Qwen, "Local (Qwen)"),
                            (Engine::OpenRouter, "OpenRouter"),
                        ] {
                            let r = ui.selectable_value(&mut engine, value, label);
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("settings-voice-engine:{label}"),
                                r.rect,
                            );
                        }
                    });
                crate::ui_probe::record(ui.ctx(), "settings-voice-engine", response.response.rect);
                self.voice.engine = engine;
                false
            },
        );
        ui.separator();
        settings_ui::voice_row(ui, "Speed", "Applies to the next piece of speech.", |ui| {
            crate::read_aloud::speed_menu(ui, &mut self.voice.speed, "settings-voice-speed");
            false
        });
        if self.voice.speed != before.speed {
            crate::read_aloud::set_speed(self.voice.speed);
        }
        if self.voice.engine == Engine::OpenRouter {
            ui.separator();
            self.voice_text(
                ui,
                "Model",
                "An OpenRouter speech model.",
                "settings-voice-model",
                |v| &mut v.model,
            );
            ui.separator();
            self.voice_text(
                ui,
                "Voice name",
                "A voice the model offers.",
                "settings-voice-name",
                |v| &mut v.voice,
            );
        }
        if self.voice.engine == Engine::OpenRouter
            || hosted_dictation
            || self.voice.selected_account().is_some()
            || !self.voice.pending_secret_removal.is_empty()
            || self.voice_form.setup
        {
            ui.separator();
            let previous_endpoint = self.voice.endpoint.clone();
            self.voice_text(
                ui,
                "Endpoint",
                "Keys are sent only to the endpoint they were saved for.",
                "settings-voice-endpoint",
                |v| &mut v.endpoint,
            );
            if self.voice.endpoint != previous_endpoint {
                self.voice_form.key_test = None;
                self.voice_form.message = None;
            }
            ui.separator();
            self.save_voice_changes(&before);
            self.voice_accounts(ui);
        } else {
            self.save_voice_changes(&before);
        }
        feedback_row(ui, &mut self.voice_form.message);
    }

    fn transcription_model_picker(&mut self, ui: &mut egui::Ui) {
        if self.voice_form.models_endpoint.as_deref() != Some(self.voice.endpoint.as_str()) {
            self.voice_form.models.clear();
            self.voice_form.model_error = None;
            self.voice_form.model_request = None;
            self.voice_form.models_endpoint = Some(self.voice.endpoint.clone());
        }
        if let Some(request) = &self.voice_form.model_request {
            match request.try_recv() {
                Ok(Ok(models)) => {
                    self.voice_form.models = models;
                    self.voice_form.model_error = None;
                    self.voice_form.model_request = None;
                }
                Ok(Err(error)) => {
                    self.voice_form.model_error = Some(Feedback::new(error));
                    self.voice_form.model_request = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.voice_form.model_error = Some(Feedback::new(
                        "The model list could not be loaded. Open it to try again.".into(),
                    ));
                    self.voice_form.model_request = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let mut fetch = false;
        settings_ui::voice_row(
            ui,
            "Transcription model",
            "Choose from OpenRouter's transcription models.",
            |ui| {
                let response = egui::ComboBox::from_id_salt("settings-dictation-model")
                    .icon(theme::combo_chevron)
                    .selected_text(&self.voice.transcription_model)
                    .show_ui(ui, |ui| {
                        theme::menu_style(ui);
                        if self.voice_form.models.is_empty() {
                            ui.label(if self.voice_form.model_request.is_some() {
                                "Loading models…"
                            } else {
                                "Open to load models"
                            });
                        }
                        for model in &self.voice_form.models {
                            let option = ui.selectable_value(
                                &mut self.voice.transcription_model,
                                model.clone(),
                                model,
                            );
                            crate::ui_probe::record(
                                ui.ctx(),
                                format!("settings-dictation-model:{model}"),
                                option.rect,
                            );
                        }
                    });
                crate::ui_probe::record(
                    ui.ctx(),
                    "settings-dictation-model",
                    response.response.rect,
                );
                if response.response.clicked()
                    && self.voice_form.models.is_empty()
                    && self.voice_form.model_request.is_none()
                {
                    fetch = true;
                }
                false
            },
        );
        feedback_row(ui, &mut self.voice_form.model_error);
        if fetch {
            let endpoint = self.voice.endpoint.clone();
            let ctx = ui.ctx().clone();
            let (sender, receiver) = std::sync::mpsc::channel();
            self.voice_form.model_error = None;
            self.voice_form.model_request = Some(receiver);
            std::thread::spawn(move || {
                let _ = sender.send(rustidian::voice_account::transcription_models(&endpoint));
                ctx.request_repaint();
            });
        }
    }

    fn voice_text(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        description: &str,
        id: &str,
        field: impl FnOnce(&mut Settings) -> &mut String,
    ) {
        settings_ui::voice_row(ui, title, description, |ui| {
            let response = ui.add(
                theme::text_field(TextEdit::singleline(field(&mut self.voice)).id_salt(id))
                    .desired_width(ui.available_width()),
            );
            crate::ui_probe::record(ui.ctx(), id, response.rect);
            false
        });
    }

    fn voice_accounts(&mut self, ui: &mut egui::Ui) {
        let stores = Stores::app();
        if !self.voice.pending_secret_removal.is_empty() {
            settings_ui::voice_row(
                ui,
                "Key removal pending",
                "An old or unused key could not be removed. Unlock the keyring and retry.",
                |ui| {
                    let retry = theme::button(ui, "Retry key removal", theme::Kind::Outline);
                    crate::ui_probe::record(ui.ctx(), "settings-voice-retry-removal", retry.rect);
                    if retry.clicked() {
                        let result = rustidian::voice_account::state_base()
                            .ok_or_else(|| "No voice settings folder is available.".to_string())
                            .and_then(|base| self.voice.cleanup_pending(&base, stores));
                        let message = match result {
                            Ok(()) => "Old keys removed.".to_string(),
                            Err(error) => {
                                self.reload_voice_on_conflict(&error);
                                format!("Key removal is still pending: {error}")
                            }
                        };
                        self.voice_form.message(message);
                    }
                    false
                },
            );
            ui.separator();
        }
        let tested =
            self.voice_form
                .key_test
                .as_ref()
                .and_then(|request| match request.try_recv() {
                    Ok(result) => Some(result),
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        Some(Err("The key test was interrupted. Try again.".into()))
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => None,
                });
        if let Some(result) = tested {
            self.voice_form.key_test = None;
            self.voice_form.message(match result {
                Ok(()) => "Key accepted. Save it to use this account.".to_string(),
                Err(error) => error,
            });
        }
        let accounts = self.voice.accounts.clone();
        let status = match self.voice.selected_account() {
            Some(account) if account.endpoint == self.voice.endpoint => {
                format!(
                    "Selected \"{}\" (key …{}). Saving a key does not verify it.",
                    account.label, account.ending
                )
            }
            Some(account) => format!(
                "\"{}\" was saved for {}; add a key for this endpoint.",
                account.label, account.endpoint
            ),
            None => "Not connected. Add an OpenRouter key below.".into(),
        };
        settings_ui::voice_row(ui, "OpenRouter account", &status, |_| false);
        for account in &accounts {
            ui.separator();
            let selected = self.voice.selected.as_deref() == Some(account.id.as_str());
            let mut detail = format!("Key …{}", account.ending);
            if account.session_only {
                detail.push_str(" · this session only");
            }
            if account.endpoint != self.voice.endpoint {
                detail.push_str(&format!(" · for {}", account.endpoint));
            }
            let title = if selected {
                format!("{} (in use)", account.label)
            } else {
                account.label.clone()
            };
            settings_ui::voice_row(ui, &title, &detail, |ui| {
                let id = &account.id;
                let disconnect = theme::button(ui, "Disconnect", theme::Kind::Destructive);
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("settings-voice-disconnect:{}", account.label),
                    disconnect.rect,
                );
                if disconnect.clicked() {
                    let result = rustidian::voice_account::state_base()
                        .ok_or_else(|| "No voice settings folder is available; the account and key are unchanged.".to_string())
                        .and_then(|base| self.voice.disconnect(&base, stores, id));
                    match result {
                        Ok(()) if self.voice.pending_secret_removal.is_empty() => self
                            .voice_form
                            .message(format!("Disconnected \"{}\".", account.label)),
                        Ok(()) => self.voice_form.message(format!(
                            "Disconnected \"{}\"; key removal is pending. Retry above.",
                            account.label
                        )),
                        Err(error) => {
                            self.reload_voice_on_conflict(&error);
                            self.voice_form.message(error);
                        }
                    }
                }
                let replace = theme::button(ui, "Replace key", theme::Kind::Outline);
                crate::ui_probe::record(
                    ui.ctx(),
                    format!("settings-voice-replace:{}", account.label),
                    replace.rect,
                );
                if replace.clicked() {
                    self.voice_form.replacing = Some(id.clone());
                    self.voice_form.label = account.label.clone();
                    self.voice_form.key.clear();
                    self.voice_form.offer_session = false;
                    self.voice_form.key_test = None;
                    self.voice_form.message = None;
                }
                if !selected {
                    let select = theme::button(ui, "Use", theme::Kind::Outline);
                    crate::ui_probe::record(
                        ui.ctx(),
                        format!("settings-voice-use:{}", account.label),
                        select.rect,
                    );
                    if select.clicked() {
                        self.voice.selected = Some(id.clone());
                    }
                }
                false
            });
        }
        ui.separator();
        let replacing = self.voice_form.replacing.clone();
        let heading = if replacing.is_some() {
            "Replace key"
        } else {
            "Add a key"
        };
        settings_ui::voice_row(ui, heading, "A name for the key (optional).", |ui| {
            let response = ui.add(
                theme::text_field(
                    TextEdit::singleline(&mut self.voice_form.label)
                        .id_salt("settings-voice-label")
                        .hint_text("Personal"),
                )
                .desired_width(ui.available_width()),
            );
            crate::ui_probe::record(ui.ctx(), "settings-voice-label", response.rect);
            false
        });
        settings_ui::voice_row(
            ui,
            "API key",
            if self.voice_form.setup {
                "Add your OpenRouter key, or install the local voice."
            } else {
                "Kept in the system keyring, never in a file. Create one at openrouter.ai/keys."
            },
            |ui| {
                let response = ui.add(
                    theme::text_field(
                        TextEdit::singleline(&mut self.voice_form.key)
                            .id_salt("settings-voice-key")
                            .password(true)
                            .hint_text("sk-or-…"),
                    )
                    .desired_width(ui.available_width()),
                );
                crate::ui_probe::record(ui.ctx(), "settings-voice-key", response.rect);
                if response.changed() {
                    self.voice_form.key_test = None;
                    self.voice_form.message = None;
                }
                if self.voice_form.setup {
                    crate::ui_probe::record(ui.ctx(), "settings-voice-setup", response.rect);
                }
                if self.voice_form.focus_key {
                    response.scroll_to_me(Some(egui::Align::Center));
                    let settled = ui.memory(|memory| {
                        memory.has_focus(response.id) && memory.had_focus_last_frame(response.id)
                    });
                    if !response.has_focus() {
                        response.request_focus();
                    }
                    self.voice_form.focus_key = !settled || ui.is_sizing_pass();
                    if self.voice_form.focus_key {
                        ui.ctx().request_repaint();
                    }
                }
                if response.has_focus() {
                    crate::ui_probe::record(ui.ctx(), "settings-voice-key-focused", response.rect);
                }
                false
            },
        );
        let mut save = None;
        let mut test = false;
        feedback_row(ui, &mut self.voice_form.message);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let typed = !self.voice_form.key.trim().is_empty();
            let store = ui.add_enabled_ui(typed, |ui| {
                theme::button(ui, "Save key", theme::Kind::Primary)
            });
            crate::ui_probe::record(ui.ctx(), "settings-voice-save", store.inner.rect);
            if store.inner.clicked() {
                save = Some(false);
            }
            if self.voice_form.offer_session {
                let keep = ui.add_enabled_ui(typed, |ui| {
                    theme::button(ui, "Keep for this session only", theme::Kind::Outline)
                });
                crate::ui_probe::record(ui.ctx(), "settings-voice-session", keep.inner.rect);
                if keep.inner.clicked() {
                    save = Some(true);
                }
            }
            let pending = self.voice_form.key_test.is_some();
            let check = ui.add_enabled_ui(typed && !pending, |ui| {
                theme::button(
                    ui,
                    if pending { "Testing…" } else { "Test key" },
                    theme::Kind::Outline,
                )
            });
            crate::ui_probe::record(ui.ctx(), "settings-voice-test", check.inner.rect);
            if check.inner.clicked() {
                test = true;
            }
            if replacing.is_some() {
                let cancel = theme::button(ui, "Cancel", theme::Kind::Ghost);
                if cancel.clicked() {
                    self.voice_form = VoiceForm::default();
                }
            }
        });
        if test {
            let endpoint = self.voice.endpoint.clone();
            let key = self.voice_form.key.clone();
            let ctx = ui.ctx().clone();
            let (sender, receiver) = std::sync::mpsc::channel();
            self.voice_form.message = None;
            self.voice_form.key_test = Some(receiver);
            std::thread::spawn(move || {
                let _ = sender.send(rustidian::voice_account::test_key(&endpoint, &key));
                ctx.request_repaint();
            });
        }
        if let Some(session_only) = save {
            let key = std::mem::take(&mut self.voice_form.key);
            let result = rustidian::voice_account::state_base()
                .ok_or_else(|| {
                    SaveError::Settings(
                        "No voice settings folder is available; the account and key are unchanged."
                            .into(),
                    )
                })
                .and_then(|base| {
                    self.voice.save_key(
                        &base,
                        stores,
                        replacing.as_deref(),
                        &self.voice_form.label,
                        &key,
                        session_only,
                    )
                });
            match result {
                Ok(_) => {
                    self.voice_form = VoiceForm::default();
                    self.voice_form
                        .message(if !self.voice.pending_secret_removal.is_empty() {
                            "Key saved; an old key could not be removed. Retry key removal above."
                        } else if session_only {
                            "Key kept until Rustidian closes."
                        } else {
                            "Key saved in the system keyring."
                        });
                }
                Err(SaveError::Invalid(reason)) => {
                    self.voice_form.key = key;
                    self.voice_form.message(reason);
                }
                Err(SaveError::Keyring(reason)) => {
                    self.voice_form.key = key;
                    self.voice_form.offer_session = true;
                    let pending = if self.voice.pending_secret_removal.is_empty() {
                        ""
                    } else {
                        " An unused key is queued for removal; retry above."
                    };
                    self.voice_form.message(format!(
                        "{reason} The previous account and key are unchanged.{pending} Keep the key for this session only?"
                    ));
                }
                Err(SaveError::Settings(reason)) => {
                    self.reload_voice_on_conflict(&reason);
                    self.voice_form.key = key;
                    self.voice_form.message(reason);
                }
            }
            ui.ctx().request_repaint();
        }
    }

    fn save_voice_changes(&mut self, before: &Settings) {
        if self.voice != *before {
            let result = rustidian::voice_account::state_base()
                .ok_or_else(|| "No voice settings folder is available.".to_string())
                .and_then(|base| self.voice.save(&base));
            if let Err(error) = result {
                if !self.reload_voice_on_conflict(&error) {
                    self.voice = before.clone();
                }
                crate::read_aloud::set_speed(self.voice.speed);
                self.voice_form.message(format!(
                    "Voice settings were not saved; the previous settings are unchanged: {error}"
                ));
                self.ctx.request_repaint();
            }
        }
    }

    pub(super) fn reload_voice_on_conflict(&mut self, error: &str) -> bool {
        reload_voice_settings_on_conflict(&mut self.voice, error)
    }
}

/// Reload voice settings another window changed; true when `error` was that conflict. Takes only the
/// settings so callers holding other parts of the app (an open tab) can still use it.
pub(crate) fn reload_voice_settings_on_conflict(voice: &mut Settings, error: &str) -> bool {
    if error.contains("changed in another window")
        && let Some(base) = rustidian::voice_account::state_base()
    {
        *voice = Settings::load(&base);
        return true;
    }
    false
}
