//! Latest-release check against GitHub.
//! A single fire-and-forget request on its own thread right after launch;
//! the UI never waits for it — the result arrives as a message when (and
//! if) the response comes back, and any failure just leaves the footer
//! showing the running version alone.

use std::time::Duration;

use iced::futures::channel::oneshot;

/// The release page the footer link opens.
pub const LATEST_RELEASE_URL: &str =
    "https://github.com/filegram/filegram-desktop/releases/latest";

const API_URL: &str = "https://api.github.com/repos/filegram/filegram-desktop/releases/latest";

/// Resolves to the latest release tag (e.g. `v0.2.2`), `None` on any
/// network or parse failure. The blocking request runs on a dedicated
/// thread, so the future never occupies the executor.
pub fn fetch_latest_tag() -> impl Future<Output = Option<String>> {
    let (tx, rx) = oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(request_latest_tag());
    });
    async move { rx.await.ok().flatten() }
}

fn request_latest_tag() -> Option<String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .into();
    let mut response = agent
        .get(API_URL)
        // GitHub rejects requests without a User-Agent.
        .header("User-Agent", concat!("filegram/", env!("CARGO_PKG_VERSION")))
        .call()
        .ok()?;
    let body = response.body_mut().read_to_string().ok()?;
    parse_tag_name(&body)
}

/// Pulls `tag_name` out of the API response without a JSON dependency:
/// release tags are plain `vX.Y.Z` strings, never escaped.
fn parse_tag_name(json: &str) -> Option<String> {
    let key = "\"tag_name\"";
    let rest = &json[json.find(key)? + key.len()..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let value = rest.strip_prefix('"')?;
    let end = value.find('"')?;
    (end > 0).then(|| value[..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tag_name_from_api_response() {
        let json = r#"{"url":"https://api.github.com/...","tag_name":"v0.2.2","name":"0.2.2"}"#;
        assert_eq!(parse_tag_name(json), Some("v0.2.2".to_string()));
    }

    #[test]
    fn parses_tag_name_with_spacing() {
        assert_eq!(
            parse_tag_name("{ \"tag_name\" : \"v1.0.0\" }"),
            Some("v1.0.0".to_string())
        );
    }

    #[test]
    fn rejects_response_without_tag_name() {
        assert_eq!(parse_tag_name(r#"{"message":"Not Found"}"#), None);
        assert_eq!(parse_tag_name(""), None);
    }

    #[test]
    fn rejects_empty_tag_name() {
        assert_eq!(parse_tag_name(r#"{"tag_name":""}"#), None);
    }
}
