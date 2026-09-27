use super::{
    devices, sessions,
    types::{AudioDevice, AudioSession},
};
use parking_lot::{Condvar, Mutex};
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone)]
pub struct Snapshot {
    pub capture: Result<Vec<AudioDevice>, String>,
    pub render: Result<Vec<AudioDevice>, String>,
    pub sessions: Result<Vec<AudioSession>, String>,
}

struct State {
    snapshot: Snapshot,
    ready: bool,
    stopping: bool,
}

pub struct Discovery {
    state: Arc<(Mutex<State>, Condvar)>,
    worker: Option<JoinHandle<()>>,
}

impl Discovery {
    pub fn new() -> Self {
        let state = Arc::new((
            Mutex::new(State {
                snapshot: Snapshot {
                    capture: Err("Microphones are loading".into()),
                    render: Err("Outputs are loading".into()),
                    sessions: Err("Applications are loading".into()),
                },
                ready: false,
                stopping: false,
            }),
            Condvar::new(),
        ));
        let shared = Arc::clone(&state);
        let worker = thread::spawn(move || {
            loop {
                let capture = devices::list_capture_devices().map_err(|e| e.to_string());
                let render = devices::list_render_devices().map_err(|e| e.to_string());
                let sessions = sessions::list_sessions().map_err(|e| e.to_string());
                let (mutex, changed) = &*shared;
                let mut state = mutex.lock();
                // Transient enumeration failures retain the last successful snapshot.
                if capture.is_ok() || state.snapshot.capture.is_err() {
                    state.snapshot.capture = capture;
                }
                if render.is_ok() || state.snapshot.render.is_err() {
                    state.snapshot.render = render;
                }
                if sessions.is_ok() || state.snapshot.sessions.is_err() {
                    state.snapshot.sessions = sessions;
                }
                state.ready = true;
                changed.notify_all();
                if state.stopping {
                    break;
                }
                changed.wait_for(&mut state, Duration::from_secs(1));
                if state.stopping {
                    break;
                }
            }
        });
        Self {
            state,
            worker: Some(worker),
        }
    }

    pub fn snapshot(&self) -> Snapshot {
        self.state.0.lock().snapshot.clone()
    }

    pub fn initial_snapshot(&self) -> Snapshot {
        let mut state = self.state.0.lock();
        while !state.ready {
            self.state.1.wait(&mut state);
        }
        state.snapshot.clone()
    }
}

impl Drop for Discovery {
    fn drop(&mut self) {
        self.state.0.lock().stopping = true;
        self.state.1.notify_all();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
