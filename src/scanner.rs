//! Parallel filesystem scan (§2 and §6.2 of ANALYSIS.md).
//! Instead of a queue with decaying timeouts — rayon with natural completion;
//! cancellation via `AtomicBool` actually stops the traversal.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use iced::futures::Stream;
use iced::futures::channel::mpsc;
use rayon::prelude::*;

use crate::fs_tree::{FsTree, TempNode};

/// Interval for sending progress to the UI, as in the original.
pub const PROGRESS_INTERVAL_MS: u64 = 100;

#[derive(Debug, Clone)]
pub enum ScanEvent {
    Progress { current: String, files: u64 },
    Finished(Arc<FsTree>),
    Canceled,
}

/// Starts the scan on a background thread and returns a stream of events
/// for `iced::Task::run`.
pub fn start_scan(root: PathBuf, cancel: Arc<AtomicBool>) -> impl Stream<Item = ScanEvent> {
    let (tx, rx) = mpsc::unbounded();
    std::thread::spawn(move || {
        let progress = Progress {
            tx: tx.clone(),
            files: AtomicU64::new(0),
            last_sent: Mutex::new(Instant::now()),
        };
        let name = root
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());
        let children = scan_dir(&root, &cancel, &progress);
        let event = if cancel.load(Ordering::Relaxed) {
            ScanEvent::Canceled
        } else {
            ScanEvent::Finished(Arc::new(FsTree::from_temp(TempNode {
                name,
                path: root,
                size: 0,
                is_dir: true,
                children,
            })))
        };
        let _ = tx.unbounded_send(event);
    });
    rx
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

/// Traverses a directory: files are collected immediately, subdirectories in parallel
/// via rayon. Read errors (permission denied etc.) yield an empty branch,
/// as in the original. The `DT_UNKNOWN` fallback goes through `entry.metadata()`.
fn scan_dir(path: &Path, cancel: &AtomicBool, progress: &Progress) -> Vec<TempNode> {
    if cancel.load(Ordering::Relaxed) {
        return Vec::new();
    }
    let Ok(entries) = fs::read_dir(path) else {
        return Vec::new();
    };

    let mut nodes = Vec::new();
    let mut dirs = Vec::new();
    for entry in entries.flatten() {
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
        let name = entry.file_name().to_string_lossy().into_owned();
        let entry_path = entry.path();
        if file_type.is_dir() {
            dirs.push((name, entry_path));
        } else if file_type.is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            progress.file_seen(&entry_path);
            nodes.push(TempNode {
                name,
                path: entry_path,
                size,
                is_dir: false,
                children: Vec::new(),
            });
        }
    }

    nodes.par_extend(dirs.into_par_iter().map(|(name, dir_path)| {
        let children = scan_dir(&dir_path, cancel, progress);
        TempNode {
            name,
            path: dir_path,
            size: 0,
            is_dir: true,
            children,
        }
    }));
    nodes
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

    #[test]
    fn cancel_yields_canceled_event() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.bin"), vec![0u8; 100]).unwrap();

        let events = run_scan(dir.path().to_path_buf(), true);
        assert!(
            matches!(events.last(), Some(ScanEvent::Canceled)),
            "{:?}",
            events.last()
        );
    }
}
