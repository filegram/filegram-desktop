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
    /// Records a scan start: the path is trimmed and moves to the front,
    /// duplicates are removed, the list is capped at [`MAX_ENTRIES`].
    /// Blank paths are ignored.
    pub fn push(&mut self, path: &str) {
        let path = path.trim();
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

    /// Reads the history file; a missing or unreadable file yields an empty
    /// history.
    pub fn load(path: &Path) -> Self {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        let mut history = Self::default();
        // Pushing in reverse order keeps the file's first line most recent
        // and reapplies the dedupe/cap invariants to hand-edited files.
        for line in text.lines().rev() {
            history.push(line);
        }
        history
    }

    /// Writes the history file, creating parent directories as needed.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, self.entries.join("\n"))
    }
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

        assert_eq!(History::load(&file), history);
    }

    #[test]
    fn load_missing_file_yields_empty_history() {
        let dir = tempfile::tempdir().unwrap();
        let history = History::load(&dir.path().join("absent.txt"));
        assert_eq!(history, History::default());
    }

    #[test]
    fn load_skips_blank_lines_and_recaps() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("history.txt");
        let lines: Vec<String> = (0..(MAX_ENTRIES + 3))
            .map(|i| format!("/dir{i}"))
            .collect();
        std::fs::write(&file, format!("{}\n\n  \n", lines.join("\n"))).unwrap();

        let history = History::load(&file);
        assert_eq!(history.entries().len(), MAX_ENTRIES);
        assert_eq!(history.latest(), Some("/dir0"));
    }
}
