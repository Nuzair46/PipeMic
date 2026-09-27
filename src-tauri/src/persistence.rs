use crate::config::{self, AppConfig};
use parking_lot::{Condvar, Mutex};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension(format!(
        "{}.{}.tmp",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    let result = (|| {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        replace_file(&temporary, path)?;
        #[cfg(unix)]
        if let Some(parent) = path.parent() {
            fs::File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)
}

#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    use windows::{
        Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        },
        core::HSTRING,
    };
    unsafe {
        MoveFileExW(
            &HSTRING::from(from.as_os_str()),
            &HSTRING::from(to.as_os_str()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| io::Error::other(error.to_string()))
}

#[derive(Default)]
struct PendingWrites {
    next: u64,
    completed: u64,
    pending: Option<(u64, AppConfig)>,
    deadline: Option<Instant>,
    flush: bool,
    stopping: bool,
    error: Option<String>,
}

pub struct ConfigWriter {
    state: Arc<(Mutex<PendingWrites>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl ConfigWriter {
    pub fn new(path: PathBuf) -> Self {
        Self::with_sink(move |config| {
            config::save_config_to_path(config, &path).map_err(|error| error.to_string())
        })
    }

    pub fn with_sink(
        mut save: impl FnMut(&AppConfig) -> Result<(), String> + Send + 'static,
    ) -> Self {
        let state = Arc::new((Mutex::new(PendingWrites::default()), Condvar::new()));
        let shared = Arc::clone(&state);
        let worker = thread::spawn(move || {
            let (mutex, changed) = &*shared;
            loop {
                let mut state = mutex.lock();
                while state.pending.is_none() && !state.stopping {
                    changed.wait(&mut state);
                }
                if state.pending.is_none() && state.stopping {
                    return;
                }
                while !state.flush && !state.stopping {
                    let deadline = state.deadline.unwrap();
                    if Instant::now() >= deadline {
                        break;
                    }
                    changed.wait_until(&mut state, deadline);
                }
                let (revision, config) = state.pending.take().unwrap();
                state.deadline = None;
                state.flush = false;
                drop(state);
                let error = save(&config).err();
                let mut state = mutex.lock();
                state.completed = revision;
                state.error = error;
                changed.notify_all();
            }
        });
        Self {
            state,
            worker: Some(worker),
        }
    }

    pub fn schedule(&self, config: &AppConfig) {
        let (mutex, changed) = &*self.state;
        let mut state = mutex.lock();
        state.next += 1;
        state.pending = Some((state.next, config.clone()));
        state
            .deadline
            .get_or_insert_with(|| Instant::now() + Duration::from_millis(200));
        changed.notify_all();
    }

    pub fn error(&self) -> Option<String> {
        self.state.0.lock().error.clone()
    }

    pub fn flush(&self) -> Result<(), String> {
        let (mutex, changed) = &*self.state;
        let mut state = mutex.lock();
        let revision = state.next;
        state.flush = true;
        changed.notify_all();
        while state.completed < revision {
            changed.wait(&mut state);
        }
        state.error.clone().map_or(Ok(()), Err)
    }
}

impl Drop for ConfigWriter {
    fn drop(&mut self) {
        self.state.0.lock().stopping = true;
        self.state.1.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_persists_the_last_slider_value_and_reports_failure() {
        let saved = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&saved);
        let writer = ConfigWriter::with_sink(move |config| {
            seen.lock().push(config.master_gain);
            Ok(())
        });
        for gain in [0.2, 0.5, 0.8] {
            writer.schedule(&AppConfig {
                master_gain: gain,
                ..AppConfig::default()
            });
        }
        writer.flush().unwrap();
        assert_eq!(*saved.lock(), [0.8]);
        let failing = ConfigWriter::with_sink(|_| Err("Disk full".into()));
        failing.schedule(&AppConfig::default());
        assert_eq!(failing.flush().unwrap_err(), "Disk full");
    }

    #[test]
    fn shutdown_flushes_pending_changes() {
        let saved = Arc::new(Mutex::new(None));
        let seen = Arc::clone(&saved);
        let writer = ConfigWriter::with_sink(move |config| {
            *seen.lock() = Some(config.master_gain);
            Ok(())
        });
        writer.schedule(&AppConfig {
            master_gain: 0.3,
            ..AppConfig::default()
        });
        drop(writer);
        assert_eq!(*saved.lock(), Some(0.3));
    }
}
