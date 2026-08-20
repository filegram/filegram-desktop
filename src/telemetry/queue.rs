//! Reports waiting for a network, kept as JSON lines on disk.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use fs4::FileExt;
use serde_json::Value;

/// How many reports survive an offline stretch; older ones are dropped.
pub const MAX_PENDING: usize = 200;

/// Appends one report. Every failure is silent: reporting must never get in
/// the way of the app.
pub fn append(path: &Path, report: &Value) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
    else {
        return;
    };
    // The sending thread drains the same file; the lock keeps a report from
    // slipping in between its read and its truncate.
    if FileExt::lock(&file).is_err() {
        return;
    }
    let _ = file.seek(SeekFrom::End(0));
    let _ = writeln!(file, "{report}");
    trim(&mut file);
    let _ = FileExt::unlock(&file);
}

/// Drops the first `count` reports, keeping whatever was appended while they
/// were being sent. Removing only what was delivered means a process killed
/// mid-send loses nothing.
pub fn remove_front(path: &Path, count: usize) {
    let Ok(mut file) = OpenOptions::new().read(true).write(true).open(path) else {
        return;
    };
    if FileExt::lock(&file).is_err() {
        return;
    }
    let reports = parse(&read(&mut file));
    let kept: String = reports
        .iter()
        .skip(count)
        .map(|report| format!("{report}\n"))
        .collect();
    let _ = file.set_len(0);
    let _ = file.seek(SeekFrom::Start(0));
    let _ = file.write_all(kept.as_bytes());
    let _ = FileExt::unlock(&file);
}

/// Everything pending, left in place. Used by `--telemetry-dump`.
pub fn peek(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .map(|text| parse(&text))
        .unwrap_or_default()
}

/// How many reports are waiting.
pub fn len(path: &Path) -> usize {
    peek(path).len()
}

fn parse(text: &str) -> Vec<Value> {
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn read(file: &mut File) -> String {
    let mut text = String::new();
    if file.seek(SeekFrom::Start(0)).is_err() || file.read_to_string(&mut text).is_err() {
        return String::new();
    }
    text
}

/// Called with the lock held.
fn trim(file: &mut File) {
    let reports = parse(&read(file));
    if reports.len() <= MAX_PENDING {
        return;
    }
    let kept: String = reports[reports.len() - MAX_PENDING..]
        .iter()
        .map(|report| format!("{report}\n"))
        .collect();
    let _ = file.set_len(0);
    let _ = file.seek(SeekFrom::Start(0));
    let _ = file.write_all(kept.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn queue_file(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("nested").join("pending.jsonl")
    }

    #[test]
    fn appended_reports_come_back_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let file = queue_file(&dir);
        append(&file, &json!({"event": "first"}));
        append(&file, &json!({"event": "second"}));
        let waiting = peek(&file);
        assert_eq!(waiting.len(), 2);
        assert_eq!(waiting[0]["event"], "first");
        assert_eq!(waiting[1]["event"], "second");
    }

    #[test]
    fn peeking_leaves_the_reports_where_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let file = queue_file(&dir);
        append(&file, &json!({"event": "first"}));
        assert_eq!(peek(&file).len(), 1);
        assert_eq!(peek(&file).len(), 1);
    }

    #[test]
    fn only_the_reports_that_went_out_are_removed() {
        let dir = tempfile::tempdir().unwrap();
        let file = queue_file(&dir);
        append(&file, &json!({"event": "sent"}));
        append(&file, &json!({"event": "arrived_mid_send"}));
        remove_front(&file, 1);
        let left = peek(&file);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0]["event"], "arrived_mid_send");
    }

    #[test]
    fn removing_more_than_there_is_empties_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = queue_file(&dir);
        append(&file, &json!({"event": "only"}));
        remove_front(&file, 5);
        assert!(peek(&file).is_empty());
    }

    #[test]
    fn a_missing_file_holds_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(peek(&dir.path().join("absent.jsonl")).is_empty());
    }

    #[test]
    fn a_truncated_line_is_skipped_and_the_rest_survives() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pending.jsonl");
        std::fs::write(&file, "{\"event\": \"good\"}\n{\"event\": \"tru\n").unwrap();
        let waiting = peek(&file);
        assert_eq!(waiting.len(), 1);
        assert_eq!(waiting[0]["event"], "good");
    }

    #[test]
    fn an_offline_run_drops_the_oldest_reports_instead_of_growing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("pending.jsonl");
        for i in 0..MAX_PENDING * 2 {
            append(&file, &json!({"event": "scan_started", "i": i}));
        }
        let waiting = peek(&file);
        assert!(waiting.len() <= MAX_PENDING, "kept {}", waiting.len());
        assert_eq!(waiting.last().unwrap()["i"], MAX_PENDING * 2 - 1);
    }
}
