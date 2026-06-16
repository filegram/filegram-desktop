#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod disk;
mod diskmap;
mod format;
mod fs_tree;
mod history;
mod i18n;
mod release;
mod scanner;
mod settings;
mod treemap;
mod ui;

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use iced::theme::{Mode, Palette};
use iced::widget::{
    button, canvas, center, column, container, mouse_area, opaque, progress_bar, responsive, row,
    stack, text,
};
use iced::{Center, Color, Element, Fill, Font, Rectangle, Size, Subscription, Task, Theme};

use diskmap::DiskMap;
use fs_tree::{FsTree, NodeId};
use i18n::Lang;
use scanner::ScanEvent;
use ui::brick::brick_actions;
use ui::chrome::{
    bar_style, chrome_button, chrome_icon_button, chrome_icon_only_button,
    chrome_icon_only_button_maybe, disk_usage_progress_style, muted_color, muted_icon, muted_text,
    themed_icon,
};
use ui::start::idle_view;

/// Path bar char budget before `/../` compression.
const PATH_BAR_MAX_CHARS: usize = 80;

/// Sized to the mouse-hint icon so the bar (and the canvas above it) keeps a
/// constant size whether or not hints show; a fluctuating canvas snaps the zoom tween.
const BAR_CONTENT_HEIGHT: f32 = 20.0;

const LMB_ICON: &[u8] = include_bytes!("../assets/lmb.svg");
const RMB_ICON: &[u8] = include_bytes!("../assets/rmb.svg");
const FOLDER_ICON: &[u8] = include_bytes!("../assets/folder.svg");
const TRASH_ICON: &[u8] = include_bytes!("../assets/trash.svg");
const RESCAN_ICON: &[u8] = include_bytes!("../assets/rescan.svg");
const BRICKS_ICON: &[u8] = include_bytes!("../assets/bricks.svg");
const LAYERS_ICON: &[u8] = include_bytes!("../assets/layers.svg");
const SIZE_ICON: &[u8] = include_bytes!("../assets/pie-chart.svg");
const UP_ICON: &[u8] = include_bytes!("../assets/up.svg");
const HOME_ICON: &[u8] = include_bytes!("../assets/home.svg");
const DOWNLOADS_ICON: &[u8] = include_bytes!("../assets/downloads.svg");
const DESKTOP_ICON: &[u8] = include_bytes!("../assets/desktop.svg");
const DOCUMENTS_ICON: &[u8] = include_bytes!("../assets/documents.svg");
const DRIVE_ICON: &[u8] = include_bytes!("../assets/drive.svg");
const USB_ICON: &[u8] = include_bytes!("../assets/usb.svg");
const GLOBE_ICON: &[u8] = include_bytes!("../assets/globe.svg");
const DISC_ICON: &[u8] = include_bytes!("../assets/disc.svg");
const SUN_ICON: &[u8] = include_bytes!("../assets/sun.svg");
const MOON_ICON: &[u8] = include_bytes!("../assets/moon.svg");

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
    nav_stack: Vec<NodeId>,
    active: Option<NodeId>,
    pending_delete: Option<NodeId>,
    scan: ScanState,
    /// `None` hides the disk-usage bar (no scan yet, or the OS query failed).
    disk_usage: Option<disk::DiskUsage>,
    /// Refreshed on window focus, so volumes mounted in the background show up.
    disk_roots: Vec<disk::DiskRoot>,
    path_input: String,
    history: history::History,
    /// `None` disables saving (tests).
    history_file: Option<PathBuf>,
    cache: canvas::Cache,
    cancel: Arc<AtomicBool>,
    theme_mode: Mode,
    /// Overrides the system preference, persists across launches.
    theme_override: Option<Mode>,
    system_lang: Lang,
    /// Overrides the system locale, persists across launches.
    lang_override: Option<Lang>,
    lang_menu_open: bool,
    /// Full list instead of the short one; reopening resets to short.
    lang_menu_expanded: bool,
    /// The recent-scan entry the pointer is over, by path; only it shows the cross.
    hovered_history: Option<String>,
    /// `None` disables saving (tests).
    settings_file: Option<PathBuf>,
    /// Latest GitHub release tag when it differs from the running version.
    latest_release: Option<String>,
    /// Tests swap in a tempdir-local delete so nothing reaches the OS trash.
    delete: fn(&Path) -> std::io::Result<()>,
    /// `FILEGRAM_SMOKE`: close on the first rendered frame so CI can headlessly
    /// flag a broken wgpu/window backend via a non-zero exit.
    smoke: bool,
}

enum ScanState {
    Idle,
    Running {
        current: String,
        files: u64,
        dirs: u64,
    },
    Done,
}

