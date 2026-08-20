//! Anonymous usage reporting.
//!
//! Off unless the user allows it: [`consent`] decides, and a disabled
//! [`Telemetry`] starts no thread and writes no files. Everything reported
//! goes through [`event::Event`], which buckets numbers so a report cannot
//! single out a machine.

pub(crate) mod consent;
pub(crate) mod device;
pub(crate) mod event;
pub(crate) mod queue;
pub(crate) mod sink;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

pub use event::Event;
use sink::Sink;

/// Reports held back until this many pile up.
const BATCH_SIZE: usize = 20;
/// How long a partial batch waits before going out anyway.
const BATCH_WAIT: Duration = Duration::from_secs(30);

/// The app's handle on reporting. A disabled one is a no-op all the way down.
pub struct Telemetry {
    worker: Option<Worker>,
}

struct Worker {
    /// Wakes the sending thread; the reports themselves travel through the
    /// queue file, so a kill between two launches loses nothing.
    wake: Sender<()>,
    thread: JoinHandle<()>,
    pending: PathBuf,
    distinct_id: String,
    base: serde_json::Map<String, Value>,
}

impl Telemetry {
    pub fn disabled() -> Self {
        Telemetry { worker: None }
    }

    /// Spawns the sending thread. A refused spawn disables reporting rather
    /// than failing the launch.
    pub fn start(
        distinct_id: String,
        channel: &str,
        pending: PathBuf,
        sink: Box<dyn Sink>,
    ) -> Self {
        let (wake, wakeups) = mpsc::channel();
        let queue_file = pending.clone();
        let thread = std::thread::Builder::new()
            .name("telemetry".into())
            .spawn(move || run(&wakeups, &queue_file, sink.as_ref()));
        match thread {
            Ok(thread) => Telemetry {
                worker: Some(Worker {
                    wake,
                    thread,
                    pending,
                    distinct_id,
                    base: sink::base_props(channel),
                }),
            },
            Err(_) => Telemetry::disabled(),
        }
    }

    pub fn track(&self, event: Event) {
        let Some(worker) = &self.worker else {
            return;
        };
        let report = sink::report(&worker.distinct_id, &worker.base, &event, now());
        queue::append(&worker.pending, &report);
        // A dead thread means reporting is over for this run; the report stays
        // queued for the next launch.
        let _ = worker.wake.send(());
    }

    /// Sends what is pending and waits for the thread. Called on quit and when
    /// the user switches reporting off.
    pub fn stop(self) {
        let Some(worker) = self.worker else {
            return;
        };
        drop(worker.wake);
        let _ = worker.thread.join();
    }
}

