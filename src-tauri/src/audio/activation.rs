use super::{AudioError, AudioResult};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

pub fn receive<T>(
    receiver: Receiver<Result<T, String>>,
    stop: &AtomicBool,
    timeout: Duration,
) -> AudioResult<T> {
    let deadline = Instant::now() + timeout;
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(AudioError::CaptureFailed("Activation cancelled".into()));
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(AudioError::CaptureFailed(
                "Timed out activating process loopback".into(),
            ));
        }
        match receiver.recv_timeout(remaining.min(Duration::from_millis(10))) {
            Ok(result) => return result.map_err(AudioError::CaptureFailed),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(AudioError::CaptureFailed(
                    "Activation callback disconnected".into(),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, mpsc};
    #[test]
    fn cancellation_drops_late_owned_results_without_waiting_for_timeout() {
        let stop = AtomicBool::new(true);
        let (sender, receiver) = mpsc::channel();
        let owned = Arc::new(());
        assert!(receive(receiver, &stop, Duration::from_secs(5)).is_err());
        drop(sender.send(Ok(Arc::clone(&owned))));
        assert_eq!(Arc::strong_count(&owned), 1);
    }
}
