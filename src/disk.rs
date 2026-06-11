//! Capacity of the volume that contains the scan root, for the mini
//! disk-usage bar in the top bar of the finished map.

use std::path::Path;

/// Used/total bytes of a volume at the moment of the query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskUsage {
    pub used: u64,
    pub total: u64,
}

impl DiskUsage {
    /// The used share of the volume in `0.0..=1.0` — the progress bar value.
    /// Clamped: the fields are public, so `used > total` is constructible.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            ((self.used as f64 / self.total as f64) as f32).min(1.0)
        }
    }
}

/// Queries the volume containing `path`. `None` when the path does not
/// exist (a typo'd scan root) or the OS query fails — the bar is hidden.
pub fn usage(path: &Path) -> Option<DiskUsage> {
    let total = fs4::total_space(path).ok()?;
    let available = fs4::available_space(path).ok()?;
    Some(DiskUsage {
        used: total.saturating_sub(available),
        total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_is_used_share_of_total() {
        let usage = DiskUsage {
            used: 250,
            total: 1000,
        };
        assert_eq!(usage.fraction(), 0.25);
    }

    #[test]
    fn fraction_of_empty_volume_is_zero() {
        let usage = DiskUsage { used: 0, total: 0 };
        assert_eq!(usage.fraction(), 0.0);
    }

    #[test]
    fn fraction_clamps_used_above_total() {
        let usage = DiskUsage {
            used: 2000,
            total: 1000,
        };
        assert_eq!(usage.fraction(), 1.0);
    }

    #[test]
    fn usage_of_existing_dir_reports_a_real_volume() {
        let dir = tempfile::tempdir().unwrap();
        let usage = usage(dir.path()).expect("a temp dir lives on a real volume");
        assert!(usage.total > 0);
        assert!(usage.used <= usage.total);
    }

    #[test]
    fn usage_of_missing_path_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(usage(&dir.path().join("missing")), None);
    }
}
