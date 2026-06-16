//! Volume capacity and mounted volume roots.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskUsage {
    pub used: u64,
    pub total: u64,
}

impl DiskUsage {
    /// Used share in `0.0..=1.0`. Clamped: fields are public, so `used > total` is constructible.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            ((self.used as f64 / self.total as f64) as f32).min(1.0)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskKind {
    /// Fixed drive, also the fallback when the OS gives no usable answer.
    Internal,
    Removable,
    Network,
    /// Only Windows reports these; on Unix dead code.
    #[cfg_attr(not(windows), allow(dead_code))]
    Optical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskRoot {
    pub path: PathBuf,
    pub kind: DiskKind,
}

/// Mounted volume roots. Unix: `/` first, then extra volumes in name order.
/// Windows: drive roots in letter order.
#[cfg(target_os = "macos")]
pub fn mounted_roots() -> Vec<DiskRoot> {
    let types = fs_types();
    let mut roots = vec![DiskRoot {
        path: PathBuf::from("/"),
        kind: DiskKind::Internal,
    }];
    roots.extend(volume_roots(Path::new("/Volumes")).into_iter().map(|path| {
        // The boot volume only shows up as `/`, so anything under /Volumes is external.
        let kind = match types.get(&path) {
            Some(fstype) if is_network_fs(fstype) => DiskKind::Network,
            _ => DiskKind::Removable,
        };
        DiskRoot { path, kind }
    }));
    roots
}

#[cfg(target_os = "linux")]
pub fn mounted_roots() -> Vec<DiskRoot> {
    let mut roots = vec![DiskRoot {
        path: PathBuf::from("/"),
        kind: DiskKind::Internal,
    }];
    if let Ok(mounts) = std::fs::read_to_string("/proc/mounts") {
        roots.extend(roots_from_mounts(&mounts, device_is_removable));
    }
    roots
}

#[cfg(windows)]
pub fn mounted_roots() -> Vec<DiskRoot> {
    // `is_dir` filters letters with no volume behind them and empty optical drives.
    ('A'..='Z')
        .map(|letter| PathBuf::from(format!("{letter}:\\")))
        .filter(|root| root.is_dir())
        .map(|path| {
            let kind = drive_kind(&path);
            DiskRoot { path, kind }
        })
        .collect()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn mounted_roots() -> Vec<DiskRoot> {
    vec![DiskRoot {
        path: PathBuf::from("/"),
        kind: DiskKind::Internal,
    }]
}

/// Whether a filesystem type name is a network filesystem.
#[cfg(any(target_os = "macos", target_os = "linux", test))]
fn is_network_fs(fstype: &str) -> bool {
    let fstype = fstype.strip_prefix("fuse.").unwrap_or(fstype);
    fstype.starts_with("nfs")
        || matches!(
            fstype,
            "cifs"
                | "smbfs"
                | "smb3"
                | "sshfs"
                | "davfs"
                | "davfs2"
                | "webdav"
                | "afpfs"
                | "ftp"
                | "curlftpfs"
                | "9p"
        )
}

/// Filesystem type per mount point. `MNT_NOWAIT` avoids refreshing entries,
/// so a dead network volume cannot hang the query.
#[cfg(target_os = "macos")]
fn fs_types() -> std::collections::HashMap<PathBuf, String> {
    use std::ffi::CStr;
    use std::os::unix::ffi::OsStrExt;
    let mut mounts: *mut libc::statfs = std::ptr::null_mut();
    let count = unsafe { libc::getmntinfo(&mut mounts, libc::MNT_NOWAIT) };
    // Backs the `from_raw_parts` non-null requirement.
    if count <= 0 || mounts.is_null() {
        return std::collections::HashMap::new();
    }
    // getmntinfo owns this buffer, valid until the next call on this thread.
    unsafe { std::slice::from_raw_parts(mounts, count as usize) }
        .iter()
        .map(|mount| {
            let point = unsafe { CStr::from_ptr(mount.f_mntonname.as_ptr()) };
            let fstype = unsafe { CStr::from_ptr(mount.f_fstypename.as_ptr()) };
            (
                PathBuf::from(std::ffi::OsStr::from_bytes(point.to_bytes())),
                fstype.to_string_lossy().into_owned(),
            )
        })
        .collect()
}

/// Whether the block device behind a mount is removable, off the sysfs
/// `removable` flag. The flag lives on the whole disk, so a partition
/// (`sdb1`) walks up to its parent (`sdb`) via the `..` of the sysfs symlink.
#[cfg(target_os = "linux")]
fn device_is_removable(device: &str) -> bool {
    let Some(name) = device.strip_prefix("/dev/") else {
        return false;
    };
    let dir = Path::new("/sys/class/block").join(name);
    [dir.join("removable"), dir.join("../removable")]
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|flag| flag.trim() == "1")
}

#[cfg(windows)]
fn drive_kind(root: &Path) -> DiskKind {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetDriveTypeW(root: *const u16) -> u32;
    }
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain([0]).collect();
    match unsafe { GetDriveTypeW(wide.as_ptr()) } {
        2 => DiskKind::Removable, // DRIVE_REMOVABLE
        4 => DiskKind::Network,   // DRIVE_REMOTE
        5 => DiskKind::Optical,   // DRIVE_CDROM
        // DRIVE_FIXED plus unknown/error codes.
        _ => DiskKind::Internal,
    }
}

