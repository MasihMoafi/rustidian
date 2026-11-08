//! Rustidian's own model account: the OpenRouter key read aloud uses for a hosted voice.
//! Follows the API-key route of the provider-login contract
//! (the shared provider-login spec): each key sits in the OS keyring, or only
//! in memory when the user declines a keyring that cannot be reached; the settings file
//! holds only secret-free facts (label, endpoint, the key's last four characters, which
//! account is selected). A key is sent only to the endpoint it was saved for.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

pub const DEFAULT_ENDPOINT: &str = "https://openrouter.ai/api/v1/audio/speech";
pub const DEFAULT_TRANSCRIPTION_MODEL: &str = "nvidia/parakeet-tdt-0.6b-v3";
pub const DEFAULT_MODEL: &str = "openai/gpt-4o-mini-tts-2025-12-15";
pub const DEFAULT_VOICE: &str = "coral";
/// OpenRouter's `pcm` speech: 24 kHz, mono, 16-bit little-endian samples.
pub const PCM_RATE: u32 = 24_000;
const KEYRING_SERVICE: &str = "rustidian-openrouter";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Engine {
    /// Kokoro on this machine.
    #[default]
    Local,
    OpenRouter,
}

/// One saved key, without the key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    /// Replacements use a fresh secret slot, so the old key stays usable until
    /// the settings file has committed. Older accounts use their account ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_id: Option<String>,
    pub label: String,
    /// The endpoint the key was saved for; no other endpoint ever receives it.
    pub endpoint: String,
    /// The key's last four characters, to tell keys apart.
    pub ending: String,
    /// Held in memory only; gone when Rustidian closes.
    #[serde(default)]
    pub session_only: bool,
}

impl Account {
    pub fn secret_id(&self) -> &str {
        self.secret_id.as_deref().unwrap_or(&self.id)
    }
}

/// A disconnected or replaced key that still needs removing from its store.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingSecret {
    pub id: String,
    pub session_only: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DictationBackend {
    #[default]
    Codex,
    Whisper,
    Parakeet,
    OpenRouter,
}

impl DictationBackend {
    pub fn command(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Whisper => "local",
            Self::Parakeet => "parakeet",
            Self::OpenRouter => "openrouter",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Detect settings saved by another Rustidian window.
    pub revision: u64,
    pub engine: Engine,
    pub dictation_backend: DictationBackend,
    pub transcription_model: String,
    pub model: String,
    pub voice: String,
    pub speed: Option<f32>,
    pub endpoint: String,
    pub accounts: Vec<Account>,
    pub selected: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pending_secret_removal: Vec<PendingSecret>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            revision: 0,
            engine: Engine::Local,
            dictation_backend: DictationBackend::Codex,
            transcription_model: DEFAULT_TRANSCRIPTION_MODEL.into(),
            model: DEFAULT_MODEL.into(),
            voice: DEFAULT_VOICE.into(),
            speed: None,
            endpoint: DEFAULT_ENDPOINT.into(),
            accounts: Vec::new(),
            selected: None,
            pending_secret_removal: Vec::new(),
        }
    }
}

/// A hosted route is used only after it has been chosen explicitly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceChoice {
    Local,
    Hosted,
    OfferHosted,
    Setup,
}

pub fn choose_engine(preferred: Engine, local_available: bool, key_available: bool) -> VoiceChoice {
    match (preferred, local_available, key_available) {
        (Engine::Local, true, _) => VoiceChoice::Local,
        (Engine::Local, false, true) => VoiceChoice::OfferHosted,
        (Engine::OpenRouter, _, true) => VoiceChoice::Hosted,
        _ => VoiceChoice::Setup,
    }
}

/// Where keys are kept. The OS keyring in the app; a map in tests.
pub trait Secrets {
    fn get(&self, id: &str) -> Result<Option<String>, String>;
    fn set(&self, id: &str, key: &str) -> Result<(), String>;
    fn delete(&self, id: &str) -> Result<(), String>;
}

/// The OS keyring (Secret Service on Linux, Keychain on macOS, Credential Manager on Windows).
pub struct Keyring;

impl Keyring {
    fn entry(id: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(KEYRING_SERVICE, id).map_err(keyring_error)
    }
}

