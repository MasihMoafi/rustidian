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
    pub message: Option<String>,
    /// The keyring failed: offer to keep the typed key for this session only.
    pub offer_session: bool,
    pub setup: bool,
    pub focus_key: bool,
    models: Vec<String>,
    models_endpoint: Option<String>,
    model_error: Option<String>,
    model_request: Option<std::sync::mpsc::Receiver<Result<Vec<String>, String>>>,
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
        let hosted_dictation = self.voice.dictation_backend == DictationBackend::OpenRouter
            || (!crate::dictation::available() && self.voice.selected_account().is_some());
        if hosted_dictation || self.voice_form.setup {
            ui.separator();
            self.transcription_model_picker(ui);
        }
        ui.separator();
        settings_ui::voice_row(
            ui,
            "Voice",
            "Local Kokoro, or a hosted voice through your OpenRouter key.",
            |ui| {
                let mut engine = self.voice.engine;
                let response = egui::ComboBox::from_id_salt("settings-voice-engine")
                    .icon(theme::combo_chevron)
                    .selected_text(match engine {
                        Engine::Local => "Local (Kokoro)",
                        Engine::OpenRouter => "OpenRouter",
                    })
                    .show_ui(ui, |ui| {
                        theme::menu_style(ui);
                        for (value, label) in [
                            (Engine::Local, "Local (Kokoro)"),
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
            || self.voice_form.setup
        {
            ui.separator();
            self.voice_text(
                ui,
                "Endpoint",
                "Keys are sent only to the endpoint they were saved for.",
                "settings-voice-endpoint",
                |v| &mut v.endpoint,
            );
            ui.separator();
            self.voice_accounts(ui);
        }
        if self.voice != before {
            self.save_voice();
        }
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
                    self.voice_form.model_error = Some(error);
                    self.voice_form.model_request = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.voice_form.model_error =
                        Some("The model list could not be loaded.".into());
                    self.voice_form.model_request = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        let description = self
            .voice_form
            .model_error
            .as_deref()
            .unwrap_or("Choose from OpenRouter's transcription models.");
        let mut fetch = false;
        settings_ui::voice_row(ui, "Transcription model", description, |ui| {
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
            crate::ui_probe::record(ui.ctx(), "settings-dictation-model", response.response.rect);
            if response.response.clicked()
                && self.voice_form.models.is_empty()
                && self.voice_form.model_request.is_none()
            {
                fetch = true;
            }
            false
        });
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
        let accounts = self.voice.accounts.clone();
        let status = match self.voice.selected_account() {
            Some(account) if account.endpoint == self.voice.endpoint => {
                format!("Using \"{}\" (key …{}).", account.label, account.ending)
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
                    match self.voice.disconnect(stores, id) {
                        Ok(()) => {
                            self.voice_form.message =
                                Some(format!("Disconnected \"{}\".", account.label))
                        }
                        Err(error) => self.voice_form.message = Some(error),
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
        let message = self.voice_form.message.clone().unwrap_or_default();
        settings_ui::voice_row(ui, "", &message, |ui| {
            let typed = !self.voice_form.key.trim().is_empty();
            if self.voice_form.offer_session {
                let keep = ui.add_enabled_ui(typed, |ui| {
                    theme::button(ui, "Keep for this session only", theme::Kind::Outline)
                });
                crate::ui_probe::record(ui.ctx(), "settings-voice-session", keep.inner.rect);
                if keep.inner.clicked() {
                    save = Some(true);
                }
            }
            let store = ui.add_enabled_ui(typed, |ui| {
                theme::button(ui, "Save key", theme::Kind::Primary)
            });
            crate::ui_probe::record(ui.ctx(), "settings-voice-save", store.inner.rect);
            if store.inner.clicked() {
                save = Some(false);
            }
            if replacing.is_some() {
                let cancel = theme::button(ui, "Cancel", theme::Kind::Ghost);
                if cancel.clicked() {
                    self.voice_form = VoiceForm::default();
                }
            }
            false
        });
        if let Some(session_only) = save {
            let key = std::mem::take(&mut self.voice_form.key);
            match self.voice.save_key(
                stores,
                replacing.as_deref(),
                &self.voice_form.label,
                &key,
                session_only,
            ) {
                Ok(_) => {
                    self.voice_form = VoiceForm {
                        message: Some(if session_only {
                            "Key kept until Rustidian closes.".into()
                        } else {
                            "Key saved in the system keyring.".into()
                        }),
                        ..VoiceForm::default()
                    };
                }
                Err(SaveError::Invalid(reason)) => {
                    self.voice_form.key = key;
                    self.voice_form.message = Some(reason);
                }
                Err(SaveError::Keyring(reason)) => {
                    self.voice_form.key = key;
                    self.voice_form.offer_session = true;
                    self.voice_form.message =
                        Some(format!("{reason} Keep the key for this session only?"));
                }
            }
        }
    }

    fn save_voice(&mut self) {
        if let Some(base) = rustidian::voice_account::state_base()
            && let Err(error) = self.voice.save(&base)
        {
            self.voice_form.message = Some(error);
        }
    }
}
