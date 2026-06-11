mod disk;
mod diskmap;
mod format;
mod fs_tree;
mod history;
mod scanner;
mod treemap;

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::sync::atomic::{AtomicBool, Ordering};

use iced::theme::{Mode, Palette};
use iced::widget::{
    button, canvas, center, column, container, mouse_area, opaque, progress_bar, responsive, row,
    stack, svg, text, text_input, tooltip,
};
use iced::{
    Border, Center, Color, Element, Fill, Font, Padding, Rectangle, Shadow, Size, Subscription,
    Task, Theme, Vector,
};

use diskmap::DiskMap;
use fs_tree::{FsTree, NodeId};
use scanner::ScanEvent;

/// Maximum number of characters in the path bar above the map (then `/../` compression).
const PATH_BAR_MAX_CHARS: usize = 80;

/// Mouse icons for the status bar hints: the pressed button is filled.
const LMB_ICON: &[u8] = include_bytes!("../assets/lmb.svg");
const RMB_ICON: &[u8] = include_bytes!("../assets/rmb.svg");
/// Hover panel action icons.
const FOLDER_ICON: &[u8] = include_bytes!("../assets/folder.svg");
const TRASH_ICON: &[u8] = include_bytes!("../assets/trash.svg");
/// Top bar button icons: a circular arrow for Rescan, a treemap-like
/// brick layout for New scan.
const RESCAN_ICON: &[u8] = include_bytes!("../assets/rescan.svg");
const BRICKS_ICON: &[u8] = include_bytes!("../assets/bricks.svg");
/// Quick-scan folder icons on the start screen.
const HOME_ICON: &[u8] = include_bytes!("../assets/home.svg");
const DOWNLOADS_ICON: &[u8] = include_bytes!("../assets/downloads.svg");
const DESKTOP_ICON: &[u8] = include_bytes!("../assets/desktop.svg");
const DOCUMENTS_ICON: &[u8] = include_bytes!("../assets/documents.svg");

/// Approximate outer size of the hover actions panel (two icon buttons),
/// used to clamp its position inside the canvas.
const ACTIONS_WIDTH: f32 = 58.0;
const ACTIONS_HEIGHT: f32 = 30.0;

/// The light-minimal chrome: an off-white window, dark gray text, an amber
/// accent matching the folder bricks. The dark mode uses the stock `Theme::Dark`.
static LIGHT_THEME: LazyLock<Theme> = LazyLock::new(|| {
    Theme::custom(
        "Filegram Light",
        Palette {
            background: Color::from_rgb8(0xFA, 0xFA, 0xFA),
            text: Color::from_rgb8(0x33, 0x33, 0x33),
            primary: Color::from_rgb8(0xBA, 0x75, 0x17),
            ..Palette::LIGHT
        },
    )
});

struct App {
    tree: Option<Arc<FsTree>>,
    current: NodeId,
    /// The downward navigation stack — analog of the original's brickStack.
    nav_stack: Vec<NodeId>,
    active: Option<NodeId>,
    /// The node awaiting trash confirmation in the modal dialog.
    pending_delete: Option<NodeId>,
    scan: ScanState,
    /// The volume of the scan root, for the mini disk-usage bar; `None`
    /// hides the bar (no scan yet, or the OS query failed).
    disk_usage: Option<disk::DiskUsage>,
    path_input: String,
    history: history::History,
    /// Where the history is persisted; `None` disables saving (tests).
    history_file: Option<PathBuf>,
    cache: canvas::Cache,
    cancel: Arc<AtomicBool>,
    /// The system light/dark preference; the chrome theme follows it.
    theme_mode: Mode,
    /// The deletion backend: `trash::delete` in production. Tests swap in
    /// a tempdir-local delete so nothing ever reaches the OS trash.
    delete: fn(&Path) -> std::io::Result<()>,
}

enum ScanState {
    Idle,
    Running { current: String, files: u64 },
    Done,
}

#[derive(Debug, Clone)]
enum Message {
    PathChanged(String),
    HistoryPicked(String),
    StartScan,
    CancelScan,
    Scan(ScanEvent),
    BrickPressed(NodeId),
    SetActive(Option<NodeId>),
    GoBack,
    NewScan,
    Rescan,
    Reveal(NodeId),
    DeleteRequested(NodeId),
    DeleteConfirmed,
    DeleteCancelled,
    SystemThemeChanged(Mode),
    WindowFocused,
}

/// The default content of the path input: the most recent scanned path,
/// or the home directory when the history is empty.
fn initial_path(history: &history::History) -> String {
    history.latest().map(str::to_string).unwrap_or_else(|| {
        dirs::home_dir()
            .map(|home| home.display().to_string())
            .unwrap_or_else(|| ".".to_string())
    })
}