fn keyring_error(error: keyring::Error) -> String {
    match error {
        keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
            "The system keyring is locked or unavailable. Unlock it and try again.".into()
        }
        _ => "The system keyring refused access. Unlock it and try again.".into(),
    }
}

impl Secrets for Keyring {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        match Self::entry(id)?.get_password() {
            Ok(key) => Ok(Some(key)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(keyring_error(error)),
        }
    }

    fn set(&self, id: &str, key: &str) -> Result<(), String> {
        Self::entry(id)?.set_password(key).map_err(keyring_error)
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        match Self::entry(id)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(keyring_error(error)),
        }
    }
}

/// Keys kept for this run only.
#[derive(Default)]
pub struct Memory(Mutex<HashMap<String, String>>);

impl Secrets for Memory {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        Ok(self
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .cloned())
    }

    fn set(&self, id: &str, key: &str) -> Result<(), String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.into(), key.into());
        Ok(())
    }

    fn delete(&self, id: &str) -> Result<(), String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).remove(id);
        Ok(())
    }
}

/// The app's session-only keys.
pub static SESSION: LazyLock<Memory> = LazyLock::new(Memory::default);

/// The two places a key can live.
#[derive(Clone, Copy)]
pub struct Stores<'a> {
    pub keyring: &'a dyn Secrets,
    pub session: &'a dyn Secrets,
}

impl Stores<'static> {
    pub fn app() -> Self {
        Self {
            keyring: &Keyring,
            session: &*SESSION,
        }
    }
}

/// What one spoken request needs.
pub struct Route {
    pub endpoint: String,
    pub model: String,
    pub voice: String,
    key: String,
}

impl std::fmt::Debug for Route {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Route")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("voice", &self.voice)
            .finish_non_exhaustive()
    }
}

/// Why a save could not use the keyring; the UI then offers session-only keeping.
#[derive(Debug, PartialEq, Eq)]
pub enum SaveError {
    Invalid(String),
    Keyring(String),
    Settings(String),
}

/// `$XDG_STATE_HOME`, else `~/.local/state`.
pub fn state_base() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
}

impl Settings {
    pub fn file(state_base: &Path) -> PathBuf {
        crate::paths::app_directory(state_base).join("voice.json")
    }

    /// Read the settings; a session-only account whose key is no longer in this run's
    /// memory (an earlier run's) is dropped.
    pub fn load(state_base: &Path) -> Self {
        let mut settings: Self = std::fs::read(Self::file(state_base))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        settings.accounts.retain(|account| {
            !account.session_only || SESSION.get(account.secret_id()).ok().flatten().is_some()
        });
        if settings
            .selected
            .as_ref()
            .is_some_and(|id| !settings.accounts.iter().any(|a| &a.id == id))
        {
            settings.selected = None;
        }
        settings
    }

    fn lock(state_base: &Path) -> Result<std::fs::File, String> {
        let folder = crate::paths::app_directory(state_base);
        std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(folder.join("voice.lock"))
            .map_err(|error| error.to_string())?;
        lock.lock().map_err(|error| error.to_string())?;
        Ok(lock)
    }

    /// Write the settings file (owner-only, never through a symlink).
    pub fn save(&mut self, state_base: &Path) -> Result<(), String> {
        let _lock = Self::lock(state_base)?;
        self.save_locked(state_base)
    }