#[derive(Debug, Clone)]
enum Message {
    PathChanged(String),
    HistoryPicked(String),
    /// `Some(path)` on enter, `None` on leave of a recent-scan row.
    HistoryHovered(Option<String>),
    HistoryRemoved(String),
    StartScan,
    CancelScan,
    Scan(ScanEvent),
    BrickPressed(NodeId),
    SetActive(Option<NodeId>),
    GoUp,
    NewScan,
    Rescan,
    Reveal(NodeId),
    DeleteRequested(NodeId),
    DeleteConfirmed,
    DeleteCancelled,
    SystemThemeChanged(Mode),
    ToggleTheme,
    LanguageMenuToggled,
    LanguageMenuExpanded,
    LanguagePicked(Lang),
    WindowFocused,
    LatestReleaseLoaded(Option<String>),
    LatestReleasePressed,
    SmokeFrameRendered,
}

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
            // Never save over a file we could not read.
            Err(error) => {
                eprintln!("filegram: failed to read the scan history: {error}");
                (history::History::default(), None)
            }
        },
        None => (history::History::default(), None),
    };
    let mut app = initial_app(history, history_file);
    app.smoke = std::env::var_os("FILEGRAM_SMOKE").is_some();
    app.settings_file = settings::default_file();
    // Safe to save over an unreadable settings file: the next toggle or
    // language pick rewrites the whole file anyway.
    let saved = app
        .settings_file
        .as_deref()
        .map(|file| {
            settings::load(file).unwrap_or_else(|error| {
                eprintln!("filegram: failed to read the settings: {error}");
                settings::Settings::default()
            })
        })
        .unwrap_or_default();
    app.theme_override = saved.theme;
    app.lang_override = saved.lang;
    let mut tasks = vec![
        iced::system::theme().map(Message::SystemThemeChanged),
        // Runs in the background so the window opens without waiting for the network.
        Task::perform(release::fetch_latest_tag(), Message::LatestReleaseLoaded),
    ];
    // `FILEGRAM_SMOKE_PATH` drives the full StartScan flow so CI exercises the
    // scan, tree build and treemap render, not just the first frame.
    if app.smoke
        && let Some(path) = std::env::var_os("FILEGRAM_SMOKE_PATH")
    {
        app.path_input = path.to_string_lossy().into_owned();
        tasks.push(Task::done(Message::StartScan));
    }
    (app, Task::batch(tasks))
}

/// Tests pass an in-memory history with `history_file: None` so they never
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
        disk_roots: disk::mounted_roots(),
        path_input: initial_path(&history),
        history,
        history_file,
        cache: canvas::Cache::new(),
        cancel: Arc::new(AtomicBool::new(false)),
        theme_mode: Mode::default(),
        theme_override: None,
        system_lang: Lang::system(),
        lang_override: None,
        lang_menu_open: false,
        lang_menu_expanded: false,
        hovered_history: None,
        settings_file: None,
        latest_release: None,
        delete: |path| trash::delete(path).map_err(std::io::Error::other),
        smoke: false,
    }
}

impl App {
    pub(crate) fn theme(&self) -> Theme {
        if self.is_dark() {
            Theme::Dark
        } else {
            LIGHT_THEME.clone()
        }
    }

    /// Manual choice when set, system preference otherwise (`Mode::None` is light).
    pub(crate) fn is_dark(&self) -> bool {
        matches!(self.theme_override.unwrap_or(self.theme_mode), Mode::Dark)
    }

    /// Manual pick when set, system locale otherwise.
    pub(crate) fn lang(&self) -> Lang {
        self.lang_override.unwrap_or(self.system_lang)
    }

    pub(crate) fn strings(&self) -> &'static i18n::Strings {
        self.lang().strings()
    }
}

fn subscription(app: &App) -> Subscription<Message> {
    let mut subs = vec![
        iced::system::theme_changes().map(Message::SystemThemeChanged),
        // No ready-made focus subscription, so filter window events for Focused.
        iced::event::listen_with(|event, _status, _window| match event {
            iced::Event::Window(iced::window::Event::Focused) => Some(Message::WindowFocused),
            _ => None,
        }),
    ];
    if app.smoke {
        // The first frame tick means the wgpu surface drew, all the smoke test proves.
        subs.push(iced::window::frames().map(|_| Message::SmokeFrameRendered));
    }
    Subscription::batch(subs)
}

