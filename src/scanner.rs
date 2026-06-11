//! Parallel filesystem scan.
//! Instead of the original's worker queue with decaying timeouts — rayon with natural completion;
//! cancellation via `AtomicBool` actually stops the traversal.
//!
//! Nodes are written into a shared append-only arena from which a background thread
//! periodically builds [`FsTree`] snapshots — the map is drawn already during
//! the scan.

#[cfg(unix)]
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use iced::futures::Stream;
use iced::futures::channel::mpsc;
use rayon::prelude::*;

use crate::fs_tree::{FsTree, ScanNode};

/// Interval for sending progress to the UI, as in the original.
pub const PROGRESS_INTERVAL_MS: u64 = 100;
/// Interval of intermediate tree snapshots for progressive rendering.
pub const SNAPSHOT_INTERVAL_MS: u64 = 500;

#[derive(Debug, Clone)]
pub enum ScanEvent {
    Progress { current: String, files: u64 },
    /// An intermediate snapshot: the arena is append-only, so the snapshot's `NodeId`s
    /// remain valid in subsequent snapshots and in the final tree.
    Snapshot(Arc<FsTree>),
    /// The final tree; on cancellation — partial (whatever was traversed).
    Finished(Arc<FsTree>),
}

/// Starts the scan on a background thread and returns a stream of events
/// for `iced::Task::run`.
pub fn start_scan(root: PathBuf, cancel: Arc<AtomicBool>) -> impl Stream<Item = ScanEvent> {
    let (tx, rx) = mpsc::unbounded();
    let name: Arc<str> = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| root.display().to_string())
        .into();
    let arena = Arc::new(Mutex::new(vec![ScanNode {
        name,
        path: root.as_path().into(),
        size: 0,
        is_dir: true,
        parent: 0,
    }]));
    // Condvar instead of a bare flag: the snapshot thread wakes up immediately
    // when the scan finishes, not after sitting out the rest of the interval
    // (the stream — and the app's tests — would otherwise hang for up to
    // SNAPSHOT_INTERVAL_MS after the scan).
    let done = Arc::new((Mutex::new(false), Condvar::new()));

    std::thread::spawn({
        let arena = arena.clone();
        let done = done.clone();
        let tx = tx.clone();
        move || {
            let progress = Progress {
                tx: tx.clone(),
                files: AtomicU64::new(0),
                last_sent: Mutex::new(Instant::now()),
            };
            let visited = VisitedDirs::new();
            visited.first_visit(&root);
            scan_dir(&root, 0, &arena, &cancel, &progress, &visited);
            *done.0.lock().unwrap() = true;
            done.1.notify_all();
            let tree = FsTree::from_arena(&arena.lock().unwrap());
            let _ = tx.unbounded_send(ScanEvent::Finished(Arc::new(tree)));
        }
    });

    // Snapshot thread: only the new tail is taken from the arena under the lock
    // (cheap Arc clones); the tree itself is built outside the lock.
    std::thread::spawn(move || {
        let mut mirror: Vec<ScanNode> = Vec::new();
        let (done_flag, done_signal) = &*done;
        loop {
            let (finished, _) = done_signal
                .wait_timeout_while(
                    done_flag.lock().unwrap(),
                    Duration::from_millis(SNAPSHOT_INTERVAL_MS),
                    |done| !*done,
                )
                .unwrap();
            if *finished {
                return;
            }
            drop(finished);
            {
                let nodes = arena.lock().unwrap();
                mirror.extend_from_slice(&nodes[mirror.len()..]);
            }
            let tree = FsTree::from_arena(&mirror);
            if tx.unbounded_send(ScanEvent::Snapshot(Arc::new(tree))).is_err() {
                return;
            }
        }
    });

    rx
}

