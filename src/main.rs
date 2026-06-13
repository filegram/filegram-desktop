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

use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use iced::theme::{Mode, Palette};
use iced::widget::{
    button, canvas, center, column, container, mouse_area, opaque, progress_bar, responsive, row,
    scrollable, stack, svg, text, text_input, tooltip,
};
use iced::{
    Border, Center, Color, Element, Fill, Font, Padding, Rectangle, Shadow, Size, Subscription,
    Task, Theme, Vector,
};

use diskmap::DiskMap;
use fs_tree::{FsTree, NodeId};
use i18n::Lang;
use scanner::ScanEvent;

/// Maximum number of characters in the path bar above the map (then `/../` compression).
const PATH_BAR_MAX_CHARS: usize = 80;

/// Fixed content height of the bottom bars, sized to the mouse-hint icon so
/// the bar — and the map canvas above it — keeps a constant size whether or
/// not the hints are shown; a fluctuating canvas size snaps the zoom tween.
const BAR_CONTENT_HEIGHT: f32 = 20.0;

/// Mouse icons for the status bar hints: the pressed button is filled.
const LMB_ICON: &[u8] = include_bytes!("../assets/lmb.svg");
const RMB_ICON: &[u8] = include_bytes!("../assets/rmb.svg");
/// Hover panel action icons.
const FOLDER_ICON: &[u8] = include_bytes!("../assets/folder.svg");
const TRASH_ICON: &[u8] = include_bytes!("../assets/trash.svg");
/// Top bar button icons: a circular arrow for Rescan, a treemap-like
/// brick layout for Select folder.
const RESCAN_ICON: &[u8] = include_bytes!("../assets/rescan.svg");
const BRICKS_ICON: &[u8] = include_bytes!("../assets/bricks.svg");
/// A stacked-layers glyph next to the file counter, and a pie-chart glyph
/// next to the collected size, in the scan tally.
const LAYERS_ICON: &[u8] = include_bytes!("../assets/layers.svg");
const SIZE_ICON: &[u8] = include_bytes!("../assets/pie-chart.svg");
/// An up arrow for the icon-only Go-up button next to Rescan; shown only
/// when there is a parent to ascend to.
const UP_ICON: &[u8] = include_bytes!("../assets/up.svg");
/// Quick-scan folder icons on the start screen.
const HOME_ICON: &[u8] = include_bytes!("../assets/home.svg");
const DOWNLOADS_ICON: &[u8] = include_bytes!("../assets/downloads.svg");
const DESKTOP_ICON: &[u8] = include_bytes!("../assets/desktop.svg");
const DOCUMENTS_ICON: &[u8] = include_bytes!("../assets/documents.svg");
/// Quick disk row icons, one per hardware kind: internal drive, plugged-in
/// media, network share, optical disc. The globe doubles as the footer's
/// language menu button.
const DRIVE_ICON: &[u8] = include_bytes!("../assets/drive.svg");
const USB_ICON: &[u8] = include_bytes!("../assets/usb.svg");
const GLOBE_ICON: &[u8] = include_bytes!("../assets/globe.svg");
const DISC_ICON: &[u8] = include_bytes!("../assets/disc.svg");
/// The theme toggle in the bottom-left corner: the icon shows the mode a
/// click switches to (the moon in light mode, the sun in dark).
const SUN_ICON: &[u8] = include_bytes!("../assets/sun.svg");
const MOON_ICON: &[u8] = include_bytes!("../assets/moon.svg");

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
    /// The exact file count of the finished tree, kept so the tally stays on
    /// screen after the scan (the live `ScanState::Running.files` is gone by
    /// then). Set on `Finished`; `0` until the first scan completes.
    scanned_files: u64,
    /// The volume of the scan root, for the mini disk-usage bar; `None`
    /// hides the bar (no scan yet, or the OS query failed).
    disk_usage: Option<disk::DiskUsage>,
    /// Mounted volume roots for the quick disk row on the start screen;
    /// refreshed on window focus, so volumes mounted in the background
    /// show up.
    disk_roots: Vec<disk::DiskRoot>,
    path_input: String,
    history: history::History,
    /// Where the history is persisted; `None` disables saving (tests).
    history_file: Option<PathBuf>,
    cache: canvas::Cache,
    cancel: Arc<AtomicBool>,
    /// The system light/dark preference; the chrome theme follows it.
    theme_mode: Mode,
    /// The user's manual choice from the theme toggle; overrides the
    /// system preference and persists across launches.
    theme_override: Option<Mode>,
    /// The system UI language, captured at startup; the chrome follows it
    /// until the menu picks an override.
    system_lang: Lang,
    /// The user's pick from the language menu; overrides the system locale
    /// and persists across launches.
    lang_override: Option<Lang>,
    /// Whether the footer language menu is open.
    lang_menu_open: bool,
    /// Whether the open menu shows the full list instead of the short one;
    /// the "…" entry expands it, reopening resets to short.
    lang_menu_expanded: bool,
    /// Where the theme and language choices are persisted; `None` disables
    /// saving (tests).
    settings_file: Option<PathBuf>,
    /// The latest GitHub release tag, when it differs from the running
    /// version; shown in the footer as a link to the release page. `None`
    /// until the background check returns (or forever, if it fails).
    latest_release: Option<String>,
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
    let mut app = initial_app(history, history_file);
    app.settings_file = settings::default_file();
    // Unlike the history, an unreadable settings file is safe to save over:
    // the next toggle or language pick rewrites the whole file anyway.
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
    (
        app,
        Task::batch([
            iced::system::theme().map(Message::SystemThemeChanged),
            // The release check starts immediately but runs entirely in the
            // background: the window opens without waiting for the network.
            Task::perform(release::fetch_latest_tag(), Message::LatestReleaseLoaded),
        ]),
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
        scanned_files: 0,
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
        settings_file: None,
        latest_release: None,
        delete: |path| trash::delete(path).map_err(std::io::Error::other),
    }
}