    fn check_current(&self, state_base: &Path) -> Result<(), String> {
        let file = Self::file(state_base);
        if std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(
                "The voice settings file is a link; Rustidian will not write through it.".into(),
            );
        }
        let revision = match std::fs::read(&file) {
            Ok(bytes) => {
                serde_json::from_slice::<Settings>(&bytes)
                    .map_err(|_| "The voice settings file could not be read.")?
                    .revision
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.to_string()),
        };
        if revision != self.revision {
            return Err(
                "Voice settings changed in another window. Reopen Settings and try again.".into(),
            );
        }
        Ok(())
    }

    fn save_locked(&mut self, state_base: &Path) -> Result<(), String> {
        self.check_current(state_base)?;
        let file = Self::file(state_base);
        let folder = file.parent().ok_or("No settings folder")?;
        let revision = self
            .revision
            .checked_add(1)
            .ok_or("Voice settings revision overflowed.")?;
        let mut next = self.clone();
        next.revision = revision;
        let mut temporary =
            tempfile::NamedTempFile::new_in(folder).map_err(|error| error.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(temporary.path(), std::fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
        }
        std::io::Write::write_all(
            &mut temporary,
            &serde_json::to_vec_pretty(&next).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        temporary
            .persist(&file)
            .map_err(|error| error.error.to_string())?;
        self.revision = revision;
        Ok(())
    }

    pub fn selected_account(&self) -> Option<&Account> {
        let id = self.selected.as_ref()?;
        self.accounts.iter().find(|account| &account.id == id)
    }

    /// Save a new key or replace one without changing the active key until the
    /// settings file has been committed. Only the last four characters are saved there.
    pub fn save_key(
        &mut self,
        state_base: &Path,
        stores: Stores<'_>,
        replace: Option<&str>,
        label: &str,
        key: &str,
        session_only: bool,
    ) -> Result<String, SaveError> {
        let key = validate_key(key).map_err(SaveError::Invalid)?;
        endpoint_allowed(&self.endpoint).map_err(SaveError::Invalid)?;
        let _lock = Self::lock(state_base).map_err(|error| SaveError::Settings(format!(
            "Voice settings were not saved; the previous account and key are unchanged: {error}."
        )))?;
        let previous = match replace {
            Some(id) => Some(
                self.accounts
                    .iter()
                    .find(|account| account.id == id)
                    .cloned()
                    .ok_or_else(|| SaveError::Invalid("That account no longer exists.".into()))?,
            ),
            None => None,
        };
        let id = replace.map_or_else(|| format!("{:016x}", fastrand::u64(..)), str::to_owned);
        let secret_id = format!("{}-{:016x}", id, fastrand::u64(..));
        let store = if session_only {
            stores.session
        } else {
            stores.keyring
        };
        // Journal the temporary slot before touching the keyring. Even if the
        // process exits or deleting this slot fails, it can be cleaned later.
        let mut staged = self.clone();
        staged.pending_secret_removal.push(PendingSecret {
            id: secret_id.clone(),
            session_only,
        });
        staged.save_locked(state_base).map_err(|error| {
            SaveError::Settings(format!(
                "Voice settings were not saved; the previous account and key are unchanged: {error}."
            ))
        })?;
        *self = staged;
        if let Err(error) = store.set(&secret_id, key) {
            let _ = self.cleanup_pending_locked(state_base, stores);
            return Err(SaveError::Keyring(error));
        }
        let ending: String = key
            .chars()
            .rev()
            .take(4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let label = match label.trim() {
            "" => format!("OpenRouter {}", self.accounts.len() + 1),
            label => label.to_owned(),
        };
        let account = Account {
            id: id.clone(),
            secret_id: Some(secret_id.clone()),
            label,
            endpoint: self.endpoint.clone(),
            ending,
            session_only,
        };
        let mut next = self.clone();
        next.pending_secret_removal
            .retain(|pending| pending.id != secret_id || pending.session_only != session_only);
        match next.accounts.iter_mut().find(|a| a.id == id) {
            Some(existing) => {
                // A replacement keeps its name unless a new one was typed.
                let label = if replace.is_some() && account.label.starts_with("OpenRouter ") {
                    existing.label.clone()
                } else {
                    account.label.clone()
                };
                *existing = Account { label, ..account };
            }
            None => next.accounts.push(account),
        }
        next.selected = Some(id.clone());
        if let Some(previous) = previous {
            next.pending_secret_removal.push(PendingSecret {
                id: previous.secret_id().to_owned(),
                session_only: previous.session_only,
            });
        }
        if let Err(error) = next.save_locked(state_base) {
            let cleanup = self.cleanup_pending_locked(state_base, stores);
            let suffix = if cleanup.is_err() {
                " Removing the unused new key is pending; retry in Settings."
            } else {
                ""
            };
            return Err(SaveError::Settings(format!(
                "Voice settings were not saved; the previous account and key are unchanged: {error}.{suffix}"
            )));
        }
        *self = next;
        let _ = self.cleanup_pending_locked(state_base, stores);
        Ok(id)
    }

    /// Commit the disconnection before removing the old key. If removal fails,
    /// its secret-free pending record remains for a retry.
    pub fn disconnect(
        &mut self,
        state_base: &Path,
        stores: Stores<'_>,
        id: &str,
    ) -> Result<(), String> {
        let _lock = Self::lock(state_base).map_err(|error| {
            format!("Voice settings were not saved; the account and key are unchanged: {error}.")
        })?;
        let account = self
            .accounts
            .iter()
            .find(|account| account.id == id)
            .cloned()
            .ok_or("That account no longer exists.")?;
        let mut next = self.clone();
        next.accounts.retain(|account| account.id != id);
        if next.selected.as_deref() == Some(id) {
            next.selected = None;
        }
        next.pending_secret_removal.push(PendingSecret {
            id: account.secret_id().to_owned(),
            session_only: account.session_only,
        });
        next.save_locked(state_base).map_err(|error| {
            format!("Voice settings were not saved; the account and key are unchanged: {error}.")
        })?;
        *self = next;
        let _ = self.cleanup_pending_locked(state_base, stores);
        Ok(())
    }

    /// Retry committed removals. A failed cleanup remains recorded on disk;
    /// deleting an already-removed key is harmless.
    pub fn cleanup_pending(&mut self, state_base: &Path, stores: Stores<'_>) -> Result<(), String> {
        let _lock = Self::lock(state_base)?;
        self.cleanup_pending_locked(state_base, stores)
    }

    fn cleanup_pending_locked(
        &mut self,
        state_base: &Path,
        stores: Stores<'_>,
    ) -> Result<(), String> {
        self.check_current(state_base)?;
        if self.pending_secret_removal.is_empty() {
            return Ok(());
        }
        for pending in &self.pending_secret_removal {
            let store = if pending.session_only {
                stores.session
            } else {
                stores.keyring
            };
            store.delete(&pending.id)?;
        }
        let mut next = self.clone();
        next.pending_secret_removal.clear();
        next.save_locked(state_base)?;
        *self = next;
        Ok(())
    }

    /// The request route for the selected account; never another account's key, and
    /// never a key saved for a different endpoint.
    pub fn route(&self, stores: Stores<'_>) -> Result<Route, String> {
        let account = self
            .selected_account()
            .ok_or("Add an OpenRouter key in Settings → Read aloud.")?;
        if account.endpoint != self.endpoint {
            return Err(format!(
                "The key \"{}\" was saved for {}; add a key for {}.",
                account.label, account.endpoint, self.endpoint
            ));
        }
        endpoint_allowed(&self.endpoint)?;
        let store = if account.session_only {
            stores.session
        } else {
            stores.keyring
        };
        let key = store.get(account.secret_id())?.ok_or_else(|| {
            format!(
                "The key \"{}\" is no longer in the keyring; add it again.",
                account.label
            )
        })?;
        Ok(Route {
            endpoint: self.endpoint.clone(),
            model: self.model.clone(),
            voice: self.voice.clone(),
            key,
        })
    }
}

fn validate_key(key: &str) -> Result<&str, String> {
    let key = key.trim();
    if key.is_empty() || key.chars().any(char::is_whitespace) {
        Err("Paste the whole key, without spaces.".into())
    } else {
        Ok(key)
    }
}

/// HTTPS anywhere; plain HTTP only to this machine.
pub fn endpoint_allowed(endpoint: &str) -> Result<(), String> {
    let uri = endpoint.parse::<ureq::http::Uri>();
    if let Ok(uri) = uri
        && let Some(authority) = uri.authority()
        && !authority.as_str().contains('@')
        && !authority.host().is_empty()
    {
        match uri.scheme_str() {
            Some("https") => return Ok(()),
            Some("http") if transcription_origin(endpoint).is_ok() => return Ok(()),
            _ => {}
        }
    }
    Err("Use an HTTPS endpoint, or a local HTTP endpoint for testing.".into())
}

/// Voice `text` through the route; returns a WAV file's bytes. Error messages never hold
/// the key.
pub fn speak(route: &Route, text: &str, speed: f32) -> Result<Vec<u8>, String> {
    let body = serde_json::json!({
        "model": route.model,
        "input": text,
        "voice": route.voice,
        "response_format": "pcm",
        "speed": speed,
    })
    .to_string();
    let agent = provider_agent(&route.endpoint);
    let response = agent
        .post(&route.endpoint)
        .header("Authorization", &format!("Bearer {}", route.key))
        .header("Content-Type", "application/json")
        .header("HTTP-Referer", "https://github.com/MasihMoafi/rustidian")
        .header("X-Title", "Rustidian")
        .send(body.as_str());
    let mut response = match response {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return Err(format!(
                "OpenRouter could not voice this (HTTP {}). Try again.",
                response.status()
            ));
        }
        Err(ureq::Error::StatusCode(401 | 403)) => {
            return Err("OpenRouter refused this key. Replace it in Settings → Read aloud.".into());
        }
        Err(ureq::Error::StatusCode(402)) => {
            return Err(
                "OpenRouter: this key has no credit left. Add credit, then try again.".into(),
            );
        }
        Err(ureq::Error::StatusCode(code)) => {
            return Err(format!(
                "OpenRouter could not voice this (HTTP {code}). Try again."
            ));
        }
        Err(_) => {
            return Err(
                "OpenRouter cannot be reached. Check the connection, or choose the local voice in Settings → Read aloud."
                    .into(),
            );
        }
    };
    let mut pcm = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(64 << 20)
        .read_to_end(&mut pcm)
        .map_err(|_| "OpenRouter's audio was cut off.".to_string())?;
    if pcm.is_empty() {
        return Err("OpenRouter sent no audio.".into());
    }
    Ok(wav(&pcm, PCM_RATE))
}

// Only OpenRouter itself, or a strictly parsed loopback origin, may receive this key.
fn transcription_origin(endpoint: &str) -> Result<String, String> {
    if endpoint == DEFAULT_ENDPOINT {
        return Ok("https://openrouter.ai".into());
    }
    let authority = endpoint
        .strip_prefix("http://")
        .and_then(|rest| rest.split('/').next())
        .ok_or("This account is not configured for OpenRouter dictation.")?;
    let (host, port) = authority.split_once(':').unwrap_or((authority, ""));
    if !matches!(host, "localhost" | "127.0.0.1")
        || (!port.is_empty() && port.parse::<u16>().ok().filter(|port| *port != 0).is_none())
    {
        return Err("This account is not configured for OpenRouter dictation.".into());
    }
    Ok(format!("http://{authority}"))
}

fn provider_agent(endpoint: &str) -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .proxy(if endpoint.starts_with("http://") {
                None
            } else {
                ureq::Proxy::try_from_env()
            })
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .max_redirects(0)
            .build(),
    )
}