/// Persists the history if enabled. A failure is logged, not fatal.
fn save_history(app: &App) {
    if let Some(file) = &app.history_file
        && let Err(error) = app.history.save(file)
    {
        eprintln!("filegram: failed to save the scan history: {error}");
    }
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
        Message::HistoryHovered(path) => {
            app.hovered_history = path;
            Task::none()
        }
        Message::HistoryRemoved(path) => {
            app.history.remove(&path);
            // Clear the highlight in case it pointed at the entry just removed.
            if app.hovered_history.as_deref() == Some(path.as_str()) {
                app.hovered_history = None;
            }
            save_history(app);
            Task::none()
        }
        Message::StartScan => {
            // Normalize once with the history's rules so input, scan, header
            // and history all see the same path ("/tmp/" becomes "/tmp").
            app.path_input = history::normalize(&app.path_input).to_string();
            if app.path_input.is_empty() {
                return Task::none();
            }
            // Only existing directories enter the history: a typo'd path must
            // not become a clickable entry and the next-launch prefill.
            if std::path::Path::new(&app.path_input).is_dir() {
                app.history.push(&app.path_input);
                save_history(app);
            }
            app.disk_usage = disk::usage(Path::new(&app.path_input));
            app.cancel = Arc::new(AtomicBool::new(false));
            app.scan = ScanState::Running {
                current: app.path_input.clone(),
                files: 0,
                dirs: 0,
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
                ScanEvent::Progress {
                    current,
                    files,
                    dirs,
                } => {
                    if let ScanState::Running { .. } = app.scan {
                        app.scan = ScanState::Running {
                            current,
                            files,
                            dirs,
                        };
                    }
                }
                // A late snapshot arriving after Finished is ignored.
                ScanEvent::Snapshot(tree) => {
                    if let ScanState::Running { .. } = app.scan {
                        app.tree = Some(tree);
                        app.cache.clear();
                    }
                }
                // A late finish after the scan was abandoned (NewScan -> Idle) is ignored.
                ScanEvent::Finished(tree) if matches!(app.scan, ScanState::Running { .. }) => {
                    // NodeIds are stable across snapshots: keep mid-scan navigation.
                    if app.tree.is_none() {
                        app.current = tree.root;
                    }
                    // Re-query the volume so the bar matches the final map.
                    app.disk_usage = root_usage(&tree);
                    app.tree = Some(tree);
                    app.scan = ScanState::Done;
                    app.cache.clear();
                }
                ScanEvent::Finished(_) => {}
            }
            Task::none()
        }
        Message::BrickPressed(id) => {
            let Some(tree) = &app.tree else {
                return Task::none();
            };
            let node = tree.node(id);
            if node.is_dir {
                if !node.children.is_empty() {
                    app.nav_stack.push(app.current);
                    app.current = id;
                    app.active = None;
                    app.cache.clear();
                }
            } else {
                let _ = open::that_detached(node.path.as_os_str());
            }
            Task::none()
        }
        Message::SetActive(id) => {
            // The highlight lives outside the map cache, no clear needed.
            app.active = id;
            Task::none()
        }
        Message::GoUp => {
            if let Some(previous) = app.nav_stack.pop() {
                app.current = previous;
                app.active = None;
                app.cache.clear();
            }
            Task::none()
        }
        Message::NewScan => {
            // Reachable mid-scan: stop the in-flight scan so a late Finished
            // cannot drag the abandoned scan back onto the screen.
            app.cancel.store(true, Ordering::Relaxed);
            app.scan = ScanState::Idle;
            // Drop the tree and its cached geometry; the start screen unuses them.
            app.tree = None;
            app.cache.clear();
            Task::none()
        }
        Message::Rescan => {
            // Rescan the root the map was built from, not the navigated dir or input.
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
            // Deletion only after a finished scan, so nothing desyncs the tree
            // from the scanner's arena; the modal also renders the target from it.
            if matches!(&app.scan, ScanState::Done) && app.tree.is_some() {
                app.pending_delete = Some(id);
            }
            Task::none()
        }
        Message::DeleteCancelled => {
            app.pending_delete = None;
            Task::none()
        }
        Message::WindowFocused => {
            // Volumes mount and unmount in the background; refresh the disk row
            // on any screen, since the start screen is where a drive plug-in lands.
            app.disk_roots = disk::mounted_roots();
            // The volume drifts in the background; refresh the bar on a finished
            // map. Mid-scan readings stay owned by StartScan/Finished.
            if matches!(&app.scan, ScanState::Done)
                && let Some(tree) = &app.tree
            {
                app.disk_usage = root_usage(tree);
            }
            Task::none()
        }
        Message::SystemThemeChanged(mode) => {
            // The map cache invalidates itself: canvas state tracks the last
            // frame's dark-mode flag.
            app.theme_mode = mode;
            Task::none()
        }
        Message::LatestReleaseLoaded(tag) => {
            // Only show a tag that differs from the running build.
            app.latest_release =
                tag.filter(|tag| tag.trim_start_matches('v') != env!("CARGO_PKG_VERSION"));
            Task::none()
        }
        Message::LatestReleasePressed => {
            if let Some(tag) = &app.latest_release {
                let _ = open::that_detached(release::release_url(tag));
            }
            Task::none()
        }
        Message::SmokeFrameRendered => {
            // While a smoke scan (FILEGRAM_SMOKE_PATH) runs, keep going until it
            // finishes so the treemap, not just the start screen, gets drawn.
            if matches!(app.scan, ScanState::Running { .. }) {
                return Task::none();
            }
            eprintln!("filegram: smoke test rendered a frame, exiting");
            iced::exit()
        }
        Message::ToggleTheme => {
            let mode = if app.is_dark() { Mode::Light } else { Mode::Dark };
            app.theme_override = Some(mode);
            save_settings(app);
            Task::none()
        }
        Message::LanguageMenuToggled => {
            app.lang_menu_open = !app.lang_menu_open;
            // Opens short unless the current language only lives in the full list.
            app.lang_menu_expanded = !Lang::PRIMARY.contains(&app.lang());
            Task::none()
        }
        Message::LanguageMenuExpanded => {
            app.lang_menu_expanded = true;
            Task::none()
        }
        Message::LanguagePicked(lang) => {
            app.lang_override = Some(lang);
            app.lang_menu_open = false;
            save_settings(app);
            Task::none()
        }
        Message::DeleteConfirmed => {
            let Some(id) = app.pending_delete.take() else {
                return Task::none();
            };
            // No mutating a snapshot while the scanner still produces them.
            if !matches!(&app.scan, ScanState::Done) {
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
            // Removes the node from `current` and adjusts the sizes of the
            // ancestors on the nav stack. make_mut clones only if the Arc is shared.
            Arc::make_mut(tree).remove_child(app.current, id, &app.nav_stack);
            // The entry left the volume; re-query like Finished does.
            app.disk_usage = root_usage(tree);
            app.active = None;
            app.cache.clear();
            Task::none()
        }
    }
}

