//! One owner for Kokoro's process, pipes, latest request, and resettable idle timer.
use std::{
    io::{Read, Write},
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(10);
const MAX_LINE: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy)]
pub(super) struct Policy {
    pub startup: Duration,
    pub reply: Duration,
    pub idle: Duration,
}

type Reply = Result<serde_json::Value, String>;
struct Job {
    epoch: u64,
    request: Option<serde_json::Value>,
    reply: mpsc::SyncSender<Reply>,
}
#[derive(Default)]
struct Queue {
    latest: Option<Job>,
    shutdown: bool,
}
struct Shared {
    epoch: AtomicU64,
    queue: Mutex<Queue>,
    wake: Condvar,
}

pub(super) struct Service {
    shared: Arc<Shared>,
    done: Mutex<Receiver<()>>,
}

impl Service {
    pub fn start(
        command: impl Fn() -> Result<Command, String> + Send + 'static,
        policy: Policy,
    ) -> Result<Self, String> {
        let shared = Arc::new(Shared {
            epoch: AtomicU64::new(0),
            queue: Mutex::new(Queue::default()),
            wake: Condvar::new(),
        });
        let owner = Arc::clone(&shared);
        let (finished, done) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("rustidian-speech".into())
            .spawn(move || {
                run(owner, command, policy);
                let _ = finished.send(());
            })
            .map_err(|error| format!("Cannot start the voice service: {error}"))?;
        Ok(Self {
            shared,
            done: Mutex::new(done),
        })
    }

    pub fn epoch(&self) -> u64 {
        self.shared.epoch.load(Ordering::Acquire)
    }

    /// Latest-only admission: replacing a queued job disconnects its old receiver.
    pub fn submit(&self, epoch: u64, request: Option<serde_json::Value>) -> Receiver<Reply> {
        let (reply, receiver) = mpsc::sync_channel(1);
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        if request.is_none() && queue.latest.is_some() {
            return receiver;
        }
        if !queue.shutdown && epoch == self.epoch() {
            queue.latest = Some(Job {
                epoch,
                request,
                reply,
            });
            self.shared.wake.notify_one();
        }
        receiver
    }

    /// Never waits for a pipe, child or service thread.
    pub fn cancel(&self) {
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);
        queue.latest = None;
        self.shared.wake.notify_one();
    }

    /// Called after the event loop returns, outside the UI callback.
    pub fn wait_shutdown(&self, timeout: Duration) -> bool {
        self.shutdown();
        self.done
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recv_timeout(timeout)
            .is_ok()
    }

    pub fn shutdown(&self) {
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        self.shared.epoch.fetch_add(1, Ordering::AcqRel);
        queue.latest = None;
        queue.shutdown = true;
        self.shared.wake.notify_one();
    }
}

impl Drop for Service {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Installed immediately after spawn, before taking either pipe or reading readiness.
struct Worker {
    child: Child,
    input: Option<ChildStdin>,
    output: Option<ChildStdout>,
    buffered: Vec<u8>,
    device: String,
    next_id: u64,
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        // Reaping happens only on the service, never on egui's exit callback.
        let until = Instant::now() + Duration::from_secs(1);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => break,
                Ok(None) if Instant::now() < until => std::thread::sleep(POLL),
                Ok(None) => {
                    eprintln!("read aloud: voice did not exit after termination");
                    break;
                }
            }
        }
    }
}

#[cfg(unix)]
fn nonblocking(pipe: &impl std::os::fd::AsFd) -> Result<(), String> {
    use rustix::fs::{OFlags, fcntl_getfl, fcntl_setfl};
    let flags = fcntl_getfl(pipe).map_err(|error| error.to_string())?;
    fcntl_setfl(pipe, flags | OFlags::NONBLOCK).map_err(|error| error.to_string())
}

#[cfg(not(unix))]
fn nonblocking<T>(_pipe: &T) -> Result<(), String> {
    Err("Local speech requires interruptible process pipes on this system.".into())
}

fn check(shared: &Shared, epoch: u64, until: Instant) -> Result<(), String> {
    if shared.epoch.load(Ordering::Acquire) != epoch {
        Err("Reading cancelled.".into())
    } else if Instant::now() >= until {
        Err("The voice timed out. Try reading again.".into())
    } else {
        Ok(())
    }
}

