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
    // Without these two PostHog turns the sender's IP into a city, a postcode
    // and coordinates, and stores them on the event.
    props.insert("$ip".into(), Value::Null);
    props.insert("$geoip_disable".into(), json!(true));
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
        "timestamp": iso8601(at),
    })
}

/// `at` (unix seconds) as `YYYY-MM-DDTHH:MM:SSZ`. PostHog rejects a batch
/// whose timestamp is a bare number, so this is not cosmetic.
pub fn iso8601(at: u64) -> String {
    let (days, seconds) = (at / 86_400, at % 86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute) = (seconds / 3_600, seconds % 3_600 / 60);
    format!(
        "{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{:02}Z",
        seconds % 60
    )
}

/// Howard Hinnant's days-to-civil algorithm, shifted to an epoch at
/// 0000-03-01 so leap years need no special case.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let days = days + 719_468;
    let era = days / 146_097;
    let day_of_era = days % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
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
    fn geoip_enrichment_is_switched_off_on_every_report() {
        // PostHog resolves the sender's IP into city, postcode and coordinates
        // unless the report opts out, which is finer than we ever promised.
        let report = report("abc", &base_props("direct"), &Event::UpdateNoticed, 0);
        assert_eq!(report["properties"]["$ip"], Value::Null);
        assert_eq!(report["properties"]["$geoip_disable"], true);
    }

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
        assert_eq!(report["timestamp"], iso8601(1_700_000_000));
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
    fn the_timestamp_is_iso_8601_which_is_all_posthog_accepts() {
        // 2026-08-20T12:34:56Z
        assert_eq!(iso8601(1_787_229_296), "2026-08-20T12:34:56Z");
    }

    #[test]
    fn the_epoch_and_a_leap_day_both_format() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn a_report_is_stamped_in_iso_8601() {
        let report = report("abc", &base_props("direct"), &Event::UpdateNoticed, 0);
        assert_eq!(report["timestamp"], "1970-01-01T00:00:00Z");
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