fn theme(app: &App) -> Theme {
    if is_dark(app) {
        Theme::Dark
    } else {
        LIGHT_THEME.clone()
    }
}

/// The effective mode: the manual choice when there is one, the system
/// preference otherwise (`Mode::None` renders light).
fn is_dark(app: &App) -> bool {
    matches!(app.theme_override.unwrap_or(app.theme_mode), Mode::Dark)
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
            // Blank — empty input or a path with line breaks, which
            // normalize to blank — has nothing to scan.
            if app.path_input.is_empty() {
                return Task::none();
            }
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
                // A late finish after the scan was abandoned (NewScan → Idle)
                // is ignored: only a still-running scan settles into the map.
                ScanEvent::Finished(tree) if matches!(app.scan, ScanState::Running { .. }) => {
                    // NodeIds are stable across snapshots: keep any navigation
                    // made during the scan.
                    if app.tree.is_none() {
                        app.current = tree.root;
                    }
                    // A long scan leaves the start-of-scan reading stale;
                    // re-query the volume so the bar matches the final map.
                    app.disk_usage = root_usage(&tree);
                    // The exact file count for the tally; the throttled live
                    // counter may have stopped a few files short of the total.
                    app.scanned_files = tree.nodes.iter().filter(|node| !node.is_dir).count() as u64;
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
        Message::GoUp => {
            if let Some(previous) = app.nav_stack.pop() {
                app.current = previous;
                app.active = None;
                app.cache.clear();
            }
            Task::none()
        }
        Message::NewScan => {
            // Reachable mid-scan now that the top bar is shared: stop the
            // in-flight scan so a late Finished cannot drag the abandoned
            // scan back onto the screen.
            app.cancel.store(true, Ordering::Relaxed);
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
            // Volumes mount and unmount while the app is in the background;
            // the quick disk row refreshes on any screen — the start screen
            // is exactly where the user lands after plugging a drive in.
            app.disk_roots = disk::mounted_roots();
            // The volume drifts while the app is in the background (other
            // programs write and delete); refresh the bar when the user
            // comes back to a finished map. Mid-scan readings stay owned
            // by StartScan/Finished — the bar is hidden until then anyway.
            if matches!(&app.scan, ScanState::Done)
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
        Message::LatestReleaseLoaded(tag) => {
            // Only a tag that differs from the running build is worth
            // showing: "v0.2.2 (v0.2.2)" tells the user nothing.
            app.latest_release =
                tag.filter(|tag| tag.trim_start_matches('v') != env!("CARGO_PKG_VERSION"));
            Task::none()
        }
        Message::LatestReleasePressed => {
            // The page of the tag the footer shows; the message only ever
            // arrives from the link, which exists when the tag is known.
            if let Some(tag) = &app.latest_release {
                let _ = open::that_detached(release::release_url(tag));
            }
            Task::none()
        }
        Message::ToggleTheme => {
            let mode = if is_dark(app) { Mode::Light } else { Mode::Dark };
            app.theme_override = Some(mode);
            save_settings(app);
            Task::none()
        }
        Message::LanguageMenuToggled => {
            app.lang_menu_open = !app.lang_menu_open;
            // The menu opens short — unless the current language only
            // lives in the full list, which would otherwise be invisible.
            app.lang_menu_expanded = !Lang::PRIMARY.contains(&lang(app));
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
            // Filesystem mutation requires a finished scan: the snapshot
            // must not be edited while the scanner still produces them.
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

/// The share `part` weighs against `whole`, as a percentage string. Tiny
/// shares keep more decimals so they don't all collapse to "0%"; a zero
/// `whole` (an empty scan) reads as "0%". The precision thresholds apply to
/// the value as it would print, so a 9.96% share reads "10%", never "10.0%".
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

/// Persists the manual theme and language choices together; a missing
/// settings file disables saving, like the history.
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

/// The effective language: the menu pick when there is one, the system
/// locale otherwise.
fn lang(app: &App) -> Lang {
    app.lang_override.unwrap_or(app.system_lang)
}

/// The string table of the effective language.
fn strings(app: &App) -> &'static i18n::Strings {
    lang(app).strings()
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
    let s = strings(app);
    let mut content = column![
        text(s.app_title).size(28),
        row![
            text_input(s.path_placeholder, &app.path_input)
                .on_input(Message::PathChanged)
                .on_submit(Message::StartScan),
            chrome_icon_button(BRICKS_ICON, s.scan, Message::StartScan),
        ]
        .spacing(8),
    ]
    .spacing(16)
    .max_width(600);
    if let Some(quick) = quick_scans(s) {
        content = content.push(quick);
    }
    if let Some(disks) = disk_scans(app) {
        content = content.push(disks);
    }
    if !app.history.entries().is_empty() {
        content = content.push(recent_scans(app));
    }
    let screen = column![center(content), corner_footer(app)];
    if !app.lang_menu_open {
        return screen.into();
    }
    stack![screen, language_menu_overlay(app)].into()
}

/// The language popup pinned above the footer's globe button: a card with
/// the short list and a trailing "…" that expands it to every language,
/// native names, the current one highlighted. The transparent backdrop
/// closes the menu on a click anywhere else.
fn language_menu_overlay(app: &App) -> Element<'_, Message> {
    let current = lang(app);
    let listed: &[Lang] = if app.lang_menu_expanded {
        &Lang::ALL
    } else {
        &Lang::PRIMARY
    };
    let mut entries = column(listed.iter().map(|&lang| {
        let style = if lang == current {
            button::primary
        } else {
            button::text
        };
        // A name never wraps: a two-line entry would break the menu rhythm;
        // the card is sized so the longest name fits next to the scrollbar.
        button(
            text(lang.native_name())
                .size(14)
                .wrapping(iced::widget::text::Wrapping::None),
        )
        .width(Fill)
        .padding(Padding {
            top: 4.0,
            right: 10.0,
            bottom: 4.0,
            left: 10.0,
        })
        .style(style)
        .on_press(Message::LanguagePicked(lang))
        .into()
    }))
    .spacing(2);
    if !app.lang_menu_expanded {
        entries = entries.push(
            button(text("…").size(14).width(Fill).align_x(Center))
                .width(Fill)
                .padding(Padding {
                    top: 4.0,
                    right: 10.0,
                    bottom: 4.0,
                    left: 10.0,
                })
                .style(button::text)
                .on_press(Message::LanguageMenuExpanded),
        );
    }
    let card = container(scrollable(entries).width(250))
        .style(tooltip_style)
        .padding(4)
        .max_height(560);
    opaque(
        mouse_area(
            container(opaque(card))
                .width(Fill)
                .height(Fill)
                .align_y(iced::Bottom)
                .padding(Padding {
                    left: 8.0,
                    bottom: 44.0,
                    ..Padding::ZERO
                }),
        )
        .on_press(Message::LanguageMenuToggled),
    )
}

/// The quick disk row right under the folder row: the root of every
/// mounted volume, a click scans the volume whole. `None` hides the row
/// when `disk_roots` is empty, like an empty folder row — possible on
/// Windows only, on Unix the list always holds at least `/`.
fn disk_scans(app: &App) -> Option<Element<'_, Message>> {
    let buttons: Vec<Element<'_, Message>> = app
        .disk_roots
        .iter()
        .map(|root| {
            quick_scan_button(
                disk_icon(root.kind),
                disk::root_label(&root.path),
                &root.path,
            )
        })
        .collect();
    (!buttons.is_empty()).then(|| {
        // The same muted header the history row wears, so the two sections
        // under the folder shortcuts read alike.
        column![
            text(strings(app).disks).size(14).style(muted_text),
            row(buttons).spacing(8).wrap(),
        ]
        .spacing(2)
        .into()
    })
}

/// The icon a quick disk row entry wears, by the hardware kind behind
/// the volume.
fn disk_icon(kind: disk::DiskKind) -> &'static [u8] {
    match kind {
        disk::DiskKind::Internal => DRIVE_ICON,
        disk::DiskKind::Removable => USB_ICON,
        disk::DiskKind::Network => GLOBE_ICON,
        disk::DiskKind::Optical => DISC_ICON,
    }
}

