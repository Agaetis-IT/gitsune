//! A log of the git commands the app ran, so the user can always see what was done.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// Entries kept per repository; the oldest are dropped first.
const MAX_ENTRIES: usize = 500;
/// Bytes of output kept per entry.
const MAX_OUTPUT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: u64,
    /// Milliseconds since the Unix epoch, when the command started.
    pub started_at: u64,
    /// The command as it would be typed, e.g. `git commit -F -`.
    pub command: String,
    pub duration_ms: u64,
    pub exit_code: i32,
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
    /// The output was cut at 16 KiB.
    pub truncated: bool,
}

type Notifier = Arc<dyn Fn() + Send + Sync>;

/// Recent commands of one repository. Cheap to share between threads.
#[derive(Default)]
pub struct ActivityLog {
    entries: Mutex<VecDeque<ActivityEntry>>,
    next_id: AtomicU64,
    notifier: Mutex<Option<Notifier>>,
}

impl ActivityLog {
    /// Calls `notify` (from the thread that ran the command) after every new entry.
    pub fn set_notifier(&self, notify: impl Fn() + Send + Sync + 'static) {
        *self.notifier.lock().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(notify));
    }

    pub fn record(
        &self,
        command: String,
        started: SystemTime,
        duration_ms: u64,
        exit_code: i32,
        stdout: &[u8],
        stderr: &str,
    ) {
        let (out, out_cut) = clip(&String::from_utf8_lossy(stdout));
        let (err, err_cut) = clip(stderr);
        let entry = ActivityEntry {
            id: self.next_id.fetch_add(1, Ordering::SeqCst) + 1,
            started_at: started
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_millis() as u64),
            command,
            duration_ms,
            exit_code,
            ok: exit_code == 0,
            stdout: out,
            stderr: err,
            truncated: out_cut || err_cut,
        };
        {
            let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
            entries.push_back(entry);
            while entries.len() > MAX_ENTRIES {
                entries.pop_front();
            }
        }
        let notifier = self
            .notifier
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(notify) = notifier {
            notify();
        }
    }

    /// The entries, oldest first.
    pub fn list(&self) -> Vec<ActivityEntry> {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }
}

fn clip(text: &str) -> (String, bool) {
    if text.len() <= MAX_OUTPUT_BYTES {
        return (text.to_string(), false);
    }
    let mut end = MAX_OUTPUT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), true)
}
