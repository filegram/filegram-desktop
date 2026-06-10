//! Human-readable sizes and path shortening (a port of the original's StringUtils).

/// Size in binary units (divisor 1024), one digit after the decimal point.
/// Below 1024 bytes — a whole number with a `B` suffix.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = UNITS[0];
    for next in &UNITS[1..] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

/// Shortens the path to `max_chars` characters by successively replacing middle
/// segments with `..` (like `hideFolderNameInPath` in the original).
/// The last segment is never replaced.
pub fn shorten_path(path: &str, max_chars: usize) -> String {
    // Windows paths are displayed with `\` — detect the separator from the input.
    let separator = if path.contains('\\') && !path.contains('/') {
        '\\'
    } else {
        '/'
    };
    let mut segments: Vec<&str> = path.split(separator).collect();
    let len = |segments: &[&str]| {
        segments.iter().map(|s| s.chars().count()).sum::<usize>()
            + segments.len().saturating_sub(1)
    };
    while len(&segments) > max_chars && segments.len() > 2 {
        let last = segments.len() - 1;
        // Candidates are middle segments only: the first (root/drive)
        // and the last one are never replaced.
        let Some(victim) = segments[1..last]
            .iter()
            .position(|s| !s.is_empty() && *s != "..")
        else {
            break;
        };
        segments[victim + 1] = "..";
    }
    segments.join(&separator.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_bytes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
    }

    #[test]
    fn human_size_binary_units() {
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(human_size(1024_u64.pow(4)), "1.0 TB");
        assert_eq!(human_size(2_621_440), "2.5 MB");
    }

    #[test]
    fn shorten_path_fits_unchanged() {
        assert_eq!(shorten_path("/a/b/c/d", 20), "/a/b/c/d");
    }

    #[test]
    fn shorten_path_replaces_middle_segments() {
        assert_eq!(
            shorten_path("/home/user/projects/filegram", 20),
            "/../../../filegram"
        );
    }

    #[test]
    fn shorten_path_keeps_last_segment() {
        // Even when it does not fit, the last segment is left untouched.
        assert_eq!(shorten_path("/home/user/filegram", 5), "/../../filegram");
    }

    #[test]
    fn shorten_path_windows_separators() {
        assert_eq!(
            shorten_path(r"C:\Users\stan\projects\filegram", 22),
            r"C:\..\..\..\filegram"
        );
    }

    #[test]
    fn shorten_path_never_replaces_first_segment() {
        // The first segment (root/drive) is never replaced even when space runs out.
        assert_eq!(
            shorten_path("home/user/projects/filegram", 1),
            "home/../../filegram"
        );
        assert_eq!(shorten_path(r"C:\Users\filegram", 1), r"C:\..\filegram");
    }
}