/// Quick scans of the standard user folders, between the scan row and the
/// history: a click scans the folder exactly like a history entry. A folder
/// the OS cannot locate is omitted; `None` when none can be, so the idle
/// screen does not reserve a blank gap for an empty row.
fn quick_scans<'a>(s: &'static i18n::Strings) -> Option<Element<'a, Message>> {
    let folders: [(&[u8], &str, Option<PathBuf>); 4] = [
        (HOME_ICON, s.home, dirs::home_dir()),
        (DOWNLOADS_ICON, s.downloads, dirs::download_dir()),
        (DESKTOP_ICON, s.desktop, dirs::desktop_dir()),
        (DOCUMENTS_ICON, s.documents, dirs::document_dir()),
    ];
    let buttons: Vec<Element<'a, Message>> = folders
        .into_iter()
        .filter_map(|(icon, name, path)| {
            path.map(|path| quick_scan_button(icon, name.to_string(), &path))
        })
        .collect();
    (!buttons.is_empty()).then(|| row(buttons).spacing(8).into())
}

/// One entry of the quick rows: an icon with a short name; a click scans
/// the path exactly like a history entry.
fn quick_scan_button<'a>(icon: &'static [u8], name: String, path: &Path) -> Element<'a, Message> {
    // Normalized the way StartScan will see it: a path that normalizes to
    // blank (a mount point with a line break) gets no on_press, so the
    // button cannot fire a scan of "".
    let path = history::normalize(&path.display().to_string()).to_string();
    button(
        row![themed_icon(icon).width(16).height(16), text(name).size(14)]
            .spacing(6)
            .align_y(Center),
    )
    .style(button::text)
    .padding(4)
    .on_press_maybe((!path.is_empty()).then(|| Message::HistoryPicked(path)))
    .into()
}

