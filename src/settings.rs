//! The persisted settings: `key=value` lines (`theme=dark`, `lang=ru-RU`)
//! in the platform config dir. A missing file or key means "follow the
//! system". The pre-0.3 format — a bare `light` / `dark` word — still
//! reads as the theme.

use std::io;
use std::path::{Path, PathBuf};

use iced::theme::Mode;

use crate::i18n::Lang;

/// The saved manual choices; `None` follows the system.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Settings {
    pub theme: Option<Mode>,
    pub lang: Option<Lang>,
}

/// Reads the saved choices. A missing file, an unknown key or an
/// unrecognized value yields the system-following default; any other
/// read error is returned.
pub fn load(path: &Path) -> io::Result<Settings> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(error) => return Err(error),
    };
    let mut settings = Settings::default();
    for line in text.lines() {
        // The legacy one-word file carried the theme alone.
        let (key, value) = line.split_once('=').unwrap_or(("theme", line));
        match (key.trim(), value.trim()) {
            ("theme", "light") => settings.theme = Some(Mode::Light),
            ("theme", "dark") => settings.theme = Some(Mode::Dark),
            // Like the theme, keep a previously parsed language when a later
            // `lang=` line carries an unrecognized tag.
            ("lang", tag) => {
                if let Some(lang) = Lang::from_tag(tag) {
                    settings.lang = Some(lang);
                }
            }
            _ => {}
        }
    }
    Ok(settings)
}

/// Writes the choices, creating parent directories as needed; a `None`
/// field is omitted, so it keeps following the system on the next launch.
/// `Mode::None` is never a manual choice; it falls back to `light`
/// defensively.
pub fn save(path: &Path, settings: Settings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = String::new();
    if let Some(mode) = settings.theme {
        let word = match mode {
            Mode::Dark => "dark",
            Mode::Light | Mode::None => "light",
        };
        text.push_str("theme=");
        text.push_str(word);
        text.push('\n');
    }
    if let Some(lang) = settings.lang {
        text.push_str("lang=");
        text.push_str(lang.tag());
        text.push('\n');
    }
    std::fs::write(path, text)
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
        let cases = [
            Settings {
                theme: Some(Mode::Dark),
                lang: None,
            },
            Settings {
                theme: None,
                lang: Some(Lang::RuRu),
            },
            Settings {
                theme: Some(Mode::Light),
                lang: Some(Lang::Es419),
            },
            Settings::default(),
        ];
        for settings in cases {
            save(&file, settings).unwrap();
            assert_eq!(load(&file).unwrap(), settings);
        }
    }

    #[test]
    fn load_missing_file_follows_system() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            load(&dir.path().join("absent.txt")).unwrap(),
            Settings::default()
        );
    }

    #[test]
    fn load_legacy_one_word_file_reads_as_theme() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.cfg");
        std::fs::write(&file, "dark").unwrap();
        assert_eq!(
            load(&file).unwrap(),
            Settings {
                theme: Some(Mode::Dark),
                lang: None,
            }
        );
    }

    #[test]
    fn load_unrecognized_content_follows_system() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.cfg");
        std::fs::write(&file, "solarized\nlang=tlh\ncolor=red\n").unwrap();
        assert_eq!(load(&file).unwrap(), Settings::default());
    }

    #[test]
    fn load_unreadable_file_errors() {
        // A directory cannot be read as a file — the error must surface.
        let dir = tempfile::tempdir().unwrap();
        assert!(load(dir.path()).is_err());
    }
}