/// A fresh reading of the volume the scanned tree lives on. `None` (gone or
/// IO error) hides the bar instead of keeping a stale number.
fn root_usage(tree: &FsTree) -> Option<disk::DiskUsage> {
    disk::usage(&tree.node(tree.root).path)
}

/// `part` against `whole` as a percentage. Tiny shares keep more decimals so
/// they don't all collapse to "0%"; thresholds apply to the printed value, so
/// 9.96% reads "10%", never "10.0%". Zero `whole` reads "0%".
fn size_percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "0%".to_string();
    }
    let percent = part as f64 / whole as f64 * 100.0;
    if (percent * 10.0).round() >= 100.0 {
        format!("{percent:.0}%")
    } else if (percent * 100.0).round() >= 100.0 {
        format!("{percent:.1}%")
    } else {
        format!("{percent:.2}%")
    }
}

/// Persists the theme and language choices together; a missing file disables saving.
fn save_settings(app: &App) {
    if let Some(file) = &app.settings_file
        && let Err(error) = settings::save(
            file,
            settings::Settings {
                theme: app.theme_override,
                lang: app.lang_override,
            },
        )
    {
        eprintln!("filegram: failed to save the settings: {error}");
    }
}

fn view(app: &App) -> Element<'_, Message> {
    match &app.scan {
        ScanState::Idle => idle_view(app),
        ScanState::Running {
            current,
            files,
            dirs,
        } => running_view(app, current, *files, *dirs),
        ScanState::Done => map_view(app),
    }
}

/// The scan progress label. Monospace digits and a monotonic counter keep the
/// label widening but never shrinking, so the path beside it does not jitter.
fn scan_label<'a>(label: &'static str, files: u64, size: f32) -> Element<'a, Message> {
    row![
        text(label).size(size),
        text(files.to_string()).size(size).font(Font::MONOSPACE),
    ]
    .align_y(Center)
    .into()
}

/// The count of directories being traversed right now, with the folder glyph.
fn dirs_in_flight<'a>(dirs: u64) -> Element<'a, Message> {
    row![
        muted_icon(FOLDER_ICON).width(16).height(16),
        text(dirs.to_string()).font(Font::MONOSPACE).style(muted_text),
    ]
    .spacing(6)
    .align_y(Center)
    .into()
}

/// Scan screen: a counter until the first snapshot, then the map grows as the
/// scan proceeds (already navigable: NodeIds are stable).
fn running_view<'a>(app: &'a App, current: &str, files: u64, dirs: u64) -> Element<'a, Message> {
    let s = app.strings();
    if app.tree.is_none() {
        return center(
            column![
                scan_label(s.scanning_files, files, 20.0),
                row![
                    dirs_in_flight(dirs),
                    text(format::shorten_path(current, PATH_BAR_MAX_CHARS)).style(muted_text),
                ]
                .spacing(8)
                .align_y(Center),
                button(text(s.cancel))
                    .style(chrome_button)
                    .on_press(Message::CancelScan),
            ]
            .spacing(16),
        )
        .into();
    }
    // Mid-scan the leading slot holds a Cancel button with the spinner; the
    // trailing slot holds the live tally.
    let tree = app.tree.as_ref().expect("running_view map branch requires a tree");
    let total = tree.node(tree.root).size;
    let bar = map_top_bar(
        app,
        button(
            row![spinner(), text(s.cancel)]
                .spacing(8)
                .align_y(Center),
        )
        .style(chrome_button)
        .on_press(Message::CancelScan)
        .into(),
        scan_stats(files, total).into(),
    );
    let mut footer = row![
        dirs_in_flight(dirs),
        container(text(format::shorten_path(current, PATH_BAR_MAX_CHARS)).style(muted_text))
            .width(Fill)
            .height(Fill)
            .align_y(Center),
    ]
    .spacing(16)
    .height(BAR_CONTENT_HEIGHT)
    .align_y(Center);
    footer = footer.extend(mouse_hints(app));
    let footer = container(footer).padding(8).style(bar_style);
    column![bar, map_canvas(app), footer].into()
}