/// Deduplication of directories by (device, inode). APFS firmlinks (/Users,
/// /Library etc., listed in /usr/share/firmlinks) are not symlinks — they look
/// like plain directories but lead into the Data volume, which is itself
/// mounted at /System/Volumes/Data, so a scan of "/" would count them twice.
/// Comparing the parent's and child's devices is not enough: /Users legally
/// lives on another volume and must still be scanned — only a second path to
/// the same directory is skipped.
struct VisitedDirs {
    #[cfg(unix)]
    seen: Mutex<HashSet<(u64, u64)>>,
}

impl VisitedDirs {
    fn new() -> Self {
        Self {
            #[cfg(unix)]
            seen: Mutex::new(HashSet::new()),
        }
    }

    /// Records the directory; `false` means its (device, inode) was already
    /// visited via another path. On non-unix there are no firmlinks/bind
    /// mounts to worry about — always `true`, no extra stat.
    fn first_visit(&self, path: &Path) -> bool {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let Ok(meta) = fs::metadata(path) else {
                return true;
            };
            self.seen.lock().unwrap().insert((meta.dev(), meta.ino()))
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            true
        }
    }
}

/// Progress: an atomic file counter; updates go to the UI no more often than
/// [`PROGRESS_INTERVAL_MS`]; `try_lock` keeps workers from waiting on each other.
struct Progress {
    tx: mpsc::UnboundedSender<ScanEvent>,
    files: AtomicU64,
    last_sent: Mutex<Instant>,
}

impl Progress {
    fn file_seen(&self, path: &Path) {
        let files = self.files.fetch_add(1, Ordering::Relaxed) + 1;
        let Ok(mut last_sent) = self.last_sent.try_lock() else {
            return;
        };
        if last_sent.elapsed() < Duration::from_millis(PROGRESS_INTERVAL_MS) {
            return;
        }
        *last_sent = Instant::now();
        let _ = self.tx.unbounded_send(ScanEvent::Progress {
            current: path.display().to_string(),
            files,
        });
    }
}

