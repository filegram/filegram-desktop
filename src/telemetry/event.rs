//! What gets reported, and the buckets that keep it from identifying anyone.

use serde_json::{Map, Value, json};

/// Where a scan was started from. The path itself never leaves the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    Home,
    Downloads,
    Desktop,
    Documents,
    Disk,
    Recent,
    Typed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileAction {
    Open,
    Reveal,
    Trash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    AppStarted {
        first_launch: bool,
    },
    /// Sent once per run, carrying the counters kept over the session; single
    /// clicks are never reported on their own.
    SessionEnded {
        seconds: u64,
        scans: u32,
        zoom_ins: u32,
        go_ups: u32,
    },
    ScanStarted {
        root: RootKind,
    },
    ScanFinished {
        seconds: u64,
        files: u64,
        bytes: u64,
    },
    ScanCancelled {
        seconds: u64,
    },
    FileAction {
        action: FileAction,
        ok: bool,
    },
    SettingChanged {
        key: &'static str,
        value: String,
    },
    UpdateNoticed,
    UpdateOpened,
    /// `location` is `file.rs:line` with the source directories stripped; the
    /// panic message is dropped, since paths reach it easily.
    Panicked {
        location: String,
    },
    /// The last thing sent, right before reporting is switched off.
    TelemetryDisabled,
}

impl RootKind {
    fn name(self) -> &'static str {
        match self {
            RootKind::Home => "home",
            RootKind::Downloads => "downloads",
            RootKind::Desktop => "desktop",
            RootKind::Documents => "documents",
            RootKind::Disk => "disk",
            RootKind::Recent => "recent",
            RootKind::Typed => "typed",
        }
    }
}

impl FileAction {
    fn name(self) -> &'static str {
        match self {
            FileAction::Open => "open",
            FileAction::Reveal => "reveal",
            FileAction::Trash => "trash",
        }
    }
}

impl Event {
    /// Reported as-is; renaming one splits its history in two.
    pub fn name(&self) -> &'static str {
        match self {
            Event::AppStarted { .. } => "app_started",
            Event::SessionEnded { .. } => "session_ended",
            Event::ScanStarted { .. } => "scan_started",
            Event::ScanFinished { .. } => "scan_finished",
            Event::ScanCancelled { .. } => "scan_cancelled",
            Event::FileAction { .. } => "file_action",
            Event::SettingChanged { .. } => "setting_changed",
            Event::UpdateNoticed => "update_noticed",
            Event::UpdateOpened => "update_opened",
            Event::Panicked { .. } => "panicked",
            Event::TelemetryDisabled => "telemetry_disabled",
        }
    }

    pub fn props(&self) -> Map<String, Value> {
        let pairs: Vec<(&str, Value)> = match self {
            Event::AppStarted { first_launch } => vec![("first_launch", json!(first_launch))],
            Event::SessionEnded {
                seconds,
                scans,
                zoom_ins,
                go_ups,
            } => vec![
                ("duration", json!(duration_bucket(*seconds))),
                ("scans", json!(count_bucket(u64::from(*scans)))),
                ("zoom_ins", json!(count_bucket(u64::from(*zoom_ins)))),
                ("go_ups", json!(count_bucket(u64::from(*go_ups)))),
            ],
            Event::ScanStarted { root } => vec![("root", json!(root.name()))],
            Event::ScanFinished {
                seconds,
                files,
                bytes,
            } => vec![
                ("duration", json!(duration_bucket(*seconds))),
                ("files", json!(count_bucket(*files))),
                ("size", json!(size_bucket(*bytes))),
            ],
            Event::ScanCancelled { seconds } => {
                vec![("duration", json!(duration_bucket(*seconds)))]
            }
            Event::FileAction { action, ok } => {
                vec![("action", json!(action.name())), ("ok", json!(ok))]
            }
            Event::SettingChanged { key, value } => {
                vec![("setting", json!(key)), ("value", json!(value))]
            }
            Event::UpdateNoticed | Event::UpdateOpened | Event::TelemetryDisabled => Vec::new(),
            Event::Panicked { location } => vec![("location", json!(location))],
        };
        pairs
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect()
    }
}

/// Guards the reported strings against anything naming a machine or its owner.
/// Separators and spaces cover paths and file names; a dot is allowed only in a
/// panic location, so host names like `alice-macbook.local` stay out.
#[cfg(test)]
pub fn looks_personal(text: &str) -> bool {
    if text.contains(['/', '\\', '~', ' ']) {
        return true;
    }
    text.contains('.') && !is_panic_location(text)
}

#[cfg(test)]
fn is_panic_location(text: &str) -> bool {
    let Some((file, line)) = text.rsplit_once(':') else {
        return false;
    };
    file.ends_with(".rs")
        && !line.is_empty()
        && line.bytes().all(|b| b.is_ascii_digit())
        && file.matches('.').count() == 1
}

fn count_bucket(count: u64) -> &'static str {
    match count {
        0 => "0",
        1..=10 => "1-10",
        11..=100 => "11-100",
        101..=1_000 => "101-1k",
        1_001..=10_000 => "1k-10k",
        10_001..=100_000 => "10k-100k",
        100_001..=1_000_000 => "100k-1M",
        _ => "1M+",
    }
}

