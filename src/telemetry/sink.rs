//! Turning events into PostHog reports and getting them there.

use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::telemetry::event::Event;

/// Write-only project key, baked in at build time from `FILEGRAM_POSTHOG_KEY`.
/// Builds without it report nothing, which keeps development runs and forks
/// out of the project's numbers.
const API_KEY: Option<&str> = option_env!("FILEGRAM_POSTHOG_KEY");
const ENDPOINT: &str = "https://eu.i.posthog.com/batch/";
const TIMEOUT: Duration = Duration::from_secs(10);

/// Where finished batches go. Tests substitute their own.
pub trait Sink: Send {
    /// `false` puts the batch back on disk for the next launch.
    fn deliver(&self, batch: &[Value]) -> bool;
}

pub struct PostHog;

/// Whether this build can report at all.
pub fn configured() -> bool {
    API_KEY.is_some()
}

impl Sink for PostHog {
    fn deliver(&self, batch: &[Value]) -> bool {
        let Some(key) = API_KEY else {
            return false;
        };
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .build()
            .into();
        agent
            .post(ENDPOINT)
            .send_json(batch_body(key, batch))
            .is_ok()
    }
}

/// Properties shared by every report of this launch.
pub fn base_props(channel: &str) -> Map<String, Value> {
    let mut props = Map::new();
    props.insert("version".into(), json!(env!("CARGO_PKG_VERSION")));
    props.insert("os".into(), json!(std::env::consts::OS));
    props.insert("arch".into(), json!(std::env::consts::ARCH));
    props.insert("channel".into(), json!(channel));
    props
}

/// One PostHog event. `at` is unix seconds.
pub fn report(distinct_id: &str, base: &Map<String, Value>, event: &Event, at: u64) -> Value {
    let mut properties = base.clone();
    properties.insert("distinct_id".into(), json!(distinct_id));
    properties.extend(event.props());
    json!({
        "event": event.name(),
        "properties": properties,
        "timestamp": at,
    })
}

pub fn batch_body(api_key: &str, batch: &[Value]) -> Value {
    json!({ "api_key": api_key, "batch": batch })
}

/// `file:line` with the directories dropped, so a build path never ships.
pub fn panic_location(file: &str, line: u32) -> String {
    let name = file.rsplit(['/', '\\']).next().unwrap_or(file);
    format!("{name}:{line}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::event::{Event, looks_personal};

    #[test]
    fn a_report_carries_the_install_id_the_event_and_its_props() {
        let report = report(
            "abc",
            &base_props("direct"),
            &Event::UpdateOpened,
            1_700_000_000,
        );
        assert_eq!(report["event"], "update_opened");
        assert_eq!(report["properties"]["distinct_id"], "abc");
        assert_eq!(report["properties"]["channel"], "direct");
        assert_eq!(report["timestamp"], 1_700_000_000_u64);
    }

    #[test]
    fn event_props_join_the_shared_ones() {
        let report = report(
            "abc",
            &base_props("snap"),
            &Event::ScanCancelled { seconds: 3 },
            0,
        );
        assert_eq!(report["properties"]["duration"], "1-5s");
        assert_eq!(report["properties"]["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn shared_props_name_the_build_never_the_machine() {
        let base = base_props("flatpak");
        assert_eq!(base["os"], std::env::consts::OS);
        assert_eq!(base["arch"], std::env::consts::ARCH);
        for (key, value) in &base {
            let Some(text) = value.as_str() else { continue };
            assert!(!text.contains(['/', '\\', '~']), "{key} leaks {text}");
        }
    }

    #[test]
    fn the_batch_body_is_what_posthog_expects() {
        let batch = vec![report(
            "abc",
            &base_props("direct"),
            &Event::UpdateNoticed,
            0,
        )];
        let body = batch_body("phc_key", &batch);
        assert_eq!(body["api_key"], "phc_key");
        assert_eq!(body["batch"].as_array().unwrap().len(), 1);
        assert_eq!(body["batch"][0]["event"], "update_noticed");
    }

    #[test]
    fn a_reported_panic_location_stays_impersonal() {
        let report = report(
            "abc",
            &base_props("direct"),
            &Event::Panicked {
                location: panic_location("/home/alice/src/main.rs", 42),
            },
            0,
        );
        let location = report["properties"]["location"].as_str().unwrap();
        assert_eq!(location, "main.rs:42");
        assert!(!looks_personal(location));
    }

    #[test]
    fn a_windows_source_path_loses_its_directories_too() {
        assert_eq!(panic_location("C:\\src\\ui\\start.rs", 7), "start.rs:7");
    }
}