/// The top bar shared by the scan and finished-map screens. Left: `leading`
/// controls, the Go-up button (disabled at the root), the current path. Right:
/// `trailing` action, then the disk-usage gauge pinned to the far right.
fn map_top_bar<'a>(
    app: &'a App,
    leading: Element<'a, Message>,
    trailing: Element<'a, Message>,
) -> Element<'a, Message> {
    let tree = app.tree.as_ref().expect("map_top_bar requires a tree");
    let current_path = tree.node(app.current).path.display().to_string();
    let left = row![
        leading,
        chrome_icon_only_button_maybe(
            UP_ICON,
            app.strings().go_up,
            (!app.nav_stack.is_empty()).then_some(Message::GoUp),
        ),
        container(
            text(format::shorten_path(&current_path, PATH_BAR_MAX_CHARS))
                .style(muted_text)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .padding(8),
    ]
    .spacing(8)
    .align_y(Center);
    let mut actions = row![trailing].spacing(24).align_y(Center);
    if let Some(usage_bar) = disk_usage_bar(app) {
        actions = actions.push(usage_bar);
    }
    container(
        row![left.width(Fill)]
            .push(container(actions).width(Fill).align_x(iced::Right))
            .spacing(8)
            .align_y(Center),
    )
    // A wider right inset keeps the gauge off the window edge.
    .padding(iced::Padding::new(8.0).right(16.0))
    .style(bar_style)
    .into()
}

fn map_view(app: &App) -> Element<'_, Message> {
    let Some(tree) = &app.tree else {
        return idle_view(app);
    };
    let s = app.strings();
    // The tally comes from the tree root, so it stays consistent after deletions.
    let root = tree.node(tree.root);
    let bar = map_top_bar(
        app,
        row![
            chrome_icon_button(HOME_ICON, s.new_scan, Message::NewScan),
            chrome_icon_only_button(RESCAN_ICON, s.rescan, Message::Rescan),
        ]
        .spacing(8)
        .align_y(Center)
        .into(),
        scan_stats(root.files, root.size).into(),
    );
    let content = column![bar, map_canvas(app), status_bar(app)];
    // A click on the dimmed backdrop outside the card cancels.
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

/// The trash confirmation card: target and the Cancel / Move to Trash buttons.
fn delete_dialog(app: &App, target: NodeId) -> Element<'_, Message> {
    let tree = app.tree.as_ref().expect("delete_dialog requires a tree");
    let node = tree.node(target);
    let s = app.strings();
    let kind = if node.is_dir { s.folder } else { s.file };
    container(
        column![
            text(s.trash_question).size(20),
            text(format!(
                "{kind} \"{}\" — {}",
                node.name,
                format::human_size(node.size)
            )),
            row![
                button(text(s.cancel))
                    .style(chrome_button)
                    .on_press(Message::DeleteCancelled),
                button(text(s.trash_button))
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

/// Bottom status bar: the active node (or current folder) with its size on the
/// left, mouse button hints on the right.
fn status_bar(app: &App) -> Element<'_, Message> {
    let tree = app.tree.as_ref().expect("status_bar requires a tree");
    let node = tree.node(app.active.unwrap_or(app.current));
    let percent = size_percent(node.size, tree.node(tree.root).size);
    let size_label = if node.is_dir {
        format!(
            "{} · {} · {} ({})",
            node.name,
            format::human_size(node.size),
            percent,
            node.children.len(),
        )
    } else {
        format!(
            "{} · {} · {}",
            node.name,
            format::human_size(node.size),
            percent,
        )
    };
    // A fixed content height keeps the bar (and the canvas above it) constant
    // whether or not hints show; without it, clearing `active` would shrink the
    // bar, resize the canvas, and snap the zoom transition.
    let info = row![
        diskmap::entry_icon(node.is_dir, &node.name, 16.0),
        text(size_label).size(14).style(muted_text),
    ]
    .spacing(8)
    .align_y(Center);
    let mut content = row![container(info).width(Fill)]
        .spacing(16)
        .height(BAR_CONTENT_HEIGHT)
        .align_y(Center);
    content = content.extend(mouse_hints(app));
    container(content)
        .padding(8)
        .style(bar_style)
        .into()
}

/// The mini disk-usage bar. The label quotes *free* space while the fill shows
/// the *used* share, the file-manager convention. `None` omits the bar.
fn disk_usage_bar(app: &App) -> Option<Element<'_, Message>> {
    let usage = app.disk_usage?;
    let label = format!(
        "{} {} {}",
        format::human_size(usage.total.saturating_sub(usage.used)),
        app.strings().disk_free,
        format::human_size(usage.total)
    );
    Some(
        column![
            text(label).size(11).style(muted_text),
            progress_bar(0.0..=1.0, usage.fraction())
                .length(140)
                .girth(6)
                .style(disk_usage_progress_style),
        ]
        .spacing(4)
        .align_x(Center)
        .into(),
    )
}

/// A mouse icon with the pressed button filled, plus the action.
fn mouse_hint<'a>(icon: &'static [u8], action: &'a str) -> Element<'a, Message> {
    row![
        themed_icon(icon).width(15).height(20),
        text(action).size(14).style(muted_text)
    ]
    .spacing(6)
    .align_y(Center)
    .into()
}

/// LMB/RMB hints, shown only while a brick is hovered; Go-up gated on a parent.
fn mouse_hints(app: &App) -> Vec<Element<'_, Message>> {
    if app.active.is_none() {
        return Vec::new();
    }
    let s = app.strings();
    let mut hints = vec![mouse_hint(LMB_ICON, s.hint_select)];
    if !app.nav_stack.is_empty() {
        hints.push(mouse_hint(RMB_ICON, s.hint_go_up));
    }
    hints
}

