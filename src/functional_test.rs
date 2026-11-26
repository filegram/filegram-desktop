use iced::Task;

use crate::fs_tree::NodeId;
use crate::{App, Message, ScanState, update};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Scan,
    EnterDirectory { directory: NodeId },
    GoUp,
    RequestDelete { file: NodeId },
    CancelDelete { file: NodeId },
    ToggleTheme { expected_dark: bool },
    NewScan,
    Edge(EdgeStage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeStage {
    Scan,
    EnterDirectory { directory: NodeId },
    GoUp,
    RequestDelete { file: NodeId },
    CancelDelete { file: NodeId },
    ConfirmDelete { file: NodeId },
    DeleteConfirmed { file: NodeId },
    AfterDelete { file: NodeId },
    RescanRemoved,
    RescanAdded,
    ToggleTheme { expected_dark: bool },
    NewScan,
    BlankPath,
    BlankPathSubmitted { scans: u32 },
}

pub enum Action {
    Wait,
    Message(Message),
    Exit,
}

pub fn on_frame(app: &mut App) -> Task<Message> {
    match next_action(app) {
        Action::Wait => Task::none(),
        Action::Message(message) => update(app, message),
        Action::Exit => iced::exit(),
    }
}

pub fn next_action(app: &mut App) -> Action {
    match app.functional.expect("functional test stage is required") {
        Stage::Scan => {
            if matches!(app.scan, ScanState::Running { .. }) {
                return Action::Wait;
            }
            assert!(matches!(app.scan, ScanState::Done), "scan did not finish");
            let tree = app.tree.as_ref().expect("finished scan has no tree");
            assert_eq!(tree.node(tree.root).files, 3, "fixture file count changed");
            let root = tree.root;
            let directory = child(app, root, "sub");
            app.functional = Some(Stage::EnterDirectory { directory });
            Action::Message(Message::BrickPressed(directory))
        }
        Stage::EnterDirectory { directory } => {
            assert_eq!(app.current, directory, "directory click did not navigate");
            assert_eq!(
                app.nav_stack.as_slice(),
                &[root(app)],
                "navigation stack is wrong"
            );
            app.functional = Some(Stage::GoUp);
            Action::Message(Message::GoUp)
        }
        Stage::GoUp => {
            let root = root(app);
            assert_eq!(app.current, root, "go-up did not restore the root");
            assert!(app.nav_stack.is_empty(), "go-up left a navigation entry");
            let file = child(app, root, "small.bin");
            app.functional = Some(Stage::RequestDelete { file });
            Action::Message(Message::DeleteRequested(file))
        }
        Stage::RequestDelete { file } => {
            assert_eq!(app.pending_delete, Some(file), "delete dialog did not open");
            app.functional = Some(Stage::CancelDelete { file });
            Action::Message(Message::DeleteCancelled)
        }
        Stage::CancelDelete { file } => {
            assert_eq!(app.pending_delete, None, "delete dialog did not close");
            assert!(
                app.tree
                    .as_ref()
                    .is_some_and(|tree| { tree.node(root(app)).children.contains(&file) }),
                "cancelling delete removed the file"
            );
            app.functional = Some(Stage::ToggleTheme {
                expected_dark: !app.is_dark(),
            });
            Action::Message(Message::ToggleTheme)
        }
        Stage::ToggleTheme { expected_dark } => {
            assert_eq!(app.is_dark(), expected_dark, "theme did not toggle");
            app.functional = Some(Stage::NewScan);
            Action::Message(Message::NewScan)
        }
        Stage::NewScan => {
            assert!(
                matches!(app.scan, ScanState::Idle),
                "new-scan screen did not open"
            );
            assert!(app.tree.is_none(), "new-scan screen kept the old tree");
            eprintln!("filegram: functional GUI scenario passed");
            Action::Exit
        }
        Stage::Edge(stage) => next_edge_action(app, stage),
    }
}

fn next_edge_action(app: &mut App, stage: EdgeStage) -> Action {
    match stage {
        EdgeStage::Scan => {
            if matches!(app.scan, ScanState::Running { .. }) {
                return Action::Wait;
            }
            assert!(
                matches!(app.scan, ScanState::Done),
                "edge scan did not finish"
            );
            let root = root(app);
            let tree = app.tree.as_ref().expect("edge scan has no tree");
            assert_eq!(tree.node(root).files, 5, "edge fixture file count changed");
            let sub = child(app, root, "sub");
            assert_eq!(tree.node(child(app, root, "zero.bin")).size, 0);
            child(app, root, "файл-😀.txt");
            assert!(
                find_child(app, sub, "loop").is_none(),
                "symlink was scanned"
            );
            let empty = child(app, root, "empty");
            app.functional = Some(Stage::Edge(EdgeStage::EnterDirectory { directory: empty }));
            Action::Message(Message::BrickPressed(empty))
        }
        EdgeStage::EnterDirectory { directory } => {
            assert_eq!(app.current, root(app), "empty directory should not open");
            assert!(
                app.nav_stack.is_empty(),
                "empty directory changed navigation"
            );
            let sub = child(app, root(app), "sub");
            app.functional = Some(Stage::Edge(EdgeStage::GoUp));
            assert_ne!(sub, directory);
            Action::Message(Message::BrickPressed(sub))
        }
        EdgeStage::GoUp => {
            let sub = child(app, root(app), "sub");
            assert_eq!(app.current, sub, "non-empty directory did not open");
            let file = child(app, root(app), "small.bin");
            app.functional = Some(Stage::Edge(EdgeStage::RequestDelete { file }));
            Action::Message(Message::GoUp)
        }
        EdgeStage::RequestDelete { file } => {
            assert_eq!(app.current, root(app), "go-up did not restore the root");
            app.functional = Some(Stage::Edge(EdgeStage::CancelDelete { file }));
            Action::Message(Message::DeleteRequested(file))
        }
        EdgeStage::CancelDelete { file } => {
            assert_eq!(app.pending_delete, Some(file), "delete dialog did not open");
            app.functional = Some(Stage::Edge(EdgeStage::ConfirmDelete { file }));
            Action::Message(Message::DeleteCancelled)
        }
        EdgeStage::ConfirmDelete { file } => {
            assert_eq!(app.pending_delete, None, "delete dialog did not close");
            assert!(find_child(app, root(app), "small.bin").is_some());
            app.functional = Some(Stage::Edge(EdgeStage::DeleteConfirmed { file }));
            Action::Message(Message::DeleteRequested(file))
        }
        EdgeStage::DeleteConfirmed { file } => {
            assert_eq!(
                app.pending_delete,
                Some(file),
                "delete dialog did not reopen"
            );
            app.functional = Some(Stage::Edge(EdgeStage::AfterDelete { file }));
            Action::Message(Message::DeleteConfirmed)
        }
        EdgeStage::AfterDelete { file } => {
            let root = root(app);
            let tree = app.tree.as_ref().expect("deleted tree is missing");
            assert_eq!(tree.node(root).files, 4, "deleted file is still counted");
            assert!(
                !tree.node(root).children.contains(&file),
                "deleted file is still shown"
            );
            assert!(
                !tree.node(file).path.exists(),
                "deleted fixture still exists"
            );
            app.functional = Some(Stage::Edge(EdgeStage::RescanRemoved));
            Action::Message(Message::Rescan)
        }
        EdgeStage::RescanRemoved => {
            if matches!(app.scan, ScanState::Running { .. }) {
                return Action::Wait;
            }
            let root = root(app);
            assert_eq!(app.tree.as_ref().unwrap().node(root).files, 4);
            assert!(
                find_child(app, root, "small.bin").is_none(),
                "rescan restored deleted file"
            );
            let added = app.tree.as_ref().unwrap().node(root).path.join("added.bin");
            std::fs::write(&added, vec![0; 123]).expect("failed to add an external fixture file");
            app.functional = Some(Stage::Edge(EdgeStage::RescanAdded));
            Action::Message(Message::Rescan)
        }
        EdgeStage::RescanAdded => {
            if matches!(app.scan, ScanState::Running { .. }) {
                return Action::Wait;
            }
            let root = root(app);
            assert_eq!(app.tree.as_ref().unwrap().node(root).files, 5);
            let added = child(app, root, "added.bin");
            assert_eq!(app.tree.as_ref().unwrap().node(added).size, 123);
            app.functional = Some(Stage::Edge(EdgeStage::ToggleTheme {
                expected_dark: !app.is_dark(),
            }));
            Action::Message(Message::ToggleTheme)
        }
        EdgeStage::ToggleTheme { expected_dark } => {
            assert_eq!(app.is_dark(), expected_dark, "theme did not toggle");
            app.functional = Some(Stage::Edge(EdgeStage::NewScan));
            Action::Message(Message::NewScan)
        }
        EdgeStage::NewScan => {
            assert!(
                matches!(app.scan, ScanState::Idle),
                "new-scan screen did not open"
            );
            assert!(app.tree.is_none(), "new-scan screen kept the old tree");
            app.functional = Some(Stage::Edge(EdgeStage::BlankPath));
            Action::Message(Message::PathChanged(" \t ".to_string()))
        }
        EdgeStage::BlankPath => {
            assert_eq!(app.path_input, " \t ");
            app.functional = Some(Stage::Edge(EdgeStage::BlankPathSubmitted {
                scans: app.scans,
            }));
            Action::Message(Message::StartScan)
        }
        EdgeStage::BlankPathSubmitted { scans } => {
            assert_eq!(app.scans, scans, "blank path started a scan");
            assert!(matches!(app.scan, ScanState::Idle));
            assert!(app.tree.is_none());
            eprintln!("filegram: functional GUI edge cases passed");
            Action::Exit
        }
    }
}

fn root(app: &App) -> NodeId {
    app.tree.as_ref().expect("scenario tree is missing").root
}

fn child(app: &App, parent: NodeId, name: &str) -> NodeId {
    find_child(app, parent, name).unwrap_or_else(|| panic!("fixture child {name:?} is missing"))
}

fn find_child(app: &App, parent: NodeId, name: &str) -> Option<NodeId> {
    let tree = app.tree.as_ref().expect("scenario tree is missing");
    tree.node(parent)
        .children
        .iter()
        .copied()
        .find(|&id| tree.node(id).name.as_ref() == name)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::sync::Arc;

    use super::{Action, EdgeStage, Stage, next_action};
    use crate::fs_tree::{FsTree, ScanNode};
    use crate::{App, Message, ScanState, history, initial_app, update};

    fn node(parent: usize, path: &str, size: u64, is_dir: bool) -> ScanNode {
        ScanNode {
            name: Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            path: Path::new(path).into(),
            size,
            is_dir,
            parent,
        }
    }

    fn app_with_scanned_fixture() -> App {
        let tree = FsTree::from_arena(&[
            node(0, "/smoke-fixture", 0, true),
            node(0, "/smoke-fixture/big.bin", 16_384, false),
            node(0, "/smoke-fixture/small.bin", 4_096, false),
            node(0, "/smoke-fixture/sub", 0, true),
            node(3, "/smoke-fixture/sub/nested.bin", 8_192, false),
        ]);
        let mut app = initial_app(history::History::default(), None);
        app.tree = Some(Arc::new(tree));
        app.scan = ScanState::Done;
        app.functional = Some(Stage::Scan);
        app
    }

    fn app_with_edge_fixture() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("sub")).unwrap();
        fs::create_dir(root.join("empty")).unwrap();
        fs::write(root.join("big.bin"), vec![0; 16_384]).unwrap();
        fs::write(root.join("small.bin"), vec![0; 4_096]).unwrap();
        fs::write(root.join("sub/nested.bin"), vec![0; 8_192]).unwrap();
        fs::write(root.join("zero.bin"), []).unwrap();
        fs::write(root.join("файл-😀.txt"), "unicode").unwrap();
        let path = |path: &Path| path.to_string_lossy().into_owned();
        let tree = FsTree::from_arena(&[
            node(0, &path(root), 0, true),
            node(0, &path(&root.join("big.bin")), 16_384, false),
            node(0, &path(&root.join("small.bin")), 4_096, false),
            node(0, &path(&root.join("sub")), 0, true),
            node(3, &path(&root.join("sub/nested.bin")), 8_192, false),
            node(0, &path(&root.join("empty")), 0, true),
            node(0, &path(&root.join("zero.bin")), 0, false),
            node(0, &path(&root.join("файл-😀.txt")), 7, false),
        ]);
        let mut app = initial_app(history::History::default(), None);
        app.tree = Some(Arc::new(tree));
        app.scan = ScanState::Done;
        app.functional = Some(Stage::Edge(EdgeStage::Scan));
        app.delete = |path| std::fs::remove_file(path);
        (dir, app)
    }

    fn drive_next(app: &mut App) {
        match next_action(app) {
            Action::Message(message) => {
                let _ = update(app, message);
            }
            _ => panic!("edge scenario should send a message"),
        }
    }

    #[test]
    fn edge_empty_directory_click_keeps_root() {
        let (_dir, mut app) = app_with_edge_fixture();
        let root = app.current;

        let Action::Message(Message::BrickPressed(empty)) = next_action(&mut app) else {
            panic!("edge scan should click the empty directory");
        };
        let _ = update(&mut app, Message::BrickPressed(empty));
        assert_eq!(app.current, root);

        let Action::Message(Message::BrickPressed(directory)) = next_action(&mut app) else {
            panic!("empty-directory check should be followed by normal navigation");
        };
        assert_ne!(directory, empty);
    }

    #[test]
    fn edge_confirmed_delete_removes_file_and_requests_rescan() {
        let (dir, mut app) = app_with_edge_fixture();
        for _ in 0..7 {
            drive_next(&mut app);
        }

        assert!(!dir.path().join("small.bin").exists());
        assert!(app.pending_delete.is_none());
        let tree = app.tree.as_ref().unwrap();
        assert_eq!(tree.node(tree.root).files, 4);
        assert!(matches!(
            next_action(&mut app),
            Action::Message(Message::Rescan)
        ));
    }

    #[test]
    fn edge_blank_path_does_not_start_scan() {
        let (_dir, mut app) = app_with_edge_fixture();
        app.tree = None;
        app.scan = ScanState::Idle;
        app.scans = 2;
        app.functional = Some(Stage::Edge(EdgeStage::NewScan));

        let Action::Message(Message::PathChanged(path)) = next_action(&mut app) else {
            panic!("new-scan check should enter a blank path");
        };
        assert_eq!(path, " \t ");
        let _ = update(&mut app, Message::PathChanged(path));
        let Action::Message(Message::StartScan) = next_action(&mut app) else {
            panic!("blank path should be submitted");
        };
        let _ = update(&mut app, Message::StartScan);

        assert_eq!(app.scans, 2);
        assert!(matches!(app.scan, ScanState::Idle));
        assert!(matches!(next_action(&mut app), Action::Exit));
    }

    #[test]
    fn scenario_drives_gui_workflow_to_completion() {
        let mut app = app_with_scanned_fixture();
        let initial_dark = app.is_dark();
        let mut sent_messages = 0;

        loop {
            match next_action(&mut app) {
                Action::Message(message) => {
                    sent_messages += 1;
                    assert!(sent_messages <= 6, "functional scenario did not finish");
                    let _ = update(&mut app, message);
                }
                Action::Exit => break,
                Action::Wait => panic!("finished fixture should not make the scenario wait"),
            }
        }

        assert_eq!(sent_messages, 6);
        assert!(matches!(app.scan, ScanState::Idle));
        assert!(app.tree.is_none());
        assert!(app.pending_delete.is_none());
        assert_ne!(app.is_dark(), initial_dark);
    }

    #[test]
    fn scenario_waits_for_the_scan_to_finish() {
        let mut app = app_with_scanned_fixture();
        app.scan = ScanState::Running {
            current: "/smoke-fixture".to_string(),
            files: 1,
            dirs: 1,
        };

        assert!(matches!(next_action(&mut app), Action::Wait));
    }

    #[test]
    fn frame_applies_navigation_before_returning() {
        let mut app = app_with_scanned_fixture();

        let _ = super::on_frame(&mut app);

        assert_eq!(app.current.0, 3, "directory message was only queued");
    }

    #[test]
    fn navigation_regression_stops_the_scenario() {
        let mut app = app_with_scanned_fixture();
        let Action::Message(Message::BrickPressed(directory)) = next_action(&mut app) else {
            panic!("scan completion should press the fixture directory");
        };

        let _ = update(&mut app, Message::GoUp);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = next_action(&mut app);
        }));

        assert!(
            result.is_err(),
            "missing directory navigation was not detected"
        );
        assert_eq!(app.current.0, 0);
        assert_ne!(directory.0, 0);
    }
}