/// The scan history under the path input: a click rescans the path.
fn recent_scans(app: &App) -> Element<'_, Message> {
    column![text(strings(app).recent_scans).size(14).style(muted_text)]
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
fn scan_label<'a>(label: &'static str, files: u64, size: f32) -> Element<'a, Message> {
    row![
        text(label).size(size),
        text(files.to_string()).size(size).font(Font::MONOSPACE),
    ]
    .align_y(Center)
    .into()
}

/// Scan screen: a counter until the first snapshot, after that the map grows
/// right as the scan proceeds (navigating it already works: NodeIds are stable).
fn running_view<'a>(app: &'a App, current: &str, files: u64) -> Element<'a, Message> {
    let s = strings(app);
    if app.tree.is_none() {
        return center(
            column![
                scan_label(s.scanning_files, files, 20.0),
                text(format::shorten_path(current, PATH_BAR_MAX_CHARS)).style(muted_text),
                button(text(s.cancel))
                    .style(chrome_button)
                    .on_press(Message::CancelScan),
            ]
            .spacing(16),
        )
        .into();
    }
    // The same top bar as the finished map (Select-folder, current path and
    // disk-usage gauge). The trailing slot holds the live tally (file count
    // and collected size) before a Cancel button that carries the spinner.
    let tree = app.tree.as_ref().expect("running_view map branch requires a tree");
    let total = tree.node(tree.root).size;
    let bar = map_top_bar(
        app,
        row![
            scan_stats(files, total),
            button(
                row![spinner(), text(s.cancel)]
                    .spacing(8)
                    .align_y(Center)
            )
            .style(chrome_button)
            .on_press(Message::CancelScan),
        ]
        .spacing(16)
        .align_y(Center)
        .into(),
    );
    // The folder being scanned right now fills the bottom bar, vertically
    // centered within the fixed bar height.
    let mut footer = row![
        container(text(format::shorten_path(current, PATH_BAR_MAX_CHARS)).style(muted_text))
            .width(Fill)
            .height(Fill)
            .align_y(Center),
    ]
    .spacing(16)
    .height(BAR_CONTENT_HEIGHT)
    .align_y(Center);
    // The same mouse hints as the finished map: the growing map is already
    // navigable mid-scan, so a hovered brick can be entered or ascended from.
    if app.active.is_some() {
        footer = footer.push(mouse_hint(LMB_ICON, s.hint_select));
        if !app.nav_stack.is_empty() {
            footer = footer.push(mouse_hint(RMB_ICON, s.hint_go_up));
        }
    }
    let footer = container(footer).padding(8).style(bar_style);
    column![bar, map_canvas(app), footer].into()
}