/// Check a typed key without saving it or exposing the provider's response.
pub fn test_key(endpoint: &str, key: &str) -> Result<(), String> {
    let key = validate_key(key)?;
    let origin = transcription_origin(endpoint).map_err(|_| {
        "Key testing is available for OpenRouter or a local endpoint. Check the endpoint."
            .to_string()
    })?;
    let url = format!("{origin}/api/v1/key");
    let mut response = match provider_agent(&url)
        .get(&url)
        .header("Authorization", &format!("Bearer {key}"))
        .call()
    {
        Ok(response) if response.status().is_success() => response,
        Ok(response) => {
            return Err(format!(
                "OpenRouter could not test the key (HTTP {}). Try again.",
                response.status()
            ));
        }
        Err(ureq::Error::StatusCode(401 | 403)) => {
            return Err("OpenRouter refused this key. Check it and try again.".into());
        }
        Err(ureq::Error::StatusCode(402)) => {
            return Err(
                "OpenRouter: this key has no credit left. Add credit, then try again.".into(),
            );
        }
        Err(ureq::Error::StatusCode(code)) => {
            return Err(format!(
                "OpenRouter could not test the key (HTTP {code}). Try again."
            ));
        }
        Err(_) => {
            return Err("OpenRouter cannot be reached. Check the connection and try again.".into());
        }
    };
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(1 << 20)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the key test response. Try again.")?;
    let valid = serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()
        .is_some_and(|value| value.get("data").is_some_and(serde_json::Value::is_object));
    if valid {
        Ok(())
    } else {
        Err("OpenRouter returned an invalid key test response. Try again.".into())
    }
}

