//! Hover actions panel pinned to the active brick.

use iced::widget::{container, row, svg};
use iced::{Element, Padding, Rectangle, Size, Theme};

use crate::ui::chrome::action_button;
use crate::{App, FOLDER_ICON, Message, ScanState, TRASH_ICON, diskmap};

/// Approximate panel size, used to clamp its position inside the canvas.
const ACTIONS_WIDTH: f32 = 58.0;
const ACTIONS_HEIGHT: f32 = 30.0;

pub(crate) fn brick_actions(
    app: &App,
    target: crate::fs_tree::NodeId,
    brick: Rectangle,
    bounds: Size,
) -> Element<'_, Message> {
    // Deleting mid-scan would desync the tree from the scanner's arena.
    let deletable = matches!(&app.scan, ScanState::Done).then_some(target);
    let s = app.strings();
    let is_dir = app.tree.as_ref().is_some_and(|tree| tree.node(target).is_dir);
    let panel = container(
        row![
            action_button(
                brick_icon(FOLDER_ICON, is_dir),
                s.open_in_file_manager,
                Some(Message::Reveal(target)),
            ),
            action_button(
                brick_icon(TRASH_ICON, is_dir),
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

/// Like [`crate::ui::chrome::themed_icon`], but tinted with a brick's caption color.
fn brick_icon<'a>(icon: &'static [u8], is_dir: bool) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(icon)).style(move |theme: &Theme, _status| svg::Style {
        color: Some(diskmap::brick_text_color(theme, is_dir)),
    })
}
