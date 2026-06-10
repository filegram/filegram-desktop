mod diskmap;
mod format;
mod fs_tree;
mod scanner;
mod treemap;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::widget::{button, canvas, center, column, container, row, text, text_input};
use iced::{Element, Fill, Size, Task};

use diskmap::DiskMap;
use fs_tree::{FsTree, NodeId};
use scanner::ScanEvent;

/// Максимум символов в строке пути над картой (далее — `/../`-сжатие).
const PATH_BAR_MAX_CHARS: usize = 80;

struct App {
    tree: Option<Arc<FsTree>>,
    current: NodeId,
    /// Стек навигации вглубь — аналог brickStack оригинала.
    nav_stack: Vec<NodeId>,
    active: Option<NodeId>,
    scan: ScanState,
    path_input: String,
    cache: canvas::Cache,
    cancel: Arc<AtomicBool>,
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
            scan: ScanState::Idle,
            path_input: home,
            cache: canvas::Cache::new(),
            cancel: Arc::new(AtomicBool::new(false)),
        },
        Task::none(),
    )
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
                ScanEvent::Finished(tree) => {
                    app.current = tree.root;
                    app.tree = Some(tree);
                    app.nav_stack.clear();
                    app.active = None;
                    app.scan = ScanState::Done;
                    app.cache.clear();
                }
                ScanEvent::Canceled => app.scan = ScanState::Idle,
            }
            Task::none()
        }
        Message::BrickPressed(id) => {
            let Some(tree) = &app.tree else {
                return Task::none();
            };
            let node = tree.node(id);
            if node.is_dir {
                // Пустая папка — игнор (в оригинале Toast).
                if !node.children.is_empty() {
                    app.nav_stack.push(app.current);
                    app.current = id;
                    app.active = None;
                    app.cache.clear();
                }
            } else {
                // Файл — открыть внешним приложением.
                let _ = open::that_detached(&node.path);
            }
            Task::none()
        }
        Message::SetActive(id) => {
            // Подсветка вне кэша карты — clear не нужен.
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
    }
}

fn view(app: &App) -> Element<'_, Message> {
    match &app.scan {
        ScanState::Idle => idle_view(app),
        ScanState::Running { current, files } => running_view(current, *files),
        ScanState::Done => map_view(app),
    }
}

fn idle_view(app: &App) -> Element<'_, Message> {
    center(
        column![
            text("Filegram — карта диска").size(28),
            row![
                text_input("Путь к каталогу…", &app.path_input)
                    .on_input(Message::PathChanged)
                    .on_submit(Message::StartScan),
                button(text("Сканировать")).on_press(Message::StartScan),
            ]
            .spacing(8),
        ]
        .spacing(16)
        .max_width(600),
    )
    .into()
}

fn running_view(current: &str, files: u64) -> Element<'_, Message> {
    center(
        column![
            text(format!("Сканирование… файлов: {files}")).size(20),
            text(format::shorten_path(current, PATH_BAR_MAX_CHARS)),
            button(text("Отмена")).on_press(Message::CancelScan),
        ]
        .spacing(16),
    )
    .into()
}

fn map_view(app: &App) -> Element<'_, Message> {
    let Some(tree) = &app.tree else {
        return idle_view(app);
    };
    let current_path = tree.node(app.current).path.display().to_string();
    let bar = row![
        button(text("← Назад"))
            .on_press_maybe((!app.nav_stack.is_empty()).then_some(Message::GoBack)),
        container(text(format::shorten_path(&current_path, PATH_BAR_MAX_CHARS)))
            .width(Fill)
            .padding(8),
        button(text("Новый скан")).on_press(Message::NewScan),
    ]
    .spacing(8)
    .padding(8);

    let map = canvas(DiskMap {
        tree,
        current: app.current,
        active: app.active,
        cache: &app.cache,
    })
    .width(Fill)
    .height(Fill);

    column![bar, map].into()
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("Filegram")
        .window_size(Size::new(1024.0, 768.0))
        .run()
}
