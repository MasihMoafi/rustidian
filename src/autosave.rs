//! Autosave writes run on one background thread, in submission order, so a slow
//! disk (`fsync`) never stalls a frame. The UI thread commits each result.
use crate::vault::{PendingSave, Result};
use std::{
    collections::HashSet,
    sync::mpsc::{self, Receiver, Sender, TryRecvError},
};

pub struct Done {
    /// The tab that asked for the save.
    pub tab: u64,
    pub save: PendingSave,
    pub result: Result<()>,
}

#[derive(Default)]
pub struct Writer {
    worker: Option<(Sender<(u64, PendingSave)>, Receiver<Done>)>,
    in_flight: HashSet<u64>,
    finished: Vec<Done>,
}

impl Writer {
    pub fn busy(&self, tab: u64) -> bool {
        self.in_flight.contains(&tab)
    }

    /// Queues one write; `wake` runs after each finished write.
    pub fn submit(&mut self, tab: u64, save: PendingSave, wake: impl Fn() + Send + 'static) {
        if self.worker.is_none() {
            let (jobs, job_receiver) = mpsc::channel::<(u64, PendingSave)>();
            let (done_sender, done) = mpsc::channel();
            let spawned = std::thread::Builder::new()
                .name("rustidian-autosave".into())
                .spawn(move || {
                    for (tab, save) in job_receiver {
                        let result = save.write();
                        if done_sender.send(Done { tab, save, result }).is_err() {
                            break;
                        }
                        wake();
                    }
                });
            if spawned.is_ok() {
                self.worker = Some((jobs, done));
            }
        }
        self.in_flight.insert(tab);
        let save = match &self.worker {
            Some((jobs, _)) => match jobs.send((tab, save)) {
                Ok(()) => return,
                Err(error) => error.0.1,
            },
            None => save,
        };
        // No worker thread: write here rather than lose the edit.
        let result = save.write();
        self.finished.push(Done { tab, save, result });
    }

    /// Writes that have finished, without waiting.
    pub fn finished(&mut self) -> Vec<Done> {
        self.collect(false)
    }

    /// Every submitted write, waiting for those still running.
    pub fn wait(&mut self) -> Vec<Done> {
        self.collect(true)
    }

    fn collect(&mut self, block: bool) -> Vec<Done> {
        let mut finished = std::mem::take(&mut self.finished);
        for done in &finished {
            self.in_flight.remove(&done.tab);
        }
        while let Some((_, receiver)) = &self.worker {
            let next = if block && !self.in_flight.is_empty() {
                receiver.recv().map_err(|_| TryRecvError::Disconnected)
            } else {
                receiver.try_recv()
            };
            match next {
                Ok(done) => {
                    self.in_flight.remove(&done.tab);
                    finished.push(done);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // The worker stopped. Its tabs stay unsaved, so the next
                    // autosave starts a new one.
                    self.worker = None;
                    self.in_flight.clear();
                }
            }
        }
        finished
    }
}
