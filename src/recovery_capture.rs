//! A bounded background queue; neither database access nor content hashing runs
//! in an editor frame. Deferred versions coalesce by file until their interval.
use crate::recovery::{Config, History};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

enum Request {
    Capture(PathBuf, String, Config),
    Clear(mpsc::Sender<Result<(), String>>),
    Configure(Config),
}

struct Deferred {
    text: String,
    config: Config,
    due: Instant,
}

pub struct Recorder {
    sender: SyncSender<Request>,
    pub errors: Receiver<String>,
    pub events: Receiver<PathBuf>,
    config: Config,
}

impl Recorder {
    pub fn start(history: History, config: Config, wake: impl Fn() + Send + 'static) -> Self {
        let (sender, receiver) = mpsc::sync_channel(16);
        let (errors, error_receiver) = mpsc::channel();
        let (events, event_receiver) = mpsc::channel();
        let initial_config = config.clone();
        std::thread::spawn(move || {
            let mut pending = HashMap::<PathBuf, Deferred>::new();
            let mut prune_after = Instant::now();
            let mut current_config = config;
            loop {
                let timeout = pending
                    .values()
                    .map(|entry| entry.due.saturating_duration_since(Instant::now()))
                    .min()
                    .unwrap_or(Duration::from_secs(60))
                    .min(Duration::from_secs(60));
                match receiver.recv_timeout(timeout) {
                    Ok(Request::Capture(path, text, config)) => {
                        current_config = config.clone();
                        pending.insert(
                            path,
                            Deferred {
                                text,
                                config,
                                due: Instant::now(),
                            },
                        );
                    }
                    Ok(Request::Clear(reply)) => {
                        pending.clear();
                        let _ = reply.send(history.clear());
                        wake();
                    }
                    Ok(Request::Configure(config)) => {
                        current_config = config.clone();
                        for entry in pending.values_mut() {
                            entry.config = config.clone();
                            entry.due = Instant::now();
                        }
                        prune_after = Instant::now();
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
                if Instant::now() >= prune_after {
                    if let Err(error) =
                        history.prune(chrono::Local::now().timestamp_millis(), &current_config)
                    {
                        let _ = errors.send(error);
                        wake();
                    }
                    prune_after = Instant::now() + Duration::from_secs(3600);
                }
                let due: Vec<_> = pending
                    .iter()
                    .filter(|(_, entry)| entry.due <= Instant::now())
                    .map(|(path, _)| path.clone())
                    .collect();
                for path in due {
                    let Some(mut entry) = pending.remove(&path) else {
                        continue;
                    };
                    let now = chrono::Local::now().timestamp_millis();
                    let result = (|| -> Result<bool, String> {
                        if history.record(&path, &entry.text, now, &entry.config, false)? {
                            let _ = events.send(path.clone());
                            return Ok(true);
                        }
                        let latest = history
                            .versions(&path)?
                            .into_iter()
                            .next()
                            .ok_or("Snapshot metadata disappeared.")?;
                        if history.content(&path, latest.id)? == entry.text {
                            return Ok(true);
                        }
                        let delay = latest
                            .timestamp_ms
                            .saturating_add((entry.config.interval_minutes * 60_000.0) as i64)
                            .saturating_sub(now)
                            .max(1) as u64;
                        entry.due = Instant::now() + Duration::from_millis(delay.min(60_000));
                        Ok(false)
                    })();
                    match result {
                        Ok(false) => {
                            pending.insert(path, entry);
                        }
                        Ok(true) => wake(),
                        Err(error) => {
                            let _ = errors.send(error);
                            wake();
                        }
                    }
                }
            }
        });
        Self {
            sender,
            errors: error_receiver,
            events: event_receiver,
            config: initial_config,
        }
    }

    /// False means the caller retains its revision and retries on a later frame.
    pub fn capture(&self, path: &Path, text: &str, config: &Config) -> bool {
        self.sender
            .try_send(Request::Capture(
                path.to_owned(),
                text.to_owned(),
                config.clone(),
            ))
            .is_ok()
    }

    pub fn configure(&mut self, config: &Config) -> bool {
        if self.config == *config {
            return true;
        }
        if self
            .sender
            .try_send(Request::Configure(config.clone()))
            .is_err()
        {
            return false;
        }
        self.config = config.clone();
        true
    }

    pub fn clear(&self) -> Result<Receiver<Result<(), String>>, String> {
        let (reply, receiver) = mpsc::channel();
        self.sender
            .try_send(Request::Clear(reply))
            .map_err(|_| "Local history is busy; try again shortly.")?;
        Ok(receiver)
    }
}