fn boot() -> (App, Task<Message>) {
    let (history, history_file) = match history::default_file() {
        Some(file) => match history::History::load(&file) {
            Ok(history) => (history, Some(file)),
            // Never save over a file we could not read: persistence is
            // disabled until the next launch.
            Err(error) => {
                eprintln!("filegram: failed to read the scan history: {error}");
                (history::History::default(), None)
            }
        },
        None => (history::History::default(), None),
    };
    (
        initial_app(history, history_file),
        iced::system::theme().map(Message::SystemThemeChanged),
    )
}

/// The initial application state. `boot` feeds it the persisted history;
/// tests pass an in-memory one with `history_file: None` so they never
/// touch the developer's real config directory.
fn initial_app(history: history::History, history_file: Option<PathBuf>) -> App {
    App {
        tree: None,
        current: NodeId(0),
        nav_stack: Vec::new(),
        active: None,
        pending_delete: None,
        scan: ScanState::Idle,
        disk_usage: None,
        path_input: initial_path(&history),
        history,
        history_file,
        cache: canvas::Cache::new(),
        cancel: Arc::new(AtomicBool::new(false)),
        theme_mode: Mode::default(),
        delete: |path| trash::delete(path).map_err(std::io::Error::other),
    }
}

fn theme(app: &App) -> Theme {
    match app.theme_mode {
        Mode::Dark => Theme::Dark,
        Mode::Light | Mode::None => LIGHT_THEME.clone(),
    }
}

fn subscription(_app: &App) -> Subscription<Message> {
    Subscription::batch([
        iced::system::theme_changes().map(Message::SystemThemeChanged),
        // There is no ready-made focus subscription, only the unfiltered
        // window::events(); listen the same way it does, for Focused alone.
        iced::event::listen_with(|event, _status, _window| match event {
            iced::Event::Window(iced::window::Event::Focused) => Some(Message::WindowFocused),
            _ => None,
        }),
    ])
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::PathChanged(path) => {
            app.path_input = path;
            Task::none()
        }
        Message::HistoryPicked(path) => {
            app.path_input = path;
            update(app, Message::StartScan)
        }
        Message::StartScan => {
            // Normalize once, with the same rules the history applies, so the
            // input, the scan, the progress header and the history all see
            // the same path ("/tmp/" scans and is recorded as "/tmp").
            app.path_input = history::normalize(&app.path_input).to_string();
            // Only directories that exist enter the history: a typo'd path
            // must not become a clickable entry and the next-launch prefill.
            if std::path::Path::new(&app.path_input).is_dir() {
                app.history.push(&app.path_input);
                if let Some(file) = &app.history_file
                    && let Err(error) = app.history.save(file)
                {
                    eprintln!("filegram: failed to save the scan history: {error}");
                }
            }
            app.disk_usage = disk::usage(Path::new(&app.path_input));
            app.cancel = Arc::new(AtomicBool::new(false));
            app.scan = ScanState::Running {
                current: app.path_input.clone(),
                files: 0,
            };
            app.tree = None;
            app.current = NodeId(0);
            app.nav_stack.clear();
            app.active = None;
            app.pending_delete = None;
            app.cache.clear();
            Task::run(
                scanner::start_scan(PathBuf::from(&app.path_input), app.cancel.clone()),
                Message::Scan,
            )
        }
        Message::CancelScan => {
            app.cancel.store(true, Ordering::Relaxed);
            Task::none()
        }
        Message::Scan(event) => {
            match event {
                ScanEvent::Progress { current, files } => {
                    if let ScanState::Running { .. } = app.scan {
                        app.scan = ScanState::Running { current, files };
                    }
                }
                // A late snapshot arriving after Finished is ignored.
                ScanEvent::Snapshot(tree) => {
                    if let ScanState::Running { .. } = app.scan {
                        app.tree = Some(tree);
                        app.cache.clear();
                    }
                }
                ScanEvent::Finished(tree) => {
                    // NodeIds are stable across snapshots: keep any navigation
                    // made during the scan.
                    if app.tree.is_none() {
                        app.current = tree.root;
                    }
                    // A long scan leaves the start-of-scan reading stale;
                    // re-query the volume so the bar matches the final map.
                    app.disk_usage = root_usage(&tree);
                    app.tree = Some(tree);
                    app.scan = ScanState::Done;
                    app.cache.clear();
                }
            }
            Task::none()
        }
        Message::BrickPressed(id) => {
            let Some(tree) = &app.tree else {
                return Task::none();
            };
            let node = tree.node(id);
            if node.is_dir {
                // An empty folder is ignored (a Toast in the original).
                if !node.children.is_empty() {
                    app.nav_stack.push(app.current);
                    app.current = id;
                    app.active = None;
                    app.cache.clear();
                }
            } else {
                // A file is opened with an external application.
                let _ = open::that_detached(node.path.as_os_str());
            }
            Task::none()
        }
        Message::SetActive(id) => {
            // The highlight lives outside the map cache — no clear needed.
            app.active = id;
            Task::none()
        }
        Message::GoBack => {
            if let Some(previous) = app.nav_stack.pop() {
                app.current = previous;
                app.active = None;
                app.cache.clear();
            }
            Task::none()
        }
        Message::NewScan => {
            app.scan = ScanState::Idle;
            Task::none()
        }
        Message::Rescan => {
            // Rescan the root the map was built from — not the directory
            // currently navigated to and not whatever the input holds.
            let Some(tree) = &app.tree else {
                return Task::none();
            };
            app.path_input = tree.node(tree.root).path.display().to_string();
            update(app, Message::StartScan)
        }
        Message::Reveal(id) => {
            if let Some(tree) = &app.tree
                && let Err(error) = opener::reveal(tree.node(id).path.as_ref())
            {
                eprintln!("filegram: failed to reveal in the file manager: {error}");
            }
            Task::none()
        }
        Message::DeleteRequested(id) => {
            // The UI disables deletion mid-scan; the same invariant is
            // enforced here so no other path can desync the tree from the
            // scanner's arena. A loaded tree is required for the same
            // reason: the modal renders the target node from it.
            if matches!(app.scan, ScanState::Done) && app.tree.is_some() {
                app.pending_delete = Some(id);
            }
            Task::none()
        }
        Message::DeleteCancelled => {
            app.pending_delete = None;
            Task::none()
        }
        Message::WindowFocused => {
            // The volume drifts while the app is in the background (other
            // programs write and delete); refresh the bar when the user
            // comes back to a finished map. Mid-scan readings stay owned
            // by StartScan/Finished — the bar is hidden until then anyway.
            if matches!(app.scan, ScanState::Done)
                && let Some(tree) = &app.tree
            {
                app.disk_usage = root_usage(tree);
            }
            Task::none()
        }
        Message::SystemThemeChanged(mode) => {
            // The map cache invalidates itself: the canvas state tracks the
            // dark-mode flag of the last drawn frame.
            app.theme_mode = mode;
            Task::none()
        }
        Message::DeleteConfirmed => {
            let Some(id) = app.pending_delete.take() else {
                return Task::none();
            };
            // Filesystem mutation requires a finished scan: the snapshot
            // must not be edited while the scanner still produces them.
            if !matches!(app.scan, ScanState::Done) {
                return Task::none();
            }
            let Some(tree) = app.tree.as_mut() else {
                return Task::none();
            };
            let path = tree.node(id).path.clone();
            if let Err(error) = (app.delete)(path.as_ref()) {
                eprintln!(
                    "filegram: failed to move {} to trash: {error}",
                    path.display()
                );
                return Task::none();
            }
            // The hit-test only yields direct children of `current`, so the
            // deleted node is removed from `current` and the sizes of the
            // ancestors on the navigation stack are adjusted. The Arc is
            // mutated in place when uniquely owned (the scanner is done and
            // its snapshots are dropped); make_mut clones only if shared.
            Arc::make_mut(tree).remove_child(app.current, id, &app.nav_stack);
            // The reading on the bar went stale the moment the entry left
            // the volume; re-query like Finished does.
            app.disk_usage = root_usage(tree);
            app.active = None;
            app.cache.clear();
            Task::none()
        }
    }
}

