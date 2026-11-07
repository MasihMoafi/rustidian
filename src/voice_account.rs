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
    pub label: String,
    /// The endpoint the key was saved for; no other endpoint ever receives it.
    pub endpoint: String,
    /// The key's last four characters, to tell keys apart.
    pub ending: String,
    /// Held in memory only; gone when Rustidian closes.
    #[serde(default)]
    pub session_only: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub engine: Engine,
    pub model: String,
    pub voice: String,
    pub endpoint: String,
    pub accounts: Vec<Account>,
    pub selected: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            engine: Engine::Local,
            model: DEFAULT_MODEL.into(),
            voice: DEFAULT_VOICE.into(),
            endpoint: DEFAULT_ENDPOINT.into(),
            accounts: Vec::new(),
            selected: None,
        }
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
            "The system keyring is locked or not available.".into()
        }
        _ => "The system keyring refused the key.".into(),
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
            !account.session_only || SESSION.get(&account.id).ok().flatten().is_some()
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

    /// Write the settings file (owner-only, never through a symlink).
    pub fn save(&self, state_base: &Path) -> Result<(), String> {
        let file = Self::file(state_base);
        let folder = file.parent().ok_or("No settings folder")?;
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        if std::fs::symlink_metadata(&file).is_ok_and(|m| m.file_type().is_symlink()) {
            return Err(
                "The voice settings file is a link; Rustidian will not write through it.".into(),
            );
        }
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
            &serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        temporary
            .persist(&file)
            .map_err(|error| error.error.to_string())?;
        Ok(())
    }

    pub fn selected_account(&self) -> Option<&Account> {
        let id = self.selected.as_ref()?;
        self.accounts.iter().find(|account| &account.id == id)
    }

    /// Save a new key (`replace` = None) or a replacement for an account, then select it.
    /// The key goes to `secrets`; only its last four characters reach the settings.
    pub fn save_key(
        &mut self,
        stores: Stores<'_>,
        replace: Option<&str>,
        label: &str,
        key: &str,
        session_only: bool,
    ) -> Result<String, SaveError> {
        let key = key.trim();
        if key.is_empty() || key.chars().any(char::is_whitespace) {
            return Err(SaveError::Invalid(
                "Paste the whole key, without spaces.".into(),
            ));
        }
        endpoint_allowed(&self.endpoint).map_err(SaveError::Invalid)?;
        let id = replace.map_or_else(|| format!("{:016x}", fastrand::u64(..)), str::to_owned);
        let store = if session_only {
            stores.session
        } else {
            stores.keyring
        };
        store.set(&id, key).map_err(SaveError::Keyring)?;
        if replace.is_some() {
            // A replaced key must not linger in the store it is leaving.
            let other = if session_only {
                stores.keyring
            } else {
                stores.session
            };
            let _ = other.delete(&id);
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
            label,
            endpoint: self.endpoint.clone(),
            ending,
            session_only,
        };
        match self.accounts.iter_mut().find(|a| a.id == id) {
            Some(existing) => {
                // A replacement keeps its name unless a new one was typed.
                let label = if replace.is_some() && account.label.starts_with("OpenRouter ") {
                    existing.label.clone()
                } else {
                    account.label.clone()
                };
                *existing = Account { label, ..account };
            }
            None => self.accounts.push(account),
        }
        self.selected = Some(id.clone());
        Ok(id)
    }

    /// Forget an account and delete its key from `secrets`.
    pub fn disconnect(&mut self, stores: Stores<'_>, id: &str) -> Result<(), String> {
        let session_only = self
            .accounts
            .iter()
            .find(|account| account.id == id)
            .is_some_and(|account| account.session_only);
        if session_only {
            stores.session.delete(id)?;
        } else {
            stores.keyring.delete(id)?;
        }
        self.accounts.retain(|account| account.id != id);
        if self.selected.as_deref() == Some(id) {
            self.selected = None;
        }
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
        let key = store.get(&account.id)?.ok_or_else(|| {
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

/// HTTPS anywhere; plain HTTP only to this machine.
pub fn endpoint_allowed(endpoint: &str) -> Result<(), String> {
    let local = [
        "http://127.0.0.1:",
        "http://127.0.0.1/",
        "http://localhost:",
        "http://localhost/",
    ];
    if endpoint.starts_with("https://") || local.iter().any(|prefix| endpoint.starts_with(prefix)) {
        Ok(())
    } else {
        Err("The voice endpoint must start with https://.".into())
    }
}

/// Voice `text` through the route; returns a WAV file's bytes. Error messages never hold
/// the key.
pub fn speak(route: &Route, text: &str) -> Result<Vec<u8>, String> {
    let body = serde_json::json!({
        "model": route.model,
        "input": text,
        "voice": route.voice,
        "response_format": "pcm",
    })
    .to_string();
    // A proxy from the environment carries hosted requests; this machine is reached directly.
    let local = route.endpoint.starts_with("http://");
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .proxy(if local {
                None
            } else {
                ureq::Proxy::try_from_env()
            })
            .build(),
    );
    let response = agent
        .post(&route.endpoint)
        .header("Authorization", &format!("Bearer {}", route.key))
        .header("Content-Type", "application/json")
        .header("HTTP-Referer", "https://github.com/MasihMoafi/rustidian")
        .header("X-Title", "Rustidian")
        .send(body.as_str());
    let mut response = match response {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(401 | 403)) => {
            return Err("OpenRouter refused this key. Replace it in Settings → Read aloud.".into());
        }
        Err(ureq::Error::StatusCode(402)) => {
            return Err("OpenRouter: this key has no credit left.".into());
        }
        Err(ureq::Error::StatusCode(code)) => {
            return Err(format!("OpenRouter could not voice this (HTTP {code})."));
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
