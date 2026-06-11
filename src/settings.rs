//! The persisted theme choice: a one-word file (`light` / `dark`) in the
//! platform config dir. An absent file means "follow the system".

use std::io;
use std::path::{Path, PathBuf};

use iced::theme::Mode;

/// Reads the saved choice. A missing file or unrecognized content yields
/// `None` (follow the system); any other read error is returned.
pub fn load(path: &Path) -> io::Result<Option<Mode>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(match text.trim() {
        "light" => Some(Mode::Light),
        "dark" => Some(Mode::Dark),
        _ => None,
    })
}

/// Writes the choice, creating parent directories as needed. `Mode::None`
/// is never a manual choice; it falls back to `light` defensively.
pub fn save(path: &Path, mode: Mode) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let word = match mode {
        Mode::Dark => "dark",
        Mode::Light | Mode::None => "light",
    };
    std::fs::write(path, word)
}

/// The platform config-dir location of the settings file.
pub fn default_file() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("filegram").join("settings.cfg"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        // The parent directory does not exist yet — save must create it.
        let file = dir.path().join("nested").join("settings.cfg");
        for mode in [Mode::Dark, Mode::Light] {
            save(&file, mode).unwrap();
            assert_eq!(load(&file).unwrap(), Some(mode));
        }
    }

    #[test]
    fn load_missing_file_follows_system() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load(&dir.path().join("absent.txt")).unwrap(), None);
    }

    #[test]
    fn load_unrecognized_content_follows_system() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.cfg");
        std::fs::write(&file, "solarized\n").unwrap();
        assert_eq!(load(&file).unwrap(), None);
    }

    #[test]
    fn load_unreadable_file_errors() {
        // A directory cannot be read as a file — the error must surface.
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).is_err());
    }
}