/// A fresh reading of the volume the scanned tree lives on. `None` (the
/// volume is gone, an IO error) hides the bar instead of keeping a stale
/// number.
fn root_usage(tree: &FsTree) -> Option<disk::DiskUsage> {
    disk::usage(&tree.node(tree.root).path)
}

/// An outline chrome button from the light-minimal mockup: transparent fill,
/// a thin gray border, the regular text color.
fn chrome_button(theme: &Theme, status: button::Status) -> button::Style {
    let palette = theme.extended_palette();
    let border_color = if palette.is_dark {
        Color::from_rgb8(0x55, 0x55, 0x55)
    } else {
        Color::from_rgb8(0xCC, 0xCC, 0xCC)
    };
    let background = matches!(
        status,
        button::Status::Hovered | button::Status::Pressed
    )
    .then(|| palette.background.weak.color.into());
    let text_color = if matches!(status, button::Status::Disabled) {
        muted_color(theme)
    } else {
        palette.background.base.text
    };
    button::Style {
        background,
        text_color,
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 4.0.into(),
        },
        ..button::Style::default()
    }
}

/// Top and status bars: a surface lifted from the window background.
fn bar_style(theme: &Theme) -> container::Style {
    let background = if theme.extended_palette().is_dark {
        Color::from_rgb8(0x33, 0x33, 0x33)
    } else {
        Color::WHITE
    };
    container::Style {
        background: Some(background.into()),
        ..container::Style::default()
    }
}

