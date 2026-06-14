//! The start (idle) screen: the path input and Scan button, the quick-folder
//! and quick-disk rows, the scan history, the language popup and the bottom
//! corner footer (theme toggle, language menu, version).

use std::path::{Path, PathBuf};

use iced::widget::{
    button, center, column, container, mouse_area, opaque, row, scrollable, space, stack, text,
    text_input,
};
use iced::{Center, Element, Fill, Padding, Theme};

use crate::ui::chrome::{action_button, chrome_icon_button, muted_text, themed_icon, tooltip_style};
use crate::{
    App, BRICKS_ICON, DESKTOP_ICON, DISC_ICON, DOCUMENTS_ICON, DOWNLOADS_ICON, DRIVE_ICON,
    GLOBE_ICON, HOME_ICON, MOON_ICON, Message, PATH_BAR_MAX_CHARS, SUN_ICON, USB_ICON, disk, format,
    history, i18n,
};
use i18n::Lang;

pub(crate) fn idle_view(app: &App) -> Element<'_, Message> {
    let s = app.strings();
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
    let current = app.lang();
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
            text(app.strings().disks).size(14).style(muted_text),
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

/// The scan history under the path input: a click rescans the path, and
/// hovering a row reveals a trailing cross that removes the entry.
fn recent_scans(app: &App) -> Element<'_, Message> {
    column![text(app.strings().recent_scans).size(14).style(muted_text)]
        .spacing(2)
        .extend(app.history.entries().iter().map(|path| recent_scan_row(app, path)))
        .into()
}

/// One history row. A click on the path rescans it; the row fills the column
/// width so the delete cross lands on the same right edge for every entry,
/// and the `mouse_area` spans that whole width so the hover target is the
/// entire row, not just the path text. The cross renders only for the hovered
/// row — at the same font size and vertical padding as the path, so revealing
/// it never changes the row height and the list below it stays put.
fn recent_scan_row<'a>(app: &'a App, path: &'a str) -> Element<'a, Message> {
    let scan = button(text(format::shorten_path(path, PATH_BAR_MAX_CHARS)).size(14))
        .style(button::text)
        .padding(4)
        .on_press(Message::HistoryPicked(path.to_string()));
    let mut entry = row![scan, space().width(Fill)].align_y(Center);
    if app.hovered_history.as_deref() == Some(path) {
        entry = entry.push(
            button(text("×").size(14))
                .style(button::text)
                .padding(Padding {
                    top: 4.0,
                    right: 8.0,
                    bottom: 4.0,
                    left: 8.0,
                })
                .on_press(Message::HistoryRemoved(path.to_string())),
        );
    }
    mouse_area(entry.width(Fill))
        .on_enter(Message::HistoryHovered(Some(path.to_string())))
        .on_exit(Message::HistoryHovered(None))
        .into()
}

/// The theme toggle: an icon button showing the mode a click switches to.
fn theme_toggle(app: &App) -> Element<'_, Message> {
    let s = app.strings();
    let (icon, tip) = if app.is_dark() {
        (SUN_ICON, s.light_theme)
    } else {
        (MOON_ICON, s.dark_theme)
    };
    action_button(themed_icon(icon), tip, Some(Message::ToggleTheme))
}

/// The language menu trigger: the same square icon button as the theme
/// toggle, a globe with the localized "Language" hint.
fn language_button(app: &App) -> Element<'_, Message> {
    action_button(
        themed_icon(GLOBE_ICON),
        app.strings().language,
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
