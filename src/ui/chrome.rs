//! Shared chrome primitives: styled fills, themed icon helpers, icon buttons.

use iced::widget::{button, container, progress_bar, row, svg, text, tooltip};
use iced::{Border, Center, Color, Element, Shadow, Theme, Vector};

use crate::Message;

pub(crate) fn chrome_button(theme: &Theme, status: button::Status) -> button::Style {
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

pub(crate) fn bar_style(theme: &Theme) -> container::Style {
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

pub(crate) fn tooltip_style(theme: &Theme) -> container::Style {
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

pub(crate) fn muted_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(muted_color(theme)),
    }
}

pub(crate) fn muted_color(theme: &Theme) -> Color {
    if theme.extended_palette().is_dark {
        Color::from_rgb8(0xAA, 0xAA, 0xAA)
    } else {
        Color::from_rgb8(0x77, 0x77, 0x77)
    }
}

pub(crate) fn disk_usage_progress_style(theme: &Theme) -> progress_bar::Style {
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

/// SVG icon tinted with the theme's text color.
pub(crate) fn themed_icon<'a>(icon: &'static [u8]) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(icon)).style(|theme: &Theme, _status| svg::Style {
        color: Some(theme.palette().text),
    })
}

/// Like [`themed_icon`], but tinted with the muted caption color.
pub(crate) fn muted_icon<'a>(icon: &'static [u8]) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(icon)).style(|theme: &Theme, _status| svg::Style {
        color: Some(muted_color(theme)),
    })
}

/// Like [`themed_icon`], but muted for disabled controls.
pub(crate) fn themed_icon_maybe_disabled<'a>(icon: &'static [u8], disabled: bool) -> svg::Svg<'a> {
    svg(svg::Handle::from_memory(icon)).style(move |theme: &Theme, _status| svg::Style {
        color: Some(if disabled {
            muted_color(theme)
        } else {
            theme.palette().text
        }),
    })
}

pub(crate) fn chrome_icon_button<'a>(
    icon: &'static [u8],
    label: &'a str,
    on_press: Message,
) -> Element<'a, Message> {
    // Labels never wrap; long translations must shorten instead.
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

/// Icon-only chrome button; tooltip names the action. The empty text keeps the
/// button height equal to the labeled `chrome_icon_button` beside it.
pub(crate) fn chrome_icon_only_button<'a>(
    icon: &'static [u8],
    tip: &'a str,
    on_press: Message,
) -> Element<'a, Message> {
    chrome_icon_only_button_maybe(icon, tip, Some(on_press))
}

/// Like [`chrome_icon_only_button`], but a missing action also mutes the icon.
pub(crate) fn chrome_icon_only_button_maybe<'a>(
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

/// Icon button with a tooltip; caller supplies the styled icon.
pub(crate) fn action_button<'a>(
    icon: svg::Svg<'a>,
    tip: &'a str,
    on_press: Option<Message>,
) -> Element<'a, Message> {
    tooltip(
        button(icon.width(18).height(18))
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