impl Worker {
    fn start(
        mut command: Command,
        shared: &Shared,
        epoch: u64,
        timeout: Duration,
    ) -> Result<Self, String> {
        let until = Instant::now() + timeout;
        let child = command
            .spawn()
            .map_err(|error| format!("Cannot start the voice: {error}"))?;
        let mut worker = Self {
            child,
            input: None,
            output: None,
            buffered: Vec::new(),
            device: String::new(),
            next_id: 0,
        };
        worker.input = Some(worker.child.stdin.take().ok_or("The voice has no input")?);
        worker.output = Some(
            worker
                .child
                .stdout
                .take()
                .ok_or("The voice has no output")?,
        );
        nonblocking(worker.input.as_ref().ok_or("The voice has no input")?)?;
        nonblocking(worker.output.as_ref().ok_or("The voice has no output")?)?;
        loop {
            let message = worker.line(shared, epoch, until)?;
            if message["ready"] == true {
                worker.device = message["device"].as_str().unwrap_or("cpu").into();
                return Ok(worker);
            }
        }
    }

    fn line(&mut self, shared: &Shared, epoch: u64, until: Instant) -> Reply {
        loop {
            check(shared, epoch, until)?;
            if let Some(end) = self.buffered.iter().position(|byte| *byte == b'\n') {
                if end > MAX_LINE {
                    return Err("The voice reply exceeded its size limit.".into());
                }
                let line: Vec<_> = self.buffered.drain(..=end).collect();
                if let Ok(value) = serde_json::from_slice(&line) {
                    return Ok(value);
                }
                continue;
            }
            if self.buffered.len() >= MAX_LINE {
                return Err("The voice reply exceeded its size limit.".into());
            }
            let mut bytes = [0; 8192];
            match self
                .output
                .as_mut()
                .ok_or("The voice has no output")?
                .read(&mut bytes)
            {
                Ok(0) => return Err("The voice stopped.".into()),
                Ok(count) => self.buffered.extend_from_slice(&bytes[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(POLL)
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    fn say(
        &mut self,
        mut request: serde_json::Value,
        shared: &Shared,
        epoch: u64,
        timeout: Duration,
    ) -> Reply {
        let until = Instant::now() + timeout;
        self.next_id += 1;
        request["id"] = self.next_id.into();
        let mut bytes = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        let mut remaining = bytes.as_slice();
        while !remaining.is_empty() {
            check(shared, epoch, until)?;
            match self
                .input
                .as_mut()
                .ok_or("The voice has no input")?
                .write(remaining)
            {
                Ok(0) => return Err("The voice stopped accepting text.".into()),
                Ok(count) => remaining = &remaining[count..],
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(POLL)
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        loop {
            let mut reply = self.line(shared, epoch, until)?;
            if reply["id"] == self.next_id {
                reply["device"] = self.device.clone().into();
                return Ok(reply);
            }
        }
    }
}

fn run(shared: Arc<Shared>, command: impl Fn() -> Result<Command, String>, policy: Policy) {
    let mut worker: Option<Worker> = None;
    let mut used = Instant::now();
    loop {
        if used.elapsed() >= policy.idle {
            worker = None;
        }
        let job = {
            let mut queue = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            if queue.shutdown {
                return;
            }
            if queue.latest.is_none() {
                let wait = if worker.is_some() {
                    policy.idle.saturating_sub(used.elapsed())
                } else {
                    Duration::from_secs(3600)
                };
                queue = shared
                    .wake
                    .wait_timeout(queue, wait)
                    .unwrap_or_else(|e| e.into_inner())
                    .0;
            }
            if queue.shutdown {
                return;
            }
            queue.latest.take()
        };
        let Some(job) = job else { continue };
        let result = (|| {
            check(&shared, job.epoch, Instant::now() + policy.startup)?;
            if worker.is_none() {
                worker = Some(Worker::start(
                    command()?,
                    &shared,
                    job.epoch,
                    policy.startup,
                )?);
            }
            let voice = worker.as_mut().ok_or("The voice is unavailable")?;
            match job.request {
                Some(request) => voice.say(request, &shared, job.epoch, policy.reply),
                None => Ok(serde_json::json!({"ready": true, "device": voice.device})),
            }
        })();
        used = Instant::now();
        if result.is_err() {
            worker = None;
        }
        let _ = job.reply.send(result);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::{fs, path::Path, process::Stdio};

    fn fixture(mode: &str, policy: Policy) -> (tempfile::TempDir, Service) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("mode"), mode).unwrap();
        let root = dir.path().to_owned();
        let service = Service::start(
            move || {
                let mut command = Command::new("python3");
                command
                    .arg("-u")
                    .arg("-c")
                    .arg(
                        r#"
import json, os, pathlib, sys, time
root = pathlib.Path(sys.argv[1])
(root / 'pid').write_text(str(os.getpid()))
mode = (root / 'mode').read_text()
if mode == 'startup': time.sleep(60)
print(json.dumps({'ready': True, 'device': 'cpu'}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    (root / 'request').write_text('received')
    if mode == 'reply': time.sleep(60)
    print(json.dumps({'id': request['id'], 'ok': True, 'text': request['text']}), flush=True)
"#,
                    )
                    .arg(&root)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null());
                Ok(command)
            },
            policy,
        )
        .unwrap();
        (dir, service)
    }

    fn policy() -> Policy {
        Policy {
            startup: Duration::from_millis(250),
            reply: Duration::from_millis(250),
            idle: Duration::from_millis(250),
        }
    }
    fn wait_file(path: &Path) {
        let until = Instant::now() + Duration::from_secs(2);
        while !path.exists() && Instant::now() < until {
            std::thread::sleep(POLL);
        }
        assert!(path.exists(), "worker did not reach {}", path.display());
    }
    fn pid(root: &Path) -> u32 {
        fs::read_to_string(root.join("pid"))
            .unwrap()
            .parse()
            .unwrap()
    }
    fn gone(pid: u32) {
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "worker {pid} survived"
        );
    }

    #[test]
    fn startup_and_reply_deadlines_reap_the_child_and_allow_another_read() {
        for mode in ["startup", "reply"] {
            let (dir, service) = fixture(mode, policy());
            let reply = service.submit(service.epoch(), Some(serde_json::json!({"text": "first"})));
            let error = reply
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap_err();
            assert!(error.contains("timed out"), "{mode}: {error}");
            gone(pid(dir.path()));
            fs::write(dir.path().join("mode"), "normal").unwrap();
            let next = service.submit(service.epoch(), Some(serde_json::json!({"text": "next"})));
            assert_eq!(
                next.recv_timeout(Duration::from_secs(2)).unwrap().unwrap()["text"],
                "next"
            );
            let active = pid(dir.path());
            assert!(service.wait_shutdown(Duration::from_secs(2)));
            gone(active);
        }
    }

    #[test]
    fn stop_and_shutdown_interrupt_both_pipe_states_without_waiting_for_a_reply() {
        for mode in ["startup", "reply"] {
            let mut policy = policy();
            policy.startup = Duration::from_secs(30);
            policy.reply = Duration::from_secs(30);
            let (dir, service) = fixture(mode, policy);
            let reply = service.submit(service.epoch(), Some(serde_json::json!({"text": "first"})));
            wait_file(
                &dir.path()
                    .join(if mode == "startup" { "pid" } else { "request" }),
            );
            let old = pid(dir.path());
            let at = Instant::now();
            service.cancel();
            assert!(at.elapsed() < Duration::from_millis(100));
            assert!(reply.recv_timeout(Duration::from_secs(1)).unwrap().is_err());
            gone(old);
            fs::write(dir.path().join("mode"), "normal").unwrap();
            let next = service.submit(service.epoch(), Some(serde_json::json!({"text": "next"})));
            assert_eq!(
                next.recv_timeout(Duration::from_secs(2)).unwrap().unwrap()["text"],
                "next"
            );
            let active = pid(dir.path());
            assert!(service.wait_shutdown(Duration::from_secs(2)));
            gone(active);
            let (dir, service) = fixture(mode, policy);
            let pending =
                service.submit(service.epoch(), Some(serde_json::json!({"text": "exit"})));
            wait_file(
                &dir.path()
                    .join(if mode == "startup" { "pid" } else { "request" }),
            );
            let active = pid(dir.path());
            let at = Instant::now();
            service.shutdown();
            assert!(at.elapsed() < Duration::from_millis(100));
            assert!(service.wait_shutdown(Duration::from_secs(2)));
            assert!(
                pending
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap()
                    .is_err()
            );
            gone(active);
        }
    }

    #[test]
    fn one_idle_deadline_reuses_then_releases_the_worker() {
        let (dir, service) = fixture("normal", policy());
        let mut first = None;
        for _ in 0..5 {
            service
                .submit(service.epoch(), None)
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap();
            assert_eq!(
                *first.get_or_insert_with(|| pid(dir.path())),
                pid(dir.path())
            );
        }
        let active = pid(dir.path());
        let until = Instant::now() + Duration::from_secs(2);
        while Path::new(&format!("/proc/{active}")).exists() && Instant::now() < until {
            std::thread::sleep(POLL);
        }
        gone(active);
        assert!(service.wait_shutdown(Duration::from_secs(2)));
    }
}
