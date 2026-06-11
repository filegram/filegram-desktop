//! The scan-start history: the last [`MAX_ENTRIES`] unique paths,
//! most recent first, persisted as a plain text file (one path per line).

use std::io;
use std::path::{Path, PathBuf};

/// How many unique paths the history keeps.
pub const MAX_ENTRIES: usize = 10;

#[derive(Debug, Default, PartialEq)]
pub struct History {
    entries: Vec<String>,
}

impl History {
    /// Records a scan start: the path is normalized (see [`normalize`]) and
    /// moves to the front, duplicates are removed, the list is capped at
    /// [`MAX_ENTRIES`]. Blank paths and paths with line breaks are ignored.
    pub fn push(&mut self, path: &str) {
        let path = normalize(path);
        if path.is_empty() {
            return;
        }
        self.entries.retain(|entry| entry != path);
        self.entries.insert(0, path.to_string());
        self.entries.truncate(MAX_ENTRIES);
    }

    /// The most recent path, if any.
    pub fn latest(&self) -> Option<&str> {
        self.entries.first().map(String::as_str)
    }

    /// The paths, most recent first.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Reads the history file; a missing file yields an empty history.
    /// Any other read error is returned so the caller can avoid saving
    /// over a file it could not read.
    pub fn load(path: &Path) -> io::Result<Self> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error),
        };
        let mut history = Self::default();
        // Pushing in reverse order keeps the file's first line most recent
        // and reapplies the dedupe/cap invariants to hand-edited files.
        for line in text.lines().rev() {
            history.push(line);
        }
        Ok(history)
    }

    /// Writes the history file, creating parent directories as needed.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.entries.join("\n"))
    }
}

/// Trims whitespace and trailing separators (`/tmp/` and `/tmp` are the same
/// directory), keeping roots like `/` or `C:\` intact. Paths with line breaks
/// normalize to blank: they cannot round-trip through the one-path-per-line
/// file format.
fn normalize(path: &str) -> &str {
    let path = path.trim();
    if path.contains(['\n', '\r']) {
        return "";
    }
    let stripped = path.trim_end_matches(['/', '\\']);
    if stripped.is_empty() || is_drive_letter(stripped) {
        path
    } else {
        stripped
    }
}

/// `C:` — a bare Windows drive letter. `C:\` must keep its separator:
/// without it the path is drive-relative, a different location. On Unix
/// `:` is an ordinary filename character, so only the exact two-character
/// ASCII-letter form counts.
fn is_drive_letter(path: &str) -> bool {
    let mut chars = path.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(':'), None) if letter.is_ascii_alphabetic()
    )
}

/// The platform config-dir location of the history file.
pub fn default_file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("filegram").join("history.txt"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_keeps_most_recent_first() {
        let mut history = History::default();
        history.push("/a");
        history.push("/b");
        assert_eq!(history.entries(), ["/b", "/a"]);
        assert_eq!(history.latest(), Some("/b"));
    }

    #[test]
    fn push_moves_duplicate_to_front() {
        let mut history = History::default();
        history.push("/a");
        history.push("/b");
        history.push("/a");
        assert_eq!(history.entries(), ["/a", "/b"]);
    }

    #[test]
    fn push_caps_at_max_entries() {
        let mut history = History::default();
        for i in 0..(MAX_ENTRIES + 5) {
            history.push(&format!("/dir{i}"));
        }
        assert_eq!(history.entries().len(), MAX_ENTRIES);
        assert_eq!(history.latest(), Some("/dir14"));
        // The oldest entries fell off.
        assert!(!history.entries().contains(&"/dir0".to_string()));
    }

    #[test]
    fn push_ignores_blank_paths() {
        let mut history = History::default();
        history.push("");
        history.push("   ");
        assert_eq!(history.latest(), None);
    }

    #[test]
    fn push_trims_and_dedupes_whitespace_variants() {
        let mut history = History::default();
        history.push("/tmp");
        history.push("/tmp ");
        assert_eq!(history.entries(), ["/tmp"]);
        history.push(" /var ");
        assert_eq!(history.latest(), Some("/var"));
    }

    #[test]
    fn push_dedupes_trailing_separator_variants() {
        let mut history = History::default();
        history.push("/tmp");
        history.push("/tmp/");
        history.push("/tmp//");
        assert_eq!(history.entries(), ["/tmp"]);
    }

    #[test]
    fn push_keeps_root_paths_intact() {
        let mut history = History::default();
        history.push("/");
        history.push(r"C:\");
        assert_eq!(history.entries(), [r"C:\", "/"]);
    }

    #[test]
    fn push_strips_separators_after_non_drive_colons() {
        // On Unix `:` is an ordinary filename character — only a bare
        // Windows drive letter keeps its trailing separator.
        let mut history = History::default();
        history.push("/tmp:");
        history.push("/tmp:/");
        assert_eq!(history.entries(), ["/tmp:"]);
    }

    #[test]
    fn push_rejects_paths_with_line_breaks() {
        let mut history = History::default();
        history.push("/tmp/a\nb");
        history.push("/tmp/a\rb");
        assert_eq!(history.latest(), None);
    }

    #[test]
    fn empty_history_has_no_latest() {
        assert_eq!(History::default().latest(), None);
    }

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        // The parent directory does not exist yet — save must create it.
        let file = dir.path().join("nested").join("history.txt");

        let mut history = History::default();
        history.push("/a");
        history.push("/b");
        history.save(&file).unwrap();

        assert_eq!(History::load(&file).unwrap(), history);
    }

    #[test]
    fn load_missing_file_yields_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::load(&dir.path().join("absent.txt")).unwrap();
        assert_eq!(history, History::default());
    }

    #[test]
    fn load_unreadable_file_errors() {
        // A directory cannot be read as a file — the error must surface
        // instead of being swallowed into an empty (clobber-prone) history.
        let dir = tempfile::tempdir().unwrap();
        assert!(History::load(dir.path()).is_err());
    }

    #[test]
    fn load_skips_blank_lines_and_recaps() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("history.txt");
        let lines: Vec<String> = (0..(MAX_ENTRIES + 3))
            .map(|i| format!("/dir{i}"))
            .collect();
        std::fs::write(&file, format!("{}\n\n  \n", lines.join("\n"))).unwrap();

        let history = History::load(&file).unwrap();
        assert_eq!(history.entries().len(), MAX_ENTRIES);
        assert_eq!(history.latest(), Some("/dir0"));
    }
}