/// Sends what the queue holds: right away once a batch has piled up, and
/// otherwise every [`BATCH_WAIT`]. What the sink refuses stays queued.
fn run(wakeups: &mpsc::Receiver<()>, pending: &Path, sink: &dyn Sink) {
    // Whatever an earlier launch could not deliver goes first.
    deliver(pending, sink);
    loop {
        match wakeups.recv_timeout(BATCH_WAIT) {
            Ok(()) if queue::len(pending) < BATCH_SIZE => continue,
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        deliver(pending, sink);
    }
    deliver(pending, sink);
}

fn deliver(pending: &Path, sink: &dyn Sink) {
    let batch = queue::peek(pending);
    if batch.is_empty() {
        return;
    }
    if sink.deliver(&batch) {
        queue::remove_front(pending, batch.len());
    }
}

/// Writes a panic straight to the queue: the sending thread may not outlive
/// the panic, but the next launch will pick this up.
pub fn record_panic(pending: &Path, distinct_id: &str, channel: &str, file: &str, line: u32) {
    let event = Event::Panicked {
        location: sink::panic_location(file, line),
    };
    let report = sink::report(distinct_id, &sink::base_props(channel), &event, now());
    queue::append(pending, &report);
}

/// Unix seconds; a clock before the epoch reports as 0.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

/// The platform cache-dir location of the queue file.
pub fn default_queue_file() -> Option<PathBuf> {
    Some(dirs::cache_dir()?.join("filegram").join("pending.jsonl"))
}

/// The platform config-dir location of the install id.
pub fn default_device_file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("filegram").join("device"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    #[derive(Default)]
    struct Recorder {
        delivered: Arc<Mutex<Vec<Value>>>,
        offline: bool,
    }

    impl Sink for Recorder {
        fn deliver(&self, batch: &[Value]) -> bool {
            if self.offline {
                return false;
            }
            self.delivered.lock().unwrap().extend(batch.iter().cloned());
            true
        }
    }

    fn started(dir: &tempfile::TempDir, offline: bool) -> (Telemetry, Arc<Mutex<Vec<Value>>>) {
        let delivered = Arc::new(Mutex::new(Vec::new()));
        let sink = Recorder {
            delivered: delivered.clone(),
            offline,
        };
        let telemetry = Telemetry::start(
            "abc".to_string(),
            "direct",
            dir.path().join("pending.jsonl"),
            Box::new(sink),
        );
        (telemetry, delivered)
    }

    #[test]
    fn a_disabled_telemetry_reports_nothing_and_touches_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let telemetry = Telemetry::disabled();
        telemetry.track(Event::UpdateNoticed);
        telemetry.stop();
        assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    }

    #[test]
    fn a_tracked_event_is_on_disk_before_it_is_ever_sent() {
        let dir = tempfile::tempdir().unwrap();
        let pending = dir.path().join("pending.jsonl");
        let (telemetry, _) = started(&dir, true);
        telemetry.track(Event::UpdateNoticed);
        // Nothing was asked to send yet; a kill right here must not lose it.
        assert_eq!(queue::peek(&pending).len(), 1);
        telemetry.stop();
    }

    #[test]
    fn tracked_events_reach_the_sink_on_stop() {
        let dir = tempfile::tempdir().unwrap();
        let (telemetry, delivered) = started(&dir, false);
        telemetry.track(Event::UpdateNoticed);
        telemetry.track(Event::UpdateOpened);
        telemetry.stop();
        let delivered = delivered.lock().unwrap();
        assert_eq!(delivered.len(), 2);
        assert_eq!(delivered[0]["event"], "update_noticed");
        assert_eq!(delivered[0]["properties"]["distinct_id"], "abc");
        assert_eq!(delivered[1]["event"], "update_opened");
    }

    #[test]
    fn an_undelivered_batch_waits_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let pending = dir.path().join("pending.jsonl");
        let (telemetry, _) = started(&dir, true);
        telemetry.track(Event::UpdateNoticed);
        telemetry.stop();
        let waiting = queue::peek(&pending);
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0]["event"], "update_noticed");
    }

    #[test]
    fn reports_left_by_an_offline_launch_go_out_on_the_next_one() {
        let dir = tempfile::tempdir().unwrap();
        let pending = dir.path().join("pending.jsonl");
        queue::append(&pending, &json!({"event": "from_yesterday"}));
        let (telemetry, delivered) = started(&dir, false);
        telemetry.stop();
        assert!(queue::peek(&pending).is_empty());
        let delivered = delivered.lock().unwrap();
        assert_eq!(delivered.len(), 1);
        assert_eq!(delivered[0]["event"], "from_yesterday");
    }

    #[test]
    fn a_panic_is_recorded_for_the_next_launch_to_send() {
        let dir = tempfile::tempdir().unwrap();
        let pending = dir.path().join("pending.jsonl");
        record_panic(&pending, "abc", "flatpak", "main.rs", 42);
        let waiting = queue::peek(&pending);
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0]["event"], "panicked");
        assert_eq!(waiting[0]["properties"]["location"], "main.rs:42");
        assert_eq!(waiting[0]["properties"]["channel"], "flatpak");
    }
}
