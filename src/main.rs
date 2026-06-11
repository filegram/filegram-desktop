mod diskmap;
mod format;
mod fs_tree;
mod scanner;
mod treemap;

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::sync::atomic::{AtomicBool, Ordering};

use iced::theme::{Mode, Palette};
use iced::widget::{
    button, canvas, center, column, container, mouse_area, opaque, responsive, row, stack, svg,
    text, text_input, tooltip,
};
use iced::{
    Border, Center, Color, Element, Fill, Padding, Rectangle, Shadow, Size, Subscription, Task,
    Theme, Vector,
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
    path_input: String,
    cache: canvas::Cache,
    cancel: Arc<AtomicBool>,
    /// The system light/dark preference; the chrome theme follows it.
    theme_mode: Mode,
}

enum ScanState {
    Idle,
    Running { current: String, files: u64 },
    Done,
}

#[derive(Debug, Clone)]
enum Message {
    PathChanged(String),
    StartScan,
    CancelScan,
    Scan(ScanEvent),
    BrickPressed(NodeId),
    SetActive(Option<NodeId>),
    GoBack,
    NewScan,
    Reveal(NodeId),
    DeleteRequested(NodeId),
    DeleteConfirmed,
    DeleteCancelled,
    SystemThemeChanged(Mode),
}

fn boot() -> (App, Task<Message>) {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    (
        App {
            tree: None,
            current: NodeId(0),
            nav_stack: Vec::new(),
            active: None,
            pending_delete: None,
            scan: ScanState::Idle,
            path_input: home,
            cache: canvas::Cache::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            theme_mode: Mode::default(),
        },
        iced::system::theme().map(Message::SystemThemeChanged),
    )
}

fn theme(app: &App) -> Theme {
    match app.theme_mode {
        Mode::Dark => Theme::Dark,
        Mode::Light | Mode::None => LIGHT_THEME.clone(),
    }
}

fn subscription(_app: &App) -> Subscription<Message> {
    iced::system::theme_changes().map(Message::SystemThemeChanged)
}

fn update(app: &mut App, message: Message) -> Task<Message> {
    match message {
        Message::PathChanged(path) => {
            app.path_input = path;
            Task::none()
        }
        Message::StartScan => {
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
            // scanner's arena.
            if matches!(app.scan, ScanState::Done) {
                app.pending_delete = Some(id);
            }
            Task::none()
        }
        Message::DeleteCancelled => {
            app.pending_delete = None;
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
            let Some(tree) = &app.tree else {
                return Task::none();
            };
            let path = tree.node(id).path.clone();
            if let Err(error) = trash::delete(path.as_ref()) {
                eprintln!(
                    "filegram: failed to move {} to trash: {error}",
                    path.display()
                );
                return Task::none();
            }
            // The hit-test only yields direct children of `current`, so the
            // deleted node is removed from `current` and the sizes of the
            // ancestors on the navigation stack are adjusted.
            let mut updated = FsTree::clone(tree);
            if updated.remove_child(app.current, id, &app.nav_stack) {
                app.tree = Some(Arc::new(updated));
            }
            app.active = None;
            app.cache.clear();
            Task::none()
        }
    }
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
    center(
        column![
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
        .max_width(600),
    )
    .into()
}

/// Scan screen: a counter until the first snapshot, after that the map grows
/// right as the scan proceeds (navigating it already works: NodeIds are stable).
fn running_view<'a>(app: &'a App, current: &str, files: u64) -> Element<'a, Message> {
    if app.tree.is_none() {
        return center(
            column![
                text(format!("Scanning… files: {files}")).size(20),
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
            text(format!("Scanning… files: {files}")),
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
    let bar = container(
        row![
            button(text("← Back"))
                .style(chrome_button)
                .on_press_maybe((!app.nav_stack.is_empty()).then_some(Message::GoBack)),
            container(
                text(format::shorten_path(&current_path, PATH_BAR_MAX_CHARS)).style(muted_text)
            )
            .width(Fill)
            .padding(8),
            button(text("New scan"))
                .style(chrome_button)
                .on_press(Message::NewScan),
        ]
        .spacing(8)
        .align_y(Center),
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
        .window_size(Size::new(1024.0, 768.0))
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `boot()` with the scan already finished — deletion is only legal then.
    fn scanned_app() -> App {
        let (mut app, _) = boot();
        app.scan = ScanState::Done;
        app
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
        let (mut app, _) = boot();
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
    fn confirm_without_tree_clears_pending() {
        let mut app = scanned_app();
        let _ = update(&mut app, Message::DeleteRequested(NodeId(3)));
        let _ = update(&mut app, Message::DeleteConfirmed);
        assert_eq!(app.pending_delete, None);
    }

    #[test]
    fn theme_follows_system_mode() {
        let (mut app, _) = boot();
        let _ = update(&mut app, Message::SystemThemeChanged(Mode::Dark));
        assert_eq!(theme(&app), Theme::Dark);
        let _ = update(&mut app, Message::SystemThemeChanged(Mode::Light));
        assert_eq!(theme(&app), *LIGHT_THEME);
    }

    #[test]
    fn new_scan_drops_pending_delete() {
        let mut app = scanned_app();
        app.path_input = std::env::temp_dir().display().to_string();
        let _ = update(&mut app, Message::DeleteRequested(NodeId(3)));
        let _ = update(&mut app, Message::StartScan);
        assert_eq!(app.pending_delete, None);
        app.cancel.store(true, Ordering::Relaxed);
    }
}
