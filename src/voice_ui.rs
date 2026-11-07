//! Settings → Read aloud: choose the local voice or the user's own OpenRouter account,
//! and add, select, replace or disconnect OpenRouter keys (`rustidian::voice_account`).
use super::*;
use rustidian::voice_account::{Engine, SaveError, Settings, Stores};

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
}

impl NotesApp {
    pub(super) fn voice_settings(&mut self, ui: &mut egui::Ui) {
        let before = self.voice.clone();
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
            "Kept in the system keyring, never in a file. Create one at openrouter.ai/keys.",
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