/// One full turn of the loading spinner, in seconds.
const SPINNER_PERIOD_SECS: f32 = 0.9;
/// The lit sweep: a 270° arc rotating over the muted full-circle track.
const SPINNER_ARC: f32 = std::f32::consts::TAU * 0.75;

/// Indeterminate spinner shown inside the Cancel button. Its own canvas so it
/// repaints every frame, independent of the map's animation state.
struct Spinner;

#[derive(Default)]
struct SpinnerState {
    /// The first frame's timestamp; rotation is measured from it.
    start: Option<Instant>,
    angle: f32,
}

impl canvas::Program<Message> for Spinner {
    type State = SpinnerState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        _bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if let canvas::Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            let start = *state.start.get_or_insert(*now);
            // Wrap into one turn so a long scan can't grow the angle into f32
            // imprecision and stutter.
            state.angle = (now.duration_since(start).as_secs_f32() / SPINNER_PERIOD_SECS
                * std::f32::consts::TAU)
                .rem_euclid(std::f32::consts::TAU);
            return Some(canvas::Action::request_redraw());
        }
        None
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let center = frame.center();
        let width = 2.0;
        let radius = (bounds.width.min(bounds.height) - width) / 2.0;
        let lit = muted_color(theme);
        // The faint full-circle track under the rotating arc.
        frame.stroke(
            &canvas::Path::circle(center, radius),
            canvas::Stroke::default()
                .with_color(Color { a: 0.25, ..lit })
                .with_width(width),
        );
        let arc = canvas::Path::new(|b| {
            b.arc(canvas::path::Arc {
                center,
                radius,
                start_angle: iced::Radians(state.angle),
                end_angle: iced::Radians(state.angle + SPINNER_ARC),
            });
        });
        frame.stroke(
            &arc,
            canvas::Stroke::default()
                .with_color(lit)
                .with_width(width)
                .with_line_cap(canvas::LineCap::Round),
        );
        vec![frame.into_geometry()]
    }
}

/// The spinner sized to sit inside the Cancel button.
fn spinner<'a>() -> Element<'a, Message> {
    canvas(Spinner).width(16).height(16).into()
}

/// The top-bar scan tally: file count and collected size, monospace so the
/// figures don't jitter.
fn scan_stats<'a>(files: u64, total: u64) -> Element<'a, Message> {
    let stat = |icon: &'static [u8], value: String| {
        row![
            muted_icon(icon).width(16).height(16),
            text(value).size(14).font(Font::MONOSPACE).style(muted_text),
        ]
        .spacing(6)
        .align_y(Center)
    };
    row![
        stat(LAYERS_ICON, files.to_string()),
        stat(SIZE_ICON, format::human_size(total)),
    ]
    .spacing(12)
    .align_y(Center)
    .into()
}