/// The top bar shared by the growing-scan and finished-map screens: the
/// Select-folder button and current-folder path on the left, the disk-usage
/// gauge centered between equal-width side zones, and the navigation buttons
/// on the right — the Go-up button (only with a parent to ascend to) followed
/// by a screen-specific `trailing` action (Rescan on the map, Cancel mid-scan).
fn map_top_bar<'a>(app: &'a App, trailing: Element<'a, Message>) -> Element<'a, Message> {
    let tree = app.tree.as_ref().expect("map_top_bar requires a tree");
    let current_path = tree.node(app.current).path.display().to_string();
    let s = strings(app);
    let mut top = row![row![
        chrome_icon_button(HOME_ICON, s.new_scan, Message::NewScan),
        container(
            text(format::shorten_path(&current_path, PATH_BAR_MAX_CHARS))
                .style(muted_text)
                .wrapping(iced::widget::text::Wrapping::None)
        )
        .padding(8),
    ]
    .spacing(8)
    .align_y(Center)
    .width(Fill)]
    .spacing(8)
    .align_y(Center);
    if let Some(usage_bar) = disk_usage_bar(app) {
        top = top.push(usage_bar);
    }
    let mut actions = row![].spacing(8);
    if !app.nav_stack.is_empty() {
        actions = actions.push(chrome_icon_only_button(UP_ICON, s.go_up, Message::GoUp));
    }
    actions = actions.push(trailing);
    container(top.push(container(actions).width(Fill).align_x(iced::Right)))
        .padding(8)
        .style(bar_style)
        .into()
}

fn map_view(app: &App) -> Element<'_, Message> {
    let Some(tree) = &app.tree else {
        return idle_view(app);
    };
    let s = strings(app);
    // The scan tally stays in the top bar after the scan, before Rescan, so
    // the file count and collected size don't vanish with the final map.
    let total = tree.node(tree.root).size;
    let bar = map_top_bar(
        app,
        row![
            scan_stats(app.scanned_files, total),
            chrome_icon_only_button(RESCAN_ICON, s.rescan, Message::Rescan),
        ]
        .spacing(16)
        .align_y(Center)
        .into(),
    );
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
    let s = strings(app);
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