/// Extra volumes under `/Volumes`, sorted by name. Only real directories:
/// the boot volume drops out as a symlink to `/`, hidden entries are not volumes.
#[cfg(any(target_os = "macos", test))]
fn volume_roots(volumes: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(volumes) else {
        return Vec::new();
    };
    let mut roots: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        // readdir entry type, not `path.is_dir()`: the latter stats the mount
        // target and can hang on a dead network volume. Symlinks drop out here too.
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .map(|entry| entry.path())
        .collect();
    roots.sort();
    roots
}

/// External mount points from `/proc/mounts` text, sorted by name: anything
/// under (or at) the dirs desktop Linux mounts drives into. Kind comes off the
/// fstype or the `removable` test, injected to keep the parser pure. Octal
/// escapes are decoded; bind-mount duplicates collapse into one entry.
#[cfg(any(target_os = "linux", test))]
fn roots_from_mounts(mounts: &str, removable: impl Fn(&str) -> bool) -> Vec<DiskRoot> {
    let mut roots: Vec<DiskRoot> = Vec::new();
    for (device, mount_point, fstype) in mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?, fields.next()?, fields.next()?))
        })
        .filter(|(_, point, _)| {
            // The directory itself counts (`mount /dev/sdb1 /mnt`), but not a sibling like /mnt2.
            ["/media", "/run/media", "/mnt"].iter().any(|dir| {
                point
                    .strip_prefix(dir)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            })
        })
    {
        let path = PathBuf::from(unescape_mount_point(mount_point));
        if roots.iter().any(|root| root.path == path) {
            continue;
        }
        let kind = if is_network_fs(fstype) {
            DiskKind::Network
        } else if removable(device) {
            DiskKind::Removable
        } else {
            DiskKind::Internal
        };
        roots.push(DiskRoot { path, kind });
    }
    roots.sort_by(|a, b| a.path.cmp(&b.path));
    roots
}

