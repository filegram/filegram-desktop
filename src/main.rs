mod format;
mod fs_tree;
mod scanner;
mod treemap;

use iced::widget::{center, text};
use iced::Element;

#[derive(Default)]
struct App;

#[derive(Debug, Clone)]
enum Message {}

fn update(_app: &mut App, _message: Message) {}

fn view(_app: &App) -> Element<'_, Message> {
    center(text("Hello, world!").size(40)).into()
}

fn main() -> iced::Result {
    iced::application(App::default, update, view)
        .title("Filegram")
        .run()
}
