//! Capacity of the volume that contains the scan root, for the mini
//! disk-usage bar in the top bar of the finished map, and the list of
//! mounted volume roots for the quick row on the start screen.

use std::path::{Path, PathBuf};

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

/// Roots of the mounted volumes, for the quick disk row on the start
/// screen. On Unix the filesystem root `/` comes first and the extra
/// volumes follow in name order; on Windows it is drive roots in
/// letter order (`A:\` before `C:\` if both exist).
#[cfg(target_os = "macos")]
pub fn mounted_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/")];
    roots.extend(volume_roots(Path::new("/Volumes")));
    roots
}

#[cfg(target_os = "linux")]
pub fn mounted_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/")];
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        roots.extend(roots_from_mounts(&mounts));
    }
    roots
}

#[cfg(windows)]
pub fn mounted_roots() -> Vec<PathBuf> {
    // `is_dir` filters letters without a volume behind them (and empty
    // optical drives, which exist but cannot be read).
    ('A'..='Z')
        .map(|letter| PathBuf::from(format!("{letter}:\\")))
        .filter(|root| root.is_dir())
        .collect()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn mounted_roots() -> Vec<PathBuf> {
    vec![PathBuf::from("/")]
}

/// The extra volumes under `/Volumes`, sorted by name. Only real
/// directories count: the boot volume drops out as a symlink to `/`
/// (already first in the row), and hidden entries (`.timemachine` and
/// friends) are not volumes.
#[cfg(any(target_os = "macos", test))]
fn volume_roots(volumes: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(volumes) else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        // The readdir entry type, not `path.is_dir()`: the latter stats
        // the mount target and can hang on a dead network volume. Symlinks
        // drop out here too, the boot firmlink among them.
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect();
    roots.sort();
    roots
}

/// Removable/extra mount points out of `/proc/mounts` text, sorted by
/// name: everything under (or mounted directly at) the directories
/// desktop Linux mounts external drives into.
/// Octal escapes (`\040` for a space) are decoded;
/// bind-mount duplicates collapse into one entry.
#[cfg(any(target_os = "linux", test))]
fn roots_from_mounts(mounts: &str) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for mount_point in mounts
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .filter(|point| {
            // The directory itself counts too (`mount /dev/sdb1 /mnt`),
            // but not a sibling like /mnt2.
            ["/media", "/run/media", "/mnt"].iter().any(|dir| {
                point
                    .strip_prefix(dir)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            })
        })
    {
        let root = PathBuf::from(unescape_mount_point(mount_point));
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots.sort();
    roots
}

/// Decodes the `\ooo` octal escapes `/proc/mounts` hides whitespace and
/// backslashes behind: `\040` space, `\011` tab, `\012` newline, `\134`
/// backslash. Anything that is not a backslash followed by exactly three
/// octal digits passes through untouched.
#[cfg(any(target_os = "linux", test))]
fn unescape_mount_point(point: &str) -> String {
    let bytes = point.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // A byte is three octal digits at most (`\377`), so the leading
        // digit stops at '3'.
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 1] <= b'3'
            && bytes[i + 2..=i + 3].iter().all(|b| (b'0'..=b'7').contains(b))
        {
            decoded.push(
                (bytes[i + 1] - b'0') * 0o100 + (bytes[i + 2] - b'0') * 0o10 + (bytes[i + 3] - b'0'),
            );
            i += 4;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// The display name of a volume root: the directory name for mounted
/// volumes (`/Volumes/Data` → `Data`), the path itself for bare roots
/// (`/`, `C:\`).
pub fn root_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
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
    use std::path::PathBuf;

    #[test]
    fn mounted_roots_include_the_filesystem_root() {
        let roots = mounted_roots();
        assert!(!roots.is_empty());
        #[cfg(unix)]
        assert_eq!(roots[0], PathBuf::from("/"));
    }

    #[cfg(unix)]
    #[test]
    fn volume_roots_lists_dirs_and_skips_the_boot_firmlink() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Data")).unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"not a volume").unwrap();
        // The boot volume appears in /Volumes as a symlink to "/".
        std::os::unix::fs::symlink("/", dir.path().join("Macintosh HD")).unwrap();
        std::fs::create_dir(dir.path().join(".hidden")).unwrap();
        assert_eq!(volume_roots(dir.path()), vec![dir.path().join("Data")]);
    }

    #[cfg(unix)]
    #[test]
    fn volume_roots_of_missing_dir_are_empty() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(volume_roots(&dir.path().join("missing")), Vec::<PathBuf>::new());
    }

    #[test]
    fn roots_from_mounts_keep_removable_mount_points_in_name_order() {
        // Deliberately out of name order: the result must not lean on
        // the kernel's mount order.
        let mounts = "\
/dev/sda1 / ext4 rw 0 0
proc /proc proc rw 0 0
/dev/sdd1 /run/media/user/Card exfat rw 0 0
/dev/sdb1 /media/user/USB\\040Drive vfat rw 0 0
/dev/sdc1 /mnt/backup ext4 rw 0 0
tmpfs /run/user/1000 tmpfs rw 0 0
/dev/sdc1 /mnt/backup ext4 rw 0 0";
        assert_eq!(
            roots_from_mounts(mounts),
            vec![
                PathBuf::from("/media/user/USB Drive"),
                PathBuf::from("/mnt/backup"),
                PathBuf::from("/run/media/user/Card"),
            ]
        );
    }

    #[test]
    fn roots_from_mounts_accept_a_drive_mounted_at_the_directory_itself() {
        // `mount /dev/sdb1 /mnt` is a common manual mount: the directory
        // itself counts, but a sibling like /mnt2 must not.
        let mounts = "\
/dev/sdb1 /mnt ext4 rw 0 0
/dev/sdc1 /mnt2 ext4 rw 0 0";
        assert_eq!(roots_from_mounts(mounts), vec![PathBuf::from("/mnt")]);
    }

    #[test]
    fn roots_from_mounts_decode_all_octal_escapes() {
        // /proc/mounts escapes space, tab, newline and backslash as \040,
        // \011, \012 and \134 — not only the space.
        let mounts = "/dev/sdb1 /media/user/a\\011b\\012c\\134d vfat rw 0 0";
        assert_eq!(
            roots_from_mounts(mounts),
            vec![PathBuf::from("/media/user/a\tb\nc\\d")]
        );
    }

    #[test]
    fn roots_from_mounts_keep_non_escape_backslashes_literal() {
        // Not an octal triple (\8 is no octal digit, \77 is too short):
        // the backslash must survive as-is instead of corrupting the path.
        let mounts = "/dev/sdb1 /media/user/a\\800b\\77 vfat rw 0 0";
        assert_eq!(
            roots_from_mounts(mounts),
            vec![PathBuf::from("/media/user/a\\800b\\77")]
        );
    }

    #[test]
    fn root_label_is_the_volume_name() {
        assert_eq!(root_label(Path::new("/Volumes/My Disk")), "My Disk");
    }

    #[test]
    fn root_label_of_bare_root_is_the_path_itself() {
        assert_eq!(root_label(Path::new("/")), "/");
    }

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
