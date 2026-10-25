use notify::{Config, Event, PollWatcher, RecommendedWatcher, RecursiveMode, Result, Watcher};
use std::{path::Path, time::Duration};

/// Keep vault updates working when native watcher resources are exhausted.
pub fn watch(
    root: &Path,
    handler: impl FnMut(Result<Event>) + Clone + Send + 'static,
) -> Result<Box<dyn Watcher>> {
    let config = Config::default().with_follow_symlinks(false);
    let native = RecommendedWatcher::new(handler.clone(), config).and_then(|mut watcher| {
        watcher.watch(root, RecursiveMode::Recursive)?;
        Ok(watcher)
    });
    match native {
        Ok(watcher) => Ok(Box::new(watcher)),
        Err(error) => {
            eprintln!("Native vault watcher unavailable ({error}); using polling");
            let mut watcher = PollWatcher::new(
                handler,
                config
                    .with_poll_interval(Duration::from_secs(2))
                    .with_compare_contents(true),
            )?;
            watcher.watch(root, RecursiveMode::Recursive)?;
            Ok(Box::new(watcher))
        }
    }
}