/// Bottom status bar: on the left — the active node (or the current folder)
/// with its size, on the right — mouse button hints. The disk-usage gauge
/// lives in the top bar of the finished map instead.
fn status_bar(app: &App) -> Element<'_, Message> {
    let tree = app.tree.as_ref().expect("status_bar requires a tree");
    let node = tree.node(app.active.unwrap_or(app.current));
    // The percentage the node weighs against the whole scanned tree, plus
    // the entry count for folders (relocated here from the brick caption).
    let percent = size_percent(node.size, tree.node(tree.root).size);
    let size_label = if node.is_dir {
        format!(
            "{} | {} | {} ({})",
            format::human_size(node.size),
            percent,
            node.name,
            node.children.len(),
        )
    } else {
        format!(
            "{} | {} | {}",
            format::human_size(node.size),
            percent,
            node.name,
        )
    };
    // A fixed content height (the mouse-hint icon's) keeps the bar — and so
    // the map canvas above it — the exact same size whether or not the hints
    // are shown. Without it the bar would shrink the moment a navigation
    // clears `active`, resizing the canvas and snapping the zoom transition.
    let mut content = row![container(text(size_label).size(14).style(muted_text)).width(Fill)]
        .spacing(16)
        .height(BAR_CONTENT_HEIGHT)
        .align_y(Center);
    // The mouse hints only make sense while the cursor is over a brick:
    // `active` is set on hover and cleared when the cursor leaves the map.
    if app.active.is_some() {
        content = content.push(mouse_hint(LMB_ICON, strings(app).hint_select));
        if !app.nav_stack.is_empty() {
            content = content.push(mouse_hint(RMB_ICON, strings(app).hint_go_up));
        }
    }
    container(content)
        .padding(8)
        .style(bar_style)
        .into()
}

/// The theme toggle: an icon button showing the mode a click switches to.
fn theme_toggle(app: &App) -> Element<'_, Message> {
    let s = strings(app);
    let (icon, tip) = if is_dark(app) {
        (SUN_ICON, s.light_theme)
    } else {
        (MOON_ICON, s.dark_theme)
    };
    action_button(icon, tip, Some(Message::ToggleTheme))
}

/// The language menu trigger: the same square icon button as the theme
/// toggle, a globe with the localized "Language" hint.
fn language_button(app: &App) -> Element<'_, Message> {
    action_button(
        GLOBE_ICON,
        strings(app).language,
        Some(Message::LanguageMenuToggled),
    )
}

/// The application version in the bottom-right corner. Once the background
/// GitHub check finds a release different from the running build (e.g. the
/// stable release under a dev build), its tag follows in parentheses as a
/// link to the release page.
fn version_label(app: &App) -> Element<'_, Message> {
    let current = text(concat!("v", env!("CARGO_PKG_VERSION")))
        .size(14)
        .style(muted_text);
    let Some(tag) = &app.latest_release else {
        return current.into();
    };
    row![
        current,
        mouse_area(
            text(format!("({tag})"))
                .size(14)
                .style(|theme: &Theme| text::Style {
                    color: Some(theme.palette().primary),
                })
        )
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(Message::LatestReleasePressed),
    ]
    .spacing(4)
    .into()
}

/// The bottom corners of the start screen: the theme toggle and the
/// language menu on the left, the version on the right. The map screens
/// stay free of chrome.
fn corner_footer(app: &App) -> Element<'_, Message> {
    row![
        theme_toggle(app),
        language_button(app),
        container(version_label(app)).width(Fill).align_x(iced::Right),
    ]
    .spacing(2)
    .padding(8)
    .align_y(Center)
    .into()
}