/// Decodes the `\ooo` octal escapes in `/proc/mounts` (`\040` space, `\011`
/// tab, `\012` newline, `\134` backslash). Anything else passes through.
#[cfg(any(target_os = "linux", test))]
fn unescape_mount_point(point: &str) -> String {
    let bytes = point.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // A byte is at most three octal digits (`\377`), so the leading digit stops at '3'.
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 1] <= b'3'
            && bytes[i + 2..=i + 3]
                .iter()
                .all(|b| (b'0'..=b'7').contains(b))
        {
            decoded.push(
                (bytes[i + 1] - b'0') * 0o100
                    + (bytes[i + 2] - b'0') * 0o10
                    + (bytes[i + 3] - b'0'),
            );
            i += 4;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Display name of a volume root: directory name (`/Volumes/Data` -> `Data`),
/// or the path itself for bare roots (`/`, `C:\`).
pub fn root_label(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Queries the volume containing `path`. `None` if the path is missing or the OS query fails.
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
    fn mounted_roots_keep_the_platform_contract() {
        #[cfg(unix)]
        assert_eq!(
            mounted_roots().first(),
            Some(&DiskRoot {
                path: PathBuf::from("/"),
                kind: DiskKind::Internal,
            })
        );
        #[cfg(windows)]
        for root in mounted_roots() {
            let root = root.path.display().to_string();
            assert!(root.len() == 3 && root.ends_with(":\\"), "{root}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn volume_roots_lists_dirs_and_skips_the_boot_firmlink() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("Data")).unwrap();
        std::fs::write(dir.path().join("notes.txt"), b"not a volume").unwrap();
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

    fn mount_points(mounts: &str) -> Vec<PathBuf> {
        roots_from_mounts(mounts, |_| false)
            .into_iter()
            .map(|root| root.path)
            .collect()
    }

    #[test]
    fn roots_from_mounts_keep_removable_mount_points_in_name_order() {
        // Deliberately out of name order.
        let mounts = "\
/dev/sda1 / ext4 rw 0 0
proc /proc proc rw 0 0
/dev/sdd1 /run/media/user/Card exfat rw 0 0
/dev/sdb1 /media/user/USB\\040Drive vfat rw 0 0
/dev/sdc1 /mnt/backup ext4 rw 0 0
tmpfs /run/user/1000 tmpfs rw 0 0
/dev/sdc1 /mnt/backup ext4 rw 0 0";
        assert_eq!(
            mount_points(mounts),
            vec![
                PathBuf::from("/media/user/USB Drive"),
                PathBuf::from("/mnt/backup"),
                PathBuf::from("/run/media/user/Card"),
            ]
        );
    }

    #[test]
    fn roots_from_mounts_bucket_kinds_by_fstype_and_device() {
        let mounts = "\
/dev/sdb1 /media/user/Stick vfat rw 0 0
/dev/sdc1 /mnt/backup ext4 rw 0 0
//server/share /mnt/share cifs rw 0 0";
        assert_eq!(
            roots_from_mounts(mounts, |device| device == "/dev/sdb1"),
            vec![
                DiskRoot {
                    path: PathBuf::from("/media/user/Stick"),
                    kind: DiskKind::Removable,
                },
                DiskRoot {
                    path: PathBuf::from("/mnt/backup"),
                    kind: DiskKind::Internal,
                },
                DiskRoot {
                    path: PathBuf::from("/mnt/share"),
                    kind: DiskKind::Network,
                },
            ]
        );
    }

    #[test]
    fn roots_from_mounts_accept_a_drive_mounted_at_the_directory_itself() {
        let mounts = "\
/dev/sdb1 /mnt ext4 rw 0 0
/dev/sdc1 /mnt2 ext4 rw 0 0";
        assert_eq!(mount_points(mounts), vec![PathBuf::from("/mnt")]);
    }

    #[test]
    fn roots_from_mounts_decode_all_octal_escapes() {
        let mounts = "/dev/sdb1 /media/user/a\\011b\\012c\\134d vfat rw 0 0";
        assert_eq!(
            mount_points(mounts),
            vec![PathBuf::from("/media/user/a\tb\nc\\d")]
        );
    }

    #[test]
    fn roots_from_mounts_keep_non_escape_backslashes_literal() {
        // Not an octal triple (\8 is no octal digit, \77 is too short).
        let mounts = "/dev/sdb1 /media/user/a\\800b\\77 vfat rw 0 0";
        assert_eq!(
            mount_points(mounts),
            vec![PathBuf::from("/media/user/a\\800b\\77")]
        );
    }

    #[test]
    fn network_filesystems_are_told_apart_from_local_ones() {
        for fstype in [
            "nfs",
            "nfs4",
            "cifs",
            "smbfs",
            "sshfs",
            "fuse.sshfs",
            "afpfs",
        ] {
            assert!(is_network_fs(fstype), "{fstype} is a network filesystem");
        }
        for fstype in ["ext4", "vfat", "exfat", "apfs", "ntfs", "fuseblk", "btrfs"] {
            assert!(!is_network_fs(fstype), "{fstype} is a local filesystem");
        }
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