fn size_bucket(bytes: u64) -> &'static str {
    const GB: u64 = 1_000_000_000;
    match bytes {
        0..=99_999_999 => "<100MB",
        100_000_000..=999_999_999 => "100MB-1GB",
        _ if bytes < 10 * GB => "1-10GB",
        _ if bytes < 100 * GB => "10-100GB",
        _ if bytes < 1_000 * GB => "100GB-1TB",
        _ => "1TB+",
    }
}

fn duration_bucket(seconds: u64) -> &'static str {
    match seconds {
        0 => "<1s",
        1..=4 => "1-5s",
        5..=29 => "5-30s",
        30..=119 => "30s-2m",
        120..=599 => "2-10m",
        _ => "10m+",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_of_each() -> Vec<Event> {
        vec![
            Event::AppStarted { first_launch: true },
            Event::SessionEnded {
                seconds: 91,
                scans: 3,
                zoom_ins: 12,
                go_ups: 4,
            },
            Event::ScanStarted {
                root: RootKind::Typed,
            },
            Event::ScanFinished {
                seconds: 7,
                files: 43_817,
                bytes: 91_000_000_000,
            },
            Event::ScanCancelled { seconds: 2 },
            Event::FileAction {
                action: FileAction::Trash,
                ok: false,
            },
            Event::SettingChanged {
                key: "lang",
                value: "ru-RU".to_string(),
            },
            Event::UpdateNoticed,
            Event::UpdateOpened,
            Event::Panicked {
                location: "main.rs:120".to_string(),
            },
            Event::TelemetryDisabled,
        ]
    }

    /// Fails to compile when a variant is added, as a reminder to extend
    /// [`sample_of_each`] so the checks below cover it.
    #[test]
    fn every_variant_has_a_sample() {
        let names: Vec<&str> = sample_of_each().iter().map(Event::name).collect();
        for event in sample_of_each() {
            let expected = match event {
                Event::AppStarted { .. } => "app_started",
                Event::SessionEnded { .. } => "session_ended",
                Event::ScanStarted { .. } => "scan_started",
                Event::ScanFinished { .. } => "scan_finished",
                Event::ScanCancelled { .. } => "scan_cancelled",
                Event::FileAction { .. } => "file_action",
                Event::SettingChanged { .. } => "setting_changed",
                Event::UpdateNoticed => "update_noticed",
                Event::UpdateOpened => "update_opened",
                Event::Panicked { .. } => "panicked",
                Event::TelemetryDisabled => "telemetry_disabled",
            };
            assert!(names.contains(&expected), "{expected} has no sample");
        }
    }

    #[test]
    fn no_event_carries_anything_that_identifies_a_machine() {
        for event in sample_of_each() {
            for (key, value) in event.props() {
                let Some(text) = value.as_str() else { continue };
                assert!(!looks_personal(text), "{}.{key} leaks {text}", event.name());
            }
        }
    }

    #[test]
    fn paths_and_names_read_as_personal() {
        for text in [
            "/Users/alice/Downloads",
            "C:\\Users\\alice",
            "~/notes.txt",
            "budget 2026.xlsx",
            "alice-macbook.local",
        ] {
            assert!(looks_personal(text), "{text} passed the check");
        }
    }

    #[test]
    fn bucketed_values_read_as_impersonal() {
        for text in [
            "10k-100k",
            "100GB-1TB",
            "5-30s",
            "trash",
            "ru-RU",
            "main.rs:120",
        ] {
            assert!(!looks_personal(text), "{text} was rejected");
        }
    }

    #[test]
    fn file_counts_are_reported_as_ranges() {
        for (files, bucket) in [
            (0, "0"),
            (1, "1-10"),
            (10, "1-10"),
            (11, "11-100"),
            (101, "101-1k"),
            (1_001, "1k-10k"),
            (43_817, "10k-100k"),
            (100_001, "100k-1M"),
            (1_000_001, "1M+"),
        ] {
            assert_eq!(count_bucket(files), bucket, "{files} files");
        }
    }

    #[test]
    fn sizes_are_reported_as_ranges() {
        let gb = 1_000_000_000;
        for (bytes, bucket) in [
            (0, "<100MB"),
            (100_000_000, "100MB-1GB"),
            (gb, "1-10GB"),
            (10 * gb, "10-100GB"),
            (100 * gb, "100GB-1TB"),
            (1_000 * gb, "1TB+"),
        ] {
            assert_eq!(size_bucket(bytes), bucket, "{bytes} bytes");
        }
    }

    #[test]
    fn durations_are_reported_as_ranges() {
        for (seconds, bucket) in [
            (0, "<1s"),
            (1, "1-5s"),
            (5, "5-30s"),
            (30, "30s-2m"),
            (120, "2-10m"),
            (600, "10m+"),
        ] {
            assert_eq!(duration_bucket(seconds), bucket, "{seconds}s");
        }
    }

    #[test]
    fn a_finished_scan_reports_only_buckets() {
        let props = Event::ScanFinished {
            seconds: 7,
            files: 43_817,
            bytes: 91_000_000_000,
        }
        .props();
        assert_eq!(props["files"], "10k-100k");
        assert_eq!(props["size"], "10-100GB");
        assert_eq!(props["duration"], "5-30s");
    }

    #[test]
    fn scan_roots_are_reported_by_kind_never_by_path() {
        assert_eq!(
            Event::ScanStarted {
                root: RootKind::Home,
            }
            .props()["root"],
            "home"
        );
    }
}
