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
    }
}

fn root(app: &App) -> NodeId {
    app.tree.as_ref().expect("scenario tree is missing").root
}

fn child(app: &App, parent: NodeId, name: &str) -> NodeId {
    let tree = app.tree.as_ref().expect("scenario tree is missing");
    tree.node(parent)
        .children
        .iter()
        .copied()
        .find(|&id| tree.node(id).name.as_ref() == name)
        .unwrap_or_else(|| panic!("fixture child {name:?} is missing"))
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use super::{Action, Stage, next_action};
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