/// The floating hint of an action button: a card lifted above the panel
/// with a thin border and a soft shadow.
fn tooltip_style(theme: &Theme) -> container::Style {
    let is_dark = theme.extended_palette().is_dark;
    let (background, border_color) = if is_dark {
        (
            Color::from_rgb8(0x3A, 0x3A, 0x3A),
            Color::from_rgb8(0x55, 0x55, 0x55),
        )
    } else {
        (Color::WHITE, Color::from_rgb8(0xCC, 0xCC, 0xCC))
    };
    container::Style {
        background: Some(background.into()),
        text_color: Some(theme.extended_palette().background.base.text),
        border: Border {
            color: border_color,
            width: 1.0,
            radius: 6.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba8(0x00, 0x00, 0x00, 0.25),
            offset: Vector::new(0.0, 2.0),
            blur_radius: 8.0,
        },
        ..container::Style::default()
    }
}

/// Secondary chrome text: the path bar, the status bar labels.
fn muted_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(muted_color(theme)),
    }
}

fn muted_color(theme: &Theme) -> Color {
    if theme.extended_palette().is_dark {
        Color::from_rgb8(0xAA, 0xAA, 0xAA)
    } else {
        Color::from_rgb8(0x77, 0x77, 0x77)
    }
}

fn view(app: &App) -> Element<'_, Message> {
    match &app.scan {
        ScanState::Idle => idle_view(app),
        ScanState::Running { current, files } => running_view(app, current, *files),
        ScanState::Done => map_view(app),
    }
}

fn idle_view(app: &App) -> Element<'_, Message> {
    let mut content = column![
        text("Filegram — disk map").size(28),
        row![
            text_input("Directory path…", &app.path_input)
                .on_input(Message::PathChanged)
                .on_submit(Message::StartScan),
            button(text("Scan"))
                .style(chrome_button)
                .on_press(Message::StartScan),
        ]
        .spacing(8),
    ]
    .spacing(16)
    .max_width(600);
    if let Some(quick) = quick_scans() {
        content = content.push(quick);
    }
    if !app.history.entries().is_empty() {
        content = content.push(recent_scans(app));
    }
    center(content).into()
}

/// Quick scans of the standard user folders, between the scan row and the
/// history: a click scans the folder exactly like a history entry. A folder
/// the OS cannot locate is omitted; `None` when none can be, so the idle
/// screen does not reserve a blank gap for an empty row.
fn quick_scans<'a>() -> Option<Element<'a, Message>> {
    let folders: [(&[u8], &str, Option<PathBuf>); 4] = [
        (HOME_ICON, "Home", dirs::home_dir()),
        (DOWNLOADS_ICON, "Downloads", dirs::download_dir()),
        (DESKTOP_ICON, "Desktop", dirs::desktop_dir()),
        (DOCUMENTS_ICON, "Documents", dirs::document_dir()),
    ];
    let buttons: Vec<Element<'a, Message>> = folders
        .into_iter()
        .filter_map(|(icon, name, path)| {
            path.map(|path| {
                button(
                    row![themed_icon(icon).width(16).height(16), text(name).size(14)]
                        .spacing(6)
                        .align_y(Center),
                )
                .style(button::text)
                .padding(4)
                .on_press(Message::HistoryPicked(path.display().to_string()))
                .into()
            })
        })
        .collect();
    (!buttons.is_empty()).then(|| row(buttons).spacing(8).into())
}

/// The scan history under the path input: a click rescans the path.
fn recent_scans(app: &App) -> Element<'_, Message> {
    column![text("Recent scans").size(14).style(muted_text)]
        .spacing(2)
        .extend(app.history.entries().iter().map(|path| {
            button(text(format::shorten_path(path, PATH_BAR_MAX_CHARS)).size(14))
                .style(button::text)
                .padding(4)
                .on_press(Message::HistoryPicked(path.clone()))
                .into()
        }))
        .into()
}

/// The scan progress label. Monospace digits keep the width a function of the
/// digit count alone, and the counter only grows — so the label can widen but
/// never shrinks, and the path next to it does not jitter.
fn scan_label<'a>(files: u64, size: f32) -> Element<'a, Message> {
    row![
        text("Scanning… files: ").size(size),
        text(files.to_string()).size(size).font(Font::MONOSPACE),
    ]
    .align_y(Center)
    .into()
}

/// Scan screen: a counter until the first snapshot, after that the map grows
/// right as the scan proceeds (navigating it already works: NodeIds are stable).
fn running_view<'a>(app: &'a App, current: &str, files: u64) -> Element<'a, Message> {
    if app.tree.is_none() {
        return center(
            column![
                scan_label(files, 20.0),
                text(format::shorten_path(current, PATH_BAR_MAX_CHARS)).style(muted_text),
                button(text("Cancel"))
                    .style(chrome_button)
                    .on_press(Message::CancelScan),
            ]
            .spacing(16),
        )
        .into();
    }
    let bar = container(
        row![
            scan_label(files, 16.0),
            container(text(format::shorten_path(current, PATH_BAR_MAX_CHARS)).style(muted_text))
                .width(Fill)
                .padding(8),
            button(text("Cancel"))
                .style(chrome_button)
                .on_press(Message::CancelScan),
        ]
        .spacing(8)
        .align_y(Center),
    )
    .padding(8)
    .style(bar_style);
    column![bar, map_canvas(app), status_bar(app)].into()
}