/// The mini disk-usage bar: how full the scan root's volume is.
/// The label quotes the *free* space while the fill shows the *used* share —
/// the file-manager convention (Windows Explorer, GNOME Files) users already
/// read at a glance; the two are complements, not a mismatch.
/// `None` when no volume has been queried yet — the bar is omitted.
fn disk_usage_bar(app: &App) -> Option<Element<'_, Message>> {
    let usage = app.disk_usage?;
    let label = format!(
        "{} {} {}",
        format::human_size(usage.total.saturating_sub(usage.used)),
        strings(app).disk_free,
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

/// The mini disk-usage bar: an amber fill (the folder-brick accent) on a
/// muted track, matching the app mockup.
fn disk_usage_progress_style(theme: &Theme) -> progress_bar::Style {
    let track = if theme.extended_palette().is_dark {
        Color::from_rgb8(0x45, 0x45, 0x45)
    } else {
        Color::from_rgb8(0xD0, 0xD0, 0xD0)
    };
    progress_bar::Style {
        background: track.into(),
        bar: Color::from_rgb8(0xF9, 0xA8, 0x25).into(),
        border: Border {
            radius: 3.0.into(),
            ..Border::default()
        },
    }
}

/// The hover actions panel pinned to the active brick's top-right corner,
/// clamped to the canvas bounds.
fn brick_actions(app: &App, target: NodeId, brick: Rectangle, bounds: Size) -> Element<'_, Message> {
    // Deletion needs a finished scan: removing entries mid-scan would
    // desync the tree from the scanner's arena.
    let deletable = matches!(&app.scan, ScanState::Done).then_some(target);
    let s = strings(app);
    let panel = container(
        row![
            action_button(
                FOLDER_ICON,
                s.open_in_file_manager,
                Some(Message::Reveal(target)),
            ),
            action_button(
                TRASH_ICON,
                s.trash_tip,
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

/// An outline chrome button with a leading icon: the Rescan / Select folder pair.
fn chrome_icon_button<'a>(
    icon: &'static [u8],
    label: &'a str,
    on_press: Message,
) -> Element<'a, Message> {
    // A label never wraps: a two-line button would outgrow its row; long
    // translations must shorten instead.
    button(
        row![
            themed_icon(icon).width(16).height(16),
            text(label).wrapping(iced::widget::text::Wrapping::None),
        ]
        .spacing(6)
        .align_y(Center),
    )
    .style(chrome_button)
    .on_press(on_press)
    .into()
}

/// An outline chrome button with only an icon (no label): used for compact
/// top-bar actions like Go up and Rescan. A tooltip names the action, since
/// the icon alone carries no text.
/// An empty text keeps the line height — and thus the button height — equal to
/// the labeled `chrome_icon_button` next to it.
fn chrome_icon_only_button<'a>(
    icon: &'static [u8],
    tip: &'a str,
    on_press: Message,
) -> Element<'a, Message> {
    chrome_icon_only_button_maybe(icon, tip, Some(on_press))
}

/// Like [`chrome_icon_only_button`], but with an optional action; a missing
/// action also mutes the icon tint to match the disabled button state.
fn chrome_icon_only_button_maybe<'a>(
    icon: &'static [u8],
    tip: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    let disabled = on_press.is_none();
    tooltip(
        button(
            row![
                themed_icon_maybe_disabled(icon, disabled)
                    .width(16)
                    .height(16),
                text("")
            ]
            .align_y(Center),
        )
        .style(chrome_button)
        .on_press_maybe(on_press),
        text(tip).size(12),
        tooltip::Position::Bottom,
    )
    .style(tooltip_style)
    .padding(8)
    .gap(6)
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

/// Like [`themed_icon`], but tinted with the muted caption color so the icon
/// matches an adjacent [`muted_text`] label (e.g. the file counter).
fn muted_icon<'a>(icon: &'static [u8]) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(icon)).style(|theme: &Theme, _status| svg::Style {
        color: Some(muted_color(theme)),
    })
}

/// Like [`themed_icon`], but muted for disabled controls.
fn themed_icon_maybe_disabled<'a>(icon: &'static [u8], disabled: bool) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(icon)).style(move |theme: &Theme, _status| svg::Style {
        color: Some(if disabled {
            muted_color(theme)
        } else {
            theme.palette().text
        }),
    })
}

/// One full turn of the loading spinner, in seconds.
const SPINNER_PERIOD_SECS: f32 = 0.9;
/// The lit sweep of the spinner ring: a 270° arc that rotates over the muted
/// full-circle track.
const SPINNER_ARC: f32 = std::f32::consts::TAU * 0.75;

/// An indeterminate loading spinner shown next to the file counter while a
/// scan runs. Its own tiny canvas so it can repaint — and keep requesting the
/// next frame — every frame, independent of the map's animation state.
struct Spinner;