/// The map canvas with the hover actions panel stacked on the active brick.
/// Call only when `app.tree.is_some()`.
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
                .find(|&(brick, _)| brick == diskmap::Brick::Node(active))
                // A brick too small for a caption gets no actions panel either.
                .filter(|&(_, rect)| diskmap::has_label(rect))
                .map(|(_, rect)| brick_actions(app, active, rect, size))
        });
        // Always stack the canvas, panel or not: moving it between a bare
        // element and a stack child rebuilds the widget tree and wipes the
        // canvas state, snapping the navigation zoom that very frame.
        match actions {
            Some(panel) => stack![map, panel].into(),
            None => stack![map].into(),
        }
    })
    .into()
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("Filegram")
        .theme(App::theme)
        .subscription(subscription)
        .window(iced::window::Settings {
            size: Size::new(1024.0, 768.0),
            // Pre-rendered RGBA so `from_rgba` needs no image decoder, unlike
            // `from_file_data` which would pull the whole `image` feature.
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

    /// A disk-isolated `App`: empty in-memory history, no history file.
    fn test_app() -> App {
        initial_app(history::History::default(), None)
    }

    /// A scan root that exits immediately: a missing child of a fresh temp dir.
    /// The guard keeps the temp dir alive for the test.
    fn missing_scan_root() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing").display().to_string();
        (dir, path)
    }

    /// [`test_app`] with a finished scan and a root-only tree, the only state
    /// deletion is legal in.
    fn scanned_app() -> App {
        scanned_app_at(Path::new("/root"))
    }

    /// [`scanned_app`] rooted at `path`, so the volume query can succeed or fail.
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

    /// A reading no real volume can produce, so a re-query is detectable.
    fn stale_usage() -> disk::DiskUsage {
        disk::DiskUsage { used: 1, total: 2 }
    }

    #[test]
    fn latest_release_shown_only_when_it_differs() {
        let mut app = test_app();
        let _ = update(&mut app, Message::LatestReleaseLoaded(None));
        assert_eq!(app.latest_release, None);

        let same = format!("v{}", env!("CARGO_PKG_VERSION"));
        let _ = update(&mut app, Message::LatestReleaseLoaded(Some(same)));
        assert_eq!(app.latest_release, None);

        let _ = update(
            &mut app,
            Message::LatestReleaseLoaded(Some("v99.0.0".to_string())),
        );
        assert_eq!(app.latest_release, Some("v99.0.0".to_string()));
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
                dirs: 0,
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
        assert_eq!(app.theme(), Theme::Dark);
        let _ = update(&mut app, Message::SystemThemeChanged(Mode::Light));
        assert_eq!(app.theme(), *LIGHT_THEME);
    }

    #[test]
    fn toggle_flips_theme() {
        let mut app = test_app();
        let _ = update(&mut app, Message::ToggleTheme);
        assert_eq!(app.theme(), Theme::Dark);
        let _ = update(&mut app, Message::ToggleTheme);
        assert_eq!(app.theme(), *LIGHT_THEME);
    }

    #[test]
    fn toggle_persists_theme_choice() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.cfg");
        let mut app = test_app();
        app.settings_file = Some(file.clone());
        let _ = update(&mut app, Message::ToggleTheme);
        assert_eq!(settings::load(&file).unwrap().theme, Some(Mode::Dark));
        let _ = update(&mut app, Message::ToggleTheme);
        assert_eq!(settings::load(&file).unwrap().theme, Some(Mode::Light));
    }

    #[test]
    fn language_pick_overrides_system_and_closes_menu() {
        let mut app = test_app();
        let _ = update(&mut app, Message::LanguageMenuToggled);
        assert!(app.lang_menu_open);
        let _ = update(&mut app, Message::LanguagePicked(Lang::RuRu));
        assert!(!app.lang_menu_open);
        assert_eq!(app.strings().scan, "Сканировать");
    }

    #[test]
    fn language_menu_expands_and_resets_on_reopen() {
        let mut app = test_app();
        app.system_lang = Lang::EnUs;
        let _ = update(&mut app, Message::LanguageMenuToggled);
        assert!(!app.lang_menu_expanded);
        let _ = update(&mut app, Message::LanguageMenuExpanded);
        assert!(app.lang_menu_expanded);
        let _ = update(&mut app, Message::LanguageMenuToggled);
        let _ = update(&mut app, Message::LanguageMenuToggled);
        assert!(app.lang_menu_open);
        assert!(!app.lang_menu_expanded);
    }

    #[test]
    fn language_menu_opens_expanded_for_an_extended_language() {
        let mut app = test_app();
        app.lang_override = Some(Lang::Uk);
        let _ = update(&mut app, Message::LanguageMenuToggled);
        assert!(app.lang_menu_expanded);
    }

    #[test]
    fn language_pick_persists_alongside_theme() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.cfg");
        let mut app = test_app();
        app.settings_file = Some(file.clone());
        let _ = update(&mut app, Message::ToggleTheme);
        let _ = update(&mut app, Message::LanguagePicked(Lang::JaJp));
        assert_eq!(
            settings::load(&file).unwrap(),
            settings::Settings {
                theme: Some(Mode::Dark),
                lang: Some(Lang::JaJp),
            }
        );
    }

    #[test]
    fn strings_follow_system_until_overridden() {
        let mut app = test_app();
        app.system_lang = Lang::DeDe;
        assert_eq!(app.strings().scan, "Scannen");
        let _ = update(&mut app, Message::LanguagePicked(Lang::Tr));
        assert_eq!(app.strings().scan, "Tara");
    }

    #[test]
    fn manual_theme_survives_system_change() {
        let mut app = test_app();
        let _ = update(&mut app, Message::SystemThemeChanged(Mode::Light));
        let _ = update(&mut app, Message::ToggleTheme);
        assert_eq!(app.theme(), Theme::Dark);
        let _ = update(&mut app, Message::SystemThemeChanged(Mode::Light));
        assert_eq!(app.theme(), Theme::Dark);
    }

    #[test]
    fn initial_path_prefers_latest_history_entry() {
        let mut history = history::History::default();
        history.push("/scans/latest");
        assert_eq!(initial_path(&history), "/scans/latest");
        assert_ne!(initial_path(&history::History::default()), "/scans/latest");
        assert!(!initial_path(&history::History::default()).is_empty());
    }

    #[test]
    fn size_percent_formats_zero_and_thresholds() {
        let cases = [
            (0, 0, "0%"),
            (0, 100, "0.00%"),
            (5, 1000, "0.50%"),
            (15, 1000, "1.5%"),
            (95, 1000, "9.5%"),
            (100, 1000, "10%"),
            (999, 1000, "100%"),
            // The printed value decides the threshold, so no "10.0%"/"1.00%" leaks.
            (996, 10_000, "10%"),
            (994, 10_000, "9.9%"),
            (996, 100_000, "1.0%"),
            (994, 100_000, "0.99%"),
        ];
        for (part, whole, expected) in cases {
            assert_eq!(size_percent(part, whole), expected);
        }
    }

    #[test]
    fn start_scan_normalizes_input_and_records_existing_dir() {
        let mut app = test_app();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().display().to_string();
        app.path_input = format!("  {path}/  ");
        let _ = update(&mut app, Message::StartScan);
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
                dirs: 0,
            },
        ] {
            app.scan = scan;
            app.disk_usage = Some(stale_usage());
            let _ = update(&mut app, Message::WindowFocused);
            assert_eq!(app.disk_usage, Some(stale_usage()));
        }
    }

    #[test]
    fn initial_app_lists_disk_roots() {
        assert_eq!(test_app().disk_roots, disk::mounted_roots());
    }

    #[test]
    fn window_focus_refreshes_disk_roots() {
        let mut app = test_app();
        app.disk_roots = vec![disk::DiskRoot {
            path: PathBuf::from("/filegram-test-unmounted"),
            kind: disk::DiskKind::Removable,
        }];
        let _ = update(&mut app, Message::WindowFocused);
        assert_eq!(app.disk_roots, disk::mounted_roots());
    }

    #[test]
    fn start_scan_ignores_a_path_that_normalizes_to_blank() {
        // A mount point can carry a line break (`\012` in /proc/mounts).
        let mut app = test_app();
        app.path_input = "/media/user/bad\nname".to_string();
        let _ = update(&mut app, Message::StartScan);
        assert!(matches!(app.scan, ScanState::Idle));
        assert!(app.history.entries().is_empty());
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
        // A tempdir-local delete keeps the test hermetic.
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
        // A real dir proves no re-query happened (one would replace the stale reading).
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

    /// The full "press Scan" flow: `StartScan`, the scan events fed back as the
    /// iced runtime would, then the resulting tree's counts and size asserted.
    #[test]
    fn pressing_scan_yields_the_correct_tree() {
        use iced::futures::StreamExt;

        let dir = tempfile::tempdir().unwrap();
        // root/{a.bin=100, b.bin=200, sub/{c.bin=300, d.bin=50, deep/{e.bin=10}}}
        std::fs::write(dir.path().join("a.bin"), vec![0u8; 100]).unwrap();
        std::fs::write(dir.path().join("b.bin"), vec![0u8; 200]).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/c.bin"), vec![0u8; 300]).unwrap();
        std::fs::write(dir.path().join("sub/d.bin"), vec![0u8; 50]).unwrap();
        std::fs::create_dir(dir.path().join("sub/deep")).unwrap();
        std::fs::write(dir.path().join("sub/deep/e.bin"), vec![0u8; 10]).unwrap();

        let mut app = test_app();
        app.path_input = dir.path().display().to_string();

        let _ = update(&mut app, Message::StartScan);
        assert!(matches!(app.scan, ScanState::Running { .. }));

        let events = iced::futures::executor::block_on(
            scanner::start_scan(PathBuf::from(&app.path_input), app.cancel.clone())
                .collect::<Vec<_>>(),
        );
        for event in events {
            let _ = update(&mut app, Message::Scan(event));
        }

        assert!(matches!(app.scan, ScanState::Done), "scan finished");
        let tree = app.tree.as_ref().expect("a tree after the scan");
        assert_eq!(app.current, tree.root, "navigation starts at the scan root");

        let files = tree.nodes.iter().filter(|n| !n.is_dir).count();
        let dirs = tree.nodes.iter().filter(|n| n.is_dir).count();
        assert_eq!(files, 5, "a, b, c, d, e");
        assert_eq!(dirs, 3, "root, sub, deep");
        // 100+200+300+50+10 file bytes + one DIR_ENTRY_SIZE per directory.
        assert_eq!(tree.node(tree.root).size, 660 + fs_tree::DIR_ENTRY_SIZE * 3);
    }
}