fn map_view(app: &App) -> Element<'_, Message> {
    let Some(tree) = &app.tree else {
        return idle_view(app);
    };
    let current_path = tree.node(app.current).path.display().to_string();
    // Equal-width side zones keep the disk-usage bar dead center between
    // the navigation controls. The bar only appears with the final map:
    // mid-scan readings would drift while the map is still growing.
    let mut top = row![
        row![
            button(text("← Back"))
                .style(chrome_button)
                .on_press_maybe((!app.nav_stack.is_empty()).then_some(Message::GoBack)),
            container(
                text(format::shorten_path(&current_path, PATH_BAR_MAX_CHARS)).style(muted_text)
            )
            .padding(8),
        ]
        .spacing(8)
        .align_y(Center)
        .width(Fill),
    ]
    .spacing(8)
    .align_y(Center);
    if let Some(usage_bar) = disk_usage_bar(app) {
        top = top.push(usage_bar);
    }
    let bar = container(
        top.push(
            container(
                row![
                    chrome_icon_button(RESCAN_ICON, "Rescan", Message::Rescan),
                    chrome_icon_button(BRICKS_ICON, "New scan", Message::NewScan),
                ]
                .spacing(8),
            )
            .width(Fill)
            .align_x(iced::Right),
        ),
    )
    .padding(8)
    .style(bar_style);

    let content = column![bar, map_canvas(app), status_bar(app)];
    // The confirmation dialog covers the whole window with an opaque dimmed
    // backdrop; a click outside the card cancels.
    let Some(target) = app.pending_delete else {
        return content.into();
    };
    stack![
        content,
        opaque(
            mouse_area(center(opaque(delete_dialog(app, target))).style(|_theme| {
                container::Style {
                    background: Some(
                        Color {
                            a: 0.6,
                            ..Color::BLACK
                        }
                        .into(),
                    ),
                    ..container::Style::default()
                }
            }))
            .on_press(Message::DeleteCancelled)
        ),
    ]
    .into()
}

/// The card of the trash confirmation dialog: what is being deleted and
/// the Cancel / Move to Trash buttons.
fn delete_dialog(app: &App, target: NodeId) -> Element<'_, Message> {
    let tree = app.tree.as_ref().expect("delete_dialog requires a tree");
    let node = tree.node(target);
    let kind = if node.is_dir { "Folder" } else { "File" };
    container(
        column![
            text("Move to trash?").size(20),
            text(format!(
                "{kind} \"{}\" — {}",
                node.name,
                format::human_size(node.size)
            )),
            row![
                button(text("Cancel"))
                    .style(chrome_button)
                    .on_press(Message::DeleteCancelled),
                button(text("Move to Trash"))
                    .style(button::danger)
                    .on_press(Message::DeleteConfirmed),
            ]
            .spacing(8),
        ]
        .spacing(16)
        .align_x(Center),
    )
    .padding(24)
    .max_width(480)
    .style(container::rounded_box)
    .into()
}

/// Bottom status bar: on the left — the active node (or the current folder)
/// with its size, on the right — mouse button hints.
fn status_bar(app: &App) -> Element<'_, Message> {
    let tree = app.tree.as_ref().expect("status_bar requires a tree");
    let node = tree.node(app.active.unwrap_or(app.current));
    let size_label = format!("{} — {}", node.name, format::human_size(node.size));
    container(
        row![
            container(text(size_label).size(14).style(muted_text)).width(Fill),
            mouse_hint(LMB_ICON, "select"),
            mouse_hint(RMB_ICON, "back"),
        ]
        .spacing(16)
        .align_y(Center),
    )
    .padding(8)
    .style(bar_style)
    .into()
}

/// The mini disk-usage bar: how full the scan root's volume is.
/// `None` when no volume has been queried yet — the bar is omitted.
fn disk_usage_bar(app: &App) -> Option<Element<'_, Message>> {
    let usage = app.disk_usage?;
    let label = format!(
        "Disk: {} / {}",
        format::human_size(usage.used),
        format::human_size(usage.total)
    );
    Some(
        row![
            text(label).size(14).style(muted_text),
            progress_bar(0.0..=1.0, usage.fraction())
                .length(140)
                .girth(6),
        ]
        .spacing(8)
        .align_y(Center)
        .into(),
    )
}