/// Available transcription model IDs, fetched without credentials.
pub fn transcription_models(endpoint: &str) -> Result<Vec<String>, String> {
    let origin = transcription_origin(endpoint)?;
    let url = format!("{origin}/api/v1/models?output_modalities=transcription");
    let mut response = provider_agent(&url)
        .get(&url)
        .call()
        .map_err(|_| "Could not load OpenRouter transcription models. Check the connection.")?;
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(4 << 20)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the transcription models.")?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "OpenRouter returned an invalid model list.")?;
    let entries = value
        .get("data")
        .and_then(|data| data.as_array())
        .ok_or("OpenRouter returned an invalid model list.")?;
    let mut models: Vec<String> = entries
        .iter()
        .filter_map(|entry| entry.get("id")?.as_str())
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect();
    models.sort();
    models.dedup();
    if models.is_empty() {
        return Err("OpenRouter returned no transcription models.".into());
    }
    Ok(models)
}

/// Transcribe WAV bytes using the selected account, without exposing its key in errors.
pub fn transcribe(route: &Route, model: &str, wav: &[u8]) -> Result<String, String> {
    use base64::Engine as _;
    let origin = transcription_origin(&route.endpoint)?;
    let mut endpoint = format!("{origin}/api/v1/audio/transcriptions");
    // Test overrides may use only the same loopback origin for which the key was saved.
    if origin.starts_with("http://")
        && let Ok(overridden) = std::env::var("RUSTIDIAN_TRANSCRIPTION_ENDPOINT")
        && overridden
            .strip_prefix(&origin)
            .is_some_and(|suffix| suffix.starts_with('/'))
    {
        endpoint = overridden;
    }
    let body = serde_json::json!({
        "model": model,
        "language": "en",
        "input_audio": {
            "data": base64::engine::general_purpose::STANDARD.encode(wav),
            "format": "wav"
        }
    })
    .to_string();
    let mut response = match provider_agent(&endpoint)
        .post(&endpoint)
        .header("Authorization", &format!("Bearer {}", route.key))
        .header("Content-Type", "application/json")
        .send(body.as_str())
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(401 | 403)) => {
            return Err("OpenRouter refused this key. Replace it in Settings → Read aloud.".into());
        }
        Err(ureq::Error::StatusCode(402)) => {
            return Err(
                "OpenRouter: this key has no credit left. Add credit, then try again.".into(),
            );
        }
        Err(ureq::Error::StatusCode(code)) => {
            return Err(format!(
                "OpenRouter could not transcribe this (HTTP {code}). Try again."
            ));
        }
        Err(_) => return Err("OpenRouter cannot be reached. Check the connection.".into()),
    };
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(1 << 20)
        .read_to_end(&mut bytes)
        .map_err(|_| "OpenRouter's transcript was cut off.")?;
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|_| "OpenRouter returned an invalid transcript.")?;
    value
        .get("text")
        .and_then(|text| text.as_str())
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| "No speech was transcribed.".into())
}