#[derive(Default)]
struct SpinnerState {
    /// The first frame's timestamp; the rotation is measured from it.
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
            state.angle =
                now.duration_since(start).as_secs_f32() / SPINNER_PERIOD_SECS * std::f32::consts::TAU;
            // Keep spinning for as long as the spinner is on screen.
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
        // The faint full-circle track under the lit, rotating arc.
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

/// The loading spinner sized to sit inside the Cancel button.
fn spinner<'a>() -> Element<'a, Message> {
    canvas(Spinner).width(16).height(16).into()
}

/// The top-bar scan tally: the file count after a stacked-layers glyph, the
/// collected size after a pie-chart glyph, both in the muted chrome style and
/// monospace so the figures don't jitter. Shared by the running-scan and
/// finished-map screens so the numbers stay on screen after the scan ends.
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
                .find(|&(brick, _)| brick == diskmap::Brick::Node(active))
                // A brick too small for a caption gets no actions panel either.
                .filter(|&(_, rect)| diskmap::has_label(tree, active, rect))
                .map(|(_, rect)| brick_actions(app, active, rect, size))
        });
        // The canvas always sits in the stack, panel or not: moving it
        // between a bare element and a stack child rebuilds the widget
        // tree and wipes the canvas state — a click clears `active`,
        // removes the panel, and the navigation zoom would lose its
        // springs (and snap) in that very frame.
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
    fn toggle_flips_theme() {
        // The default mode is Mode::None — rendered light, so the first
        // toggle lands on dark.
        let mut app = test_app();
        let _ = update(&mut app, Message::ToggleTheme);
        assert_eq!(theme(&app), Theme::Dark);
        let _ = update(&mut app, Message::ToggleTheme);
        assert_eq!(theme(&app), *LIGHT_THEME);
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
        assert_eq!(strings(&app).scan, "Сканировать");
    }

    #[test]
    fn language_menu_expands_and_resets_on_reopen() {
        let mut app = test_app();
        app.system_lang = Lang::EnUs;
        let _ = update(&mut app, Message::LanguageMenuToggled);
        // The menu opens short; "…" expands it to the full list.
        assert!(!app.lang_menu_expanded);
        let _ = update(&mut app, Message::LanguageMenuExpanded);
        assert!(app.lang_menu_expanded);
        // Closing and reopening lands on the short list again.
        let _ = update(&mut app, Message::LanguageMenuToggled);
        let _ = update(&mut app, Message::LanguageMenuToggled);
        assert!(app.lang_menu_open);
        assert!(!app.lang_menu_expanded);
    }

    #[test]
    fn language_menu_opens_expanded_for_an_extended_language() {
        // A language living only in the full list must be visible (and
        // highlighted) right away — the short list would hide it.
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
        // Either choice must save the file whole, not clobber the other.
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
        assert_eq!(strings(&app).scan, "Scannen");
        let _ = update(&mut app, Message::LanguagePicked(Lang::Tr));
        assert_eq!(strings(&app).scan, "Tara");
    }

    #[test]
    fn manual_theme_survives_system_change() {
        let mut app = test_app();
        let _ = update(&mut app, Message::SystemThemeChanged(Mode::Light));
        let _ = update(&mut app, Message::ToggleTheme);
        assert_eq!(theme(&app), Theme::Dark);
        // The OS flipping its preference must not undo the user's choice.
        let _ = update(&mut app, Message::SystemThemeChanged(Mode::Light));
        assert_eq!(theme(&app), Theme::Dark);
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
    fn size_percent_formats_zero_and_thresholds() {
        let cases = [
            (0, 0, "0%"),
            (0, 100, "0.00%"),
            (5, 1000, "0.50%"),
            (15, 1000, "1.5%"),
            (95, 1000, "9.5%"),
            (100, 1000, "10%"),
            (999, 1000, "100%"),
            // Rounding at the finer precision crosses a threshold: the
            // printed value decides, so no "10.0%" or "1.00%" boundary leaks.
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
    fn initial_app_lists_disk_roots() {
        // Whatever set of volumes the OS reports (empty is legal on
        // Windows), the start screen must mirror it exactly.
        assert_eq!(test_app().disk_roots, disk::mounted_roots());
    }

    #[test]
    fn window_focus_refreshes_disk_roots() {
        // A volume mounted while the app was in the background must appear
        // in the quick disk row when the user comes back — on any screen,
        // unlike the usage bar, which only lives on the finished map. The
        // sentinel no OS reports proves the focus handler replaced the list.
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
        // A mount point can carry a line break (`\012` in /proc/mounts);
        // such a path normalizes to blank and must not start a scan of ""
        // or enter the history.
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
