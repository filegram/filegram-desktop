//! The anonymous install id sent with every report.

use std::hash::{BuildHasher, RandomState};
use std::path::Path;

/// Reads the saved id, generating and storing one on first launch. A file that
/// is missing, unreadable or malformed is rewritten. A failed write is not
/// fatal: the launch reports under a fresh id instead of none.
pub fn load_or_create(path: &Path) -> String {
    if let Ok(text) = std::fs::read_to_string(path) {
        let saved = text.trim();
        if is_well_formed(saved) {
            return saved.to_string();
        }
    }
    let id = generate();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, &id);
    id
}

fn is_well_formed(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

/// 128 random bits as hex. `RandomState` seeds itself from the OS, which saves
/// pulling in a random-number crate for the one id this app ever needs.
fn generate() -> String {
    let (high, low) = (RandomState::new(), RandomState::new());
    format!("{:016x}{:016x}", high.hash_one(0u8), low.hash_one(0u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_id_is_thirty_two_hex_digits() {
        let id = generate();
        assert_eq!(id.len(), 32);
        assert!(
            id.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase())
        );
    }

    #[test]
    fn two_generated_ids_differ() {
        assert_ne!(generate(), generate());
    }

    #[test]
    fn the_id_is_reused_on_the_next_launch() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nested").join("device");
        let first = load_or_create(&file);
        assert_eq!(load_or_create(&file), first);
    }

    #[test]
    fn a_corrupted_file_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("device");
        std::fs::write(&file, "not-an-id\n").unwrap();
        let id = load_or_create(&file);
        assert_eq!(id.len(), 32);
        assert_eq!(std::fs::read_to_string(&file).unwrap().trim(), id);
    }

    #[test]
    fn surrounding_whitespace_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("device");
        let id = "0123456789abcdef0123456789abcdef";
        std::fs::write(&file, format!("  {id}\n")).unwrap();
        assert_eq!(load_or_create(&file), id);
    }

    #[test]
    fn an_unwritable_location_still_yields_an_id() {
        // The directory itself cannot be written as a file.
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_or_create(dir.path()).len(), 32);
    }
}