/// Wrap mono 16-bit little-endian samples in a WAV header.
pub fn wav(pcm: &[u8], rate: u32) -> Vec<u8> {
    let data = (pcm.len() & !1) as u32;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    out.extend_from_slice(&pcm[..data as usize]);
    out
}

#[cfg(test)]
mod speed_tests {
    use super::Settings;
    #[test]
    fn read_aloud_speed_survives_a_settings_restart() {
        let state = tempfile::tempdir().unwrap();
        let mut settings = Settings::default();
        assert_eq!(settings.speed, None);
        settings.speed = Some(1.3);
        settings.save(state.path()).unwrap();
        assert_eq!(Settings::load(state.path()).speed, Some(1.3));
    }
}

#[cfg(test)]
mod endpoint_tests {
    use super::endpoint_allowed;

    #[test]
    fn local_http_endpoints_cannot_hide_another_host_in_the_authority() {
        for endpoint in [
            "http://localhost:8000@outside.example/speech",
            "http://127.0.0.1:8000@outside.example/speech",
            "http://localhost.outside.example/speech",
            "https://user:password@outside.example/speech",
        ] {
            assert!(endpoint_allowed(endpoint).is_err(), "accepted {endpoint}");
        }
        for endpoint in [
            "http://127.0.0.1:8000/speech",
            "http://localhost/speech",
            "https://openrouter.ai/api/v1/audio/speech",
        ] {
            assert!(endpoint_allowed(endpoint).is_ok(), "rejected {endpoint}");
        }
    }
}