/// The hover actions panel pinned to the active brick's top-right corner,
/// clamped to the canvas bounds.
fn brick_actions(app: &App, target: NodeId, brick: Rectangle, bounds: Size) -> Element<'_, Message> {
    // Deletion needs a finished scan: removing entries mid-scan would
    // desync the tree from the scanner's arena.
    let deletable = matches!(app.scan, ScanState::Done).then_some(target);
    let panel = container(
        row![
            action_button(
                FOLDER_ICON,
                "Open in file manager",
                Some(Message::Reveal(target)),
            ),
            action_button(
                TRASH_ICON,
                "Move to trash",
                deletable.map(Message::DeleteRequested),
            ),
        ]
        .spacing(2),
    )
    .padding(2);
    let x = (brick.x + brick.width - ACTIONS_WIDTH)
        .max(brick.x)
        .min(bounds.width - ACTIONS_WIDTH)
        .max(0.0);
    let y = brick.y.min(bounds.height - ACTIONS_HEIGHT).max(0.0);
    container(panel)
        .padding(Padding {
            top: y,
            left: x,
            ..Padding::ZERO
        })
        .into()
}

/// An outline chrome button with a leading icon: the Rescan / New scan pair.
fn chrome_icon_button<'a>(
    icon: &'static [u8],
    label: &'a str,
    on_press: Message,
) -> Element<'a, Message> {
    button(
        row![themed_icon(icon).width(16).height(16), text(label)]
            .spacing(6)
            .align_y(Center),
    )
    .style(chrome_button)
    .on_press(on_press)
    .into()
}

/// A status bar action: an icon button with a tooltip.
fn action_button<'a>(
    icon: &'static [u8],
    tip: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    tooltip(
        button(themed_icon(icon).width(18).height(18))
            .padding(4)
            .style(button::text)
            .on_press_maybe(on_press),
        text(tip).size(12),
        tooltip::Position::Top,
    )
    .style(tooltip_style)
    .padding(8)
    .gap(6)
    .into()
}

/// A mouse button hint: a mouse icon with the pressed button filled, plus the action.
fn mouse_hint<'a>(icon: &'static [u8], action: &'a str) -> Element<'a, Message> {
    row![
        themed_icon(icon).width(15).height(20),
        text(action).size(14).style(muted_text)
    ]
    .spacing(6)
    .align_y(Center)
    .into()
}

/// An embedded SVG icon tinted with the theme's text color.
/// `Svg` is invariant over its lifetime, so the caller picks it.
fn themed_icon<'a>(icon: &'static [u8]) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(icon)).style(|theme: &Theme, _status| svg::Style {
        color: Some(theme.palette().text),
    })
}