/// Traverses directory `path`, already present in the arena at index `dir_index`:
/// all directory entries are added to the arena under a single mutex acquisition
/// (files and subfolder nodes), then subfolders are traversed in parallel via rayon.
/// Read errors (permission denied etc.) yield an empty branch, as in
/// the original. The `DT_UNKNOWN` fallback goes through `entry.metadata()`.
fn scan_dir(
    path: &Path,
    dir_index: usize,
    arena: &Mutex<Vec<ScanNode>>,
    cancel: &AtomicBool,
    progress: &Progress,
    visited: &VisitedDirs,
) {
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };

    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
        // On a huge directory, cancellation mid-enumeration keeps what's
        // already collected instead of stat'ing the rest.
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let Ok(file_type) = entry
            .file_type()
            .or_else(|_| entry.metadata().map(|m| m.file_type()))
        else {
            continue;
        };
        // As in the original: only regular files and directories,
        // symlinks/sockets/FIFOs are skipped.
        if file_type.is_symlink() {
            continue;
        }
        let name: Arc<str> = entry.file_name().to_string_lossy().into_owned().into();
        let entry_path = entry.path();
        if file_type.is_dir() {
            if !visited.first_visit(&entry_path) {
                continue;
            }
            dirs.push((name, entry_path));
        } else if file_type.is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            progress.file_seen(&entry_path);
            files.push(ScanNode {
                name,
                path: entry_path.into(),
                size,
                is_dir: false,
                parent: dir_index,
            });
        }
    }

    let subdirs: Vec<(usize, PathBuf)> = {
        let mut nodes = arena.lock().unwrap();
        nodes.extend(files);
        dirs.into_iter()
            .map(|(name, dir_path)| {
                let id = nodes.len();
                nodes.push(ScanNode {
                    name,
                    path: dir_path.as_path().into(),
                    size: 0,
                    is_dir: true,
                    parent: dir_index,
                });
                (id, dir_path)
            })
            .collect()
    };

    subdirs
        .into_par_iter()
        .for_each(|(id, dir_path)| scan_dir(&dir_path, id, arena, cancel, progress, visited));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs_tree::DIR_ENTRY_SIZE;
    use iced::futures::StreamExt;
    use iced::futures::executor::block_on;
    use std::fs;
    use std::sync::atomic::Ordering;

    fn run_scan(root: PathBuf, canceled: bool) -> Vec<ScanEvent> {
        let cancel = Arc::new(AtomicBool::new(false));
        cancel.store(canceled, Ordering::Relaxed);
        block_on(start_scan(root, cancel).collect::<Vec<_>>())
    }

    #[test]
    fn scans_fixture_tree_with_correct_sizes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.bin"), vec![0u8; 100]).unwrap();
        fs::write(dir.path().join("b.bin"), vec![0u8; 200]).unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/c.bin"), vec![0u8; 300]).unwrap();

        let events = run_scan(dir.path().to_path_buf(), false);
        let Some(ScanEvent::Finished(tree)) = events.last() else {
            panic!("expected Finished, got {:?}", events.last());
        };

        let root = tree.node(tree.root);
        assert!(root.is_dir);
        // root = 4096 (itself) + sub (4096 + 300) + 100 + 200
        assert_eq!(root.size, DIR_ENTRY_SIZE * 2 + 600);
        assert_eq!(root.children.len(), 3);
        // Children in descending order: sub (4396), b (200), a (100).
        let names: Vec<&str> = root
            .children
            .iter()
            .map(|&id| tree.node(id).name.as_ref())
            .collect();
        assert_eq!(names, vec!["sub", "b.bin", "a.bin"]);
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("real.bin"), vec![0u8; 100]).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real.bin"), dir.path().join("link"))
            .unwrap();

        let events = run_scan(dir.path().to_path_buf(), false);
        let Some(ScanEvent::Finished(tree)) = events.last() else {
            panic!("expected Finished");
        };
        assert_eq!(tree.node(tree.root).children.len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn first_visit_is_false_on_second_path_to_same_inode() {
        let dir = tempfile::tempdir().unwrap();
        let visited = VisitedDirs::new();
        assert!(visited.first_visit(dir.path()));
        assert!(!visited.first_visit(dir.path()));
    }

    /// A directory whose (device, inode) was already seen is skipped entirely.
    /// A real firmlink cannot be created in a test, so the second path to the
    /// same inode is simulated by pre-registering the directory in the set.
    #[cfg(unix)]
    #[test]
    fn already_visited_directory_is_skipped() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/c.bin"), vec![0u8; 300]).unwrap();

        let visited = VisitedDirs::new();
        assert!(visited.first_visit(&dir.path().join("sub")));

        let (tx, _rx) = mpsc::unbounded();
        let progress = Progress {
            tx,
            files: AtomicU64::new(0),
            last_sent: Mutex::new(Instant::now()),
        };
        let arena = Mutex::new(vec![ScanNode {
            name: "root".into(),
            path: dir.path().into(),
            size: 0,
            is_dir: true,
            parent: 0,
        }]);
        scan_dir(
            dir.path(),
            0,
            &arena,
            &AtomicBool::new(false),
            &progress,
            &visited,
        );

        let nodes = arena.lock().unwrap();
        assert!(
            nodes.iter().all(|n| n.name.as_ref() != "sub"),
            "duplicate directory must not enter the arena: {nodes:?}"
        );
    }

    /// Cancellation no longer wipes the result: `Finished` arrives with whatever
    /// was traversed so far (here — only the root).
    #[test]
    fn cancel_yields_partial_tree() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.bin"), vec![0u8; 100]).unwrap();

        let events = run_scan(dir.path().to_path_buf(), true);
        let Some(ScanEvent::Finished(tree)) = events.last() else {
            panic!("expected Finished, got {:?}", events.last());
        };
        let root = tree.node(tree.root);
        assert!(root.children.is_empty());
        assert_eq!(root.size, DIR_ENTRY_SIZE);
    }
}