/// The map canvas with the hover actions panel stacked on top of the active
/// brick; call only when `app.tree.is_some()`.
fn map_canvas(app: &App) -> Element<'_, Message> {
    let tree = app.tree.as_ref().expect("map_canvas requires a tree");
    responsive(move |size| {
        let map = canvas(DiskMap {
            tree,
            current: app.current,
            active: app.active,
            cache: &app.cache,
        })
        .width(Fill)
        .height(Fill);
        let actions = app.active.and_then(|active| {
            diskmap::level1(tree, app.current, size)
                .into_iter()
                .find(|&(id, _)| id == active)
                // A brick too small for a caption gets no actions panel either.
                .filter(|&(id, rect)| diskmap::has_label(tree, id, rect))
                .map(|(_, rect)| brick_actions(app, active, rect, size))
        });
        match actions {
            Some(panel) => stack![map, panel].into(),
            None => map.into(),
        }
    })
    .into()
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("Filegram")
        .theme(theme)
        .subscription(subscription)
        .window(iced::window::Settings {
            size: Size::new(1024.0, 768.0),
            // Raw 64x64 RGBA pixels pre-rendered from assets/icon/icon.svg:
            // `icon::from_rgba` needs no image decoder, unlike `from_file_data`
            // which would pull the whole `image` feature for one PNG.
            icon: Some(
                iced::window::icon::from_rgba(
                    include_bytes!("../assets/icon/icon-64.rgba").to_vec(),
                    64,
                    64,
                )
                .expect("assets/icon/icon-64.rgba must hold exactly 64x64 RGBA pixels"),
            ),
            ..Default::default()
        })
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A disk-isolated `App`: an empty in-memory history, no history file.
    fn test_app() -> App {
        initial_app(history::History::default(), None)
    }

    /// A scan root that exits immediately: a missing child of a fresh temp
    /// dir, so the threads spawned by `StartScan` find nothing to traverse.
    /// The guard keeps the temp dir alive for the duration of the test.
    fn missing_scan_root() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").display().to_string();
        (dir, path)
    }

    /// [`test_app`] with a finished scan and a loaded (root-only) tree —
    /// deletion is only legal in that state.
    fn scanned_app() -> App {
        scanned_app_at(Path::new("/root"))
    }

    /// [`scanned_app`] rooted at `path`, for tests that need the root
    /// volume query to succeed (a real dir) or fail (a missing one).
    fn scanned_app_at(path: &Path) -> App {
        let mut app = test_app();
        app.scan = ScanState::Done;
        app.tree = Some(Arc::new(FsTree::from_arena(&[fs_tree::ScanNode {
            name: "root".into(),
            path: path.into(),
            size: 0,
            is_dir: true,
            parent: 0,
        }])));
        app
    }

    /// A reading no real volume can produce: any re-query replaces it
    /// with `total` far above 2, and a failed one with `None`.
    fn stale_usage() -> disk::DiskUsage {
        disk::DiskUsage { used: 1, total: 2 }
    }

    #[test]
    fn delete_requires_confirmation() {
        let mut app = scanned_app();
        let _ = update(&mut app, Message::DeleteRequested(NodeId(3)));
        assert_eq!(app.pending_delete, Some(NodeId(3)));

        let _ = update(&mut app, Message::DeleteCancelled);
        assert_eq!(app.pending_delete, None);
    }

    #[test]
    fn delete_request_ignored_until_scan_finishes() {
        let mut app = test_app();
        for scan in [
            ScanState::Idle,
            ScanState::Running {
                current: String::new(),
                files: 0,
            },
        ] {
            app.scan = scan;
            let _ = update(&mut app, Message::DeleteRequested(NodeId(3)));
            assert_eq!(app.pending_delete, None);
        }
    }

    #[test]
    fn delete_request_ignored_without_tree() {
        let mut app = scanned_app();
        app.tree = None;
        let _ = update(&mut app, Message::DeleteRequested(NodeId(3)));
        assert_eq!(app.pending_delete, None);
    }

    #[test]
    fn confirm_without_tree_clears_pending() {
        let mut app = scanned_app();
        let _ = update(&mut app, Message::DeleteRequested(NodeId(3)));
        app.tree = None;
        let _ = update(&mut app, Message::DeleteConfirmed);
        assert_eq!(app.pending_delete, None);
    }

    #[test]
    fn rescan_restarts_scan_at_tree_root() {
        // A real (empty) dir as the scan root: the rescan threads exit
        // immediately, and the path survives StartScan's existence check.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().display().to_string();
        let mut app = test_app();
        app.scan = ScanState::Done;
        app.tree = Some(Arc::new(FsTree::from_arena(&[
            fs_tree::ScanNode {
                name: "root".into(),
                path: dir.path().into(),
                size: 0,
                is_dir: true,
                parent: 0,
            },
            fs_tree::ScanNode {
                name: "sub".into(),
                path: dir.path().join("sub").into(),
                size: 0,
                is_dir: true,
                parent: 0,
            },
        ])));
        // Navigated into a subdirectory with a stale input: the rescan
        // must use the scan root, not either of those.
        app.current = NodeId(1);
        app.path_input = "/somewhere/else".into();
        let _ = update(&mut app, Message::Rescan);
        assert_eq!(app.path_input, root);
        assert!(matches!(&app.scan, ScanState::Running { current, .. } if *current == root));
        app.cancel.store(true, Ordering::Relaxed);
    }

    #[test]
    fn rescan_ignored_without_tree() {
        let mut app = scanned_app();
        app.tree = None;
        let _ = update(&mut app, Message::Rescan);
        assert!(matches!(app.scan, ScanState::Done));
    }

    #[test]
    fn theme_follows_system_mode() {
        let mut app = test_app();
        let _ = update(&mut app, Message::SystemThemeChanged(Mode::Dark));
        assert_eq!(theme(&app), Theme::Dark);
        let _ = update(&mut app, Message::SystemThemeChanged(Mode::Light));
        assert_eq!(theme(&app), *LIGHT_THEME);
    }

    #[test]
    fn initial_path_prefers_latest_history_entry() {
        let mut history = history::History::default();
        history.push("/scans/latest");
        assert_eq!(initial_path(&history), "/scans/latest");
        // An empty history falls back to a usable default.
        assert_ne!(initial_path(&history::History::default()), "/scans/latest");
        assert!(!initial_path(&history::History::default()).is_empty());
    }

    #[test]
    fn start_scan_normalizes_input_and_records_existing_dir() {
        let mut app = test_app();
        // An (empty) existing directory: the scan threads exit immediately.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().display().to_string();
        app.path_input = format!("  {path}/  ");
        let _ = update(&mut app, Message::StartScan);
        // The input is normalized once (trim + trailing separator); the
        // input, the scan and the history see the same path.
        assert_eq!(app.path_input, path);
        assert_eq!(app.history.latest(), Some(path.as_str()));
        app.cancel.store(true, Ordering::Relaxed);
    }

    #[test]
    fn start_scan_skips_history_for_missing_path() {
        let mut app = test_app();
        let (_guard, root) = missing_scan_root();
        app.path_input = root;
        let _ = update(&mut app, Message::StartScan);
        // A path that does not exist must not become a history entry.
        assert_eq!(app.history.latest(), None);
        app.cancel.store(true, Ordering::Relaxed);
    }

    #[test]
    fn start_scan_queries_disk_usage_for_existing_dir() {
        let mut app = test_app();
        let dir = tempfile::tempdir().unwrap();
        app.path_input = dir.path().display().to_string();
        let _ = update(&mut app, Message::StartScan);
        let usage = app.disk_usage.expect("an existing dir has a volume");
        assert!(usage.total > 0);
        app.cancel.store(true, Ordering::Relaxed);
    }

    #[test]
    fn start_scan_drops_disk_usage_for_missing_path() {
        let mut app = test_app();
        // A stale reading from a previous scan must not survive a scan of
        // a path whose volume cannot be queried.
        app.disk_usage = Some(disk::DiskUsage {
            used: 1,
            total: 2,
        });
        let (_guard, root) = missing_scan_root();
        app.path_input = root;
        let _ = update(&mut app, Message::StartScan);
        assert_eq!(app.disk_usage, None);
        app.cancel.store(true, Ordering::Relaxed);
    }

    #[test]
    fn history_pick_fills_input_and_starts_scan() {
        let mut app = test_app();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().display().to_string();
        let _ = update(&mut app, Message::HistoryPicked(path.clone()));
        assert_eq!(app.path_input, path);
        assert!(matches!(app.scan, ScanState::Running { .. }));
        assert_eq!(app.history.latest(), Some(path.as_str()));
        app.cancel.store(true, Ordering::Relaxed);
    }

    #[test]
    fn window_focus_refreshes_disk_usage_on_finished_map() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = scanned_app_at(dir.path());
        app.disk_usage = Some(stale_usage());
        let _ = update(&mut app, Message::WindowFocused);
        let usage = app.disk_usage.expect("a temp dir lives on a real volume");
        assert!(usage.total > 2);
    }

    #[test]
    fn window_focus_hides_bar_when_volume_query_fails() {
        let (_guard, root) = missing_scan_root();
        let mut app = scanned_app_at(Path::new(&root));
        app.disk_usage = Some(stale_usage());
        let _ = update(&mut app, Message::WindowFocused);
        // The volume is gone: hiding the bar beats showing a stale number,
        // exactly like `ScanEvent::Finished`.
        assert_eq!(app.disk_usage, None);
    }

    #[test]
    fn window_focus_ignored_before_map_is_finished() {
        let mut app = test_app();
        for scan in [
            ScanState::Idle,
            ScanState::Running {
                current: String::new(),
                files: 0,
            },
        ] {
            app.scan = scan;
            app.disk_usage = Some(stale_usage());
            let _ = update(&mut app, Message::WindowFocused);
            assert_eq!(app.disk_usage, Some(stale_usage()));
        }
    }

    #[test]
    fn window_focus_ignored_without_tree() {
        let mut app = scanned_app();
        app.tree = None;
        app.disk_usage = Some(stale_usage());
        let _ = update(&mut app, Message::WindowFocused);
        assert_eq!(app.disk_usage, Some(stale_usage()));
    }

    #[test]
    fn confirmed_delete_refreshes_disk_usage() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim");
        std::fs::write(&victim, b"junk").unwrap();
        let mut app = test_app();
        app.scan = ScanState::Done;
        app.tree = Some(Arc::new(FsTree::from_arena(&[
            fs_tree::ScanNode {
                name: "root".into(),
                path: dir.path().into(),
                size: 4,
                is_dir: true,
                parent: 0,
            },
            fs_tree::ScanNode {
                name: "victim".into(),
                path: victim.as_path().into(),
                size: 4,
                is_dir: false,
                parent: 0,
            },
        ])));
        // A tempdir-local delete keeps the test hermetic: the real backend
        // would move the victim into the developer's OS trash.
        app.delete = |path| std::fs::remove_file(path);
        app.disk_usage = Some(stale_usage());
        let _ = update(&mut app, Message::DeleteRequested(NodeId(1)));
        let _ = update(&mut app, Message::DeleteConfirmed);
        assert!(!victim.exists(), "the victim is gone");
        let usage = app.disk_usage.expect("the temp dir's volume is alive");
        assert!(usage.total > 2);
    }

    #[test]
    fn failed_delete_keeps_disk_usage() {
        // The deletion fails, so nothing on the volume changed and the
        // reading must not move either — a real dir proves no re-query
        // happened (one would replace the stale reading).
        let dir = tempfile::tempdir().unwrap();
        let mut app = scanned_app_at(dir.path());
        app.delete = |_| Err(std::io::Error::other("denied"));
        app.disk_usage = Some(stale_usage());
        let _ = update(&mut app, Message::DeleteRequested(NodeId(0)));
        let _ = update(&mut app, Message::DeleteConfirmed);
        assert_eq!(app.disk_usage, Some(stale_usage()));
    }

    #[test]
    fn new_scan_drops_pending_delete() {
        let mut app = scanned_app();
        let (_guard, root) = missing_scan_root();
        app.path_input = root;
        let _ = update(&mut app, Message::DeleteRequested(NodeId(3)));
        let _ = update(&mut app, Message::StartScan);
        assert_eq!(app.pending_delete, None);
        app.cancel.store(true, Ordering::Relaxed);
    }
}
