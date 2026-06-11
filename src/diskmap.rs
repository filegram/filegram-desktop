//! Disk map canvas widget: drawing bricks with labels and nested
//! silhouettes, hit-testing and active brick highlighting.
//! Map geometry is cached in a `canvas::Cache` (analog of the original's offscreen Bitmap),
//! the highlight is drawn as a separate layer on top.

use std::cell::Cell;

use iced::widget::canvas::{self, Action, Event, Frame, Path, Stroke, Text};
use iced::{Color, Pixels, Point, Rectangle, Size, mouse};

use crate::Message;
use crate::fs_tree::{FsTree, NodeId};
use crate::treemap::{layout, normalize_weight};

/// Brick colors for one theme mode; the variant is picked per frame from
/// the application theme (the chrome follows the system light/dark scheme).
struct BrickPalette {
    map_background: Color,
    folder_fill: Color,
    folder_stroke: Color,
    folder_text: Color,
    file_fill: Color,
    file_stroke: Color,
    file_text: Color,
    nested_folder_fill: Color,
    nested_file_fill: Color,
    highlight: Color,
}

/// The palette of the original Android Disk Map: saturated fills, white labels.
const DARK_PALETTE: BrickPalette = BrickPalette {
    map_background: Color::from_rgb8(0x1C, 0x1C, 0x1C),
    folder_fill: Color::from_rgb8(0xF9, 0xA8, 0x25),
    folder_stroke: Color::from_rgb8(0x58, 0x2B, 0x04),
    folder_text: Color::WHITE,
    file_fill: Color::from_rgb8(0x4D, 0xB6, 0xAC),
    file_stroke: Color::from_rgb8(0x00, 0x4D, 0x40),
    file_text: Color::WHITE,
    nested_folder_fill: Color::from_rgb8(0xFB, 0xC0, 0x2D),
    // ARGB #4080CBC4 from the original: alpha 0x40 ≈ 0.25.
    nested_file_fill: Color::from_rgba8(0x80, 0xCB, 0xC4, 0.25),
    highlight: Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.5),
};

/// Muted fills with dark labels for the light system theme.
const LIGHT_PALETTE: BrickPalette = BrickPalette {
    map_background: Color::from_rgb8(0xD8, 0xD8, 0xD8),
    folder_fill: Color::from_rgb8(0xFF, 0xE0, 0x82),
    folder_stroke: Color::from_rgb8(0xBA, 0x75, 0x17),
    folder_text: Color::from_rgb8(0x63, 0x38, 0x06),
    file_fill: Color::from_rgb8(0xB2, 0xDF, 0xDB),
    file_stroke: Color::from_rgb8(0x0F, 0x6E, 0x56),
    file_text: Color::from_rgb8(0x04, 0x34, 0x2C),
    nested_folder_fill: Color::from_rgb8(0xFF, 0xEC, 0xB3),
    nested_file_fill: Color::from_rgba8(0x80, 0xCB, 0xC4, 0.4),
    highlight: Color::from_rgba8(0x00, 0x00, 0x00, 0.25),
};

fn palette(theme: &iced::Theme) -> &'static BrickPalette {
    if theme.extended_palette().is_dark {
        &DARK_PALETTE
    } else {
        &LIGHT_PALETTE
    }
}

/// Margin between the canvas edges and the brick area.
const MAP_MARGIN: f32 = 4.0;
const CORNER_RADIUS: f32 = 8.0;
const MAX_FONT: f32 = 28.0;
const MIN_FONT: f32 = 12.0;
/// Empirical average glyph width as a fraction of the font size — for fitting the font
/// per brick (canvas offers no cheap way to measure text).
const CHAR_WIDTH: f32 = 0.6;
/// Minimum side length of the nested-content area.
const MIN_CONTENT_SIDE: f32 = 12.0;
/// Margin of a nested silhouette (left and bottom).
const SILHOUETTE_MARGIN: f32 = 6.0;

pub struct DiskMap<'a> {
    pub tree: &'a FsTree,
    pub current: NodeId,
    pub active: Option<NodeId>,
    pub cache: &'a canvas::Cache,
}

/// First-level layout: children of `current` (already sorted by size,
/// descending) in local canvas coordinates. Public so the application can
/// anchor overlays (the hover actions panel) to a brick's rectangle.
pub fn level1(tree: &FsTree, current: NodeId, size: Size) -> Vec<(NodeId, Rectangle)> {
    let node = tree.node(current);
    let weights: Vec<f32> = node
        .children
        .iter()
        .map(|&id| normalize_weight(tree.node(id).size))
        .collect();
    let rects = layout(&weights, map_bounds(size));
    node.children.iter().copied().zip(rects).collect()
}

/// The brick caption: name and size, plus the entry count for folders.
fn brick_label(tree: &FsTree, id: NodeId) -> String {
    let node = tree.node(id);
    if node.is_dir {
        format!(
            "{} {} ({})",
            node.name,
            crate::format::human_size(node.size),
            node.children.len()
        )
    } else {
        format!("{} {}", node.name, crate::format::human_size(node.size))
    }
}

/// Whether the brick is large enough to fit its caption. Tiny slivers get
/// no label, and the hover actions panel is suppressed for them too.
pub fn has_label(tree: &FsTree, id: NodeId, rect: Rectangle) -> bool {
    label_font_size(brick_label(tree, id).chars().count(), rect).is_some()
}

/// The brick area: the canvas inset by [`MAP_MARGIN`] on every side.
fn map_bounds(size: Size) -> Rectangle {
    Rectangle {
        x: MAP_MARGIN,
        y: MAP_MARGIN,
        width: (size.width - 2.0 * MAP_MARGIN).max(0.0),
        height: (size.height - 2.0 * MAP_MARGIN).max(0.0),
    }
}

impl DiskMap<'_> {
    fn hit_test(&self, size: Size, point: Point) -> Option<NodeId> {
        level1(self.tree, self.current, size)
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(id, _)| id)
    }

    fn draw_map(&self, frame: &mut Frame, palette: &BrickPalette) {
        frame.fill_rectangle(Point::ORIGIN, frame.size(), palette.map_background);
        for (id, rect) in level1(self.tree, self.current, frame.size()) {
            self.draw_brick(frame, palette, id, rect);
        }
    }

    fn draw_brick(&self, frame: &mut Frame, palette: &BrickPalette, id: NodeId, rect: Rectangle) {
        let node = self.tree.node(id);
        let (fill, stroke, text_color) = if node.is_dir {
            (
                palette.folder_fill,
                palette.folder_stroke,
                palette.folder_text,
            )
        } else {
            (palette.file_fill, palette.file_stroke, palette.file_text)
        };
        let path = Path::rounded_rectangle(rect.position(), rect.size(), CORNER_RADIUS.into());
        frame.fill(&path, fill);
        frame.stroke(&path, Stroke::default().with_color(stroke).with_width(1.0));

        let label = brick_label(self.tree, id);
        let font_size = self.draw_label(frame, &label, text_color, rect);

        if node.is_dir {
            self.draw_nested(frame, palette, node.children.as_slice(), rect, font_size);
        }
    }

    /// Brick label; the font size is fitted per brick rather than globally
    /// (fixes a bug of the original). Returns the font size used.
    fn draw_label(&self, frame: &mut Frame, label: &str, color: Color, rect: Rectangle) -> f32 {
        // Count characters, not bytes: Cyrillic takes 2 bytes per glyph in UTF-8.
        let char_count = label.chars().count().max(1);
        let Some(font_size) = label_font_size(char_count, rect) else {
            return 0.0;
        };
        // If even the minimum size does not fit, truncate the text to the width.
        let max_chars = (rect.width / (CHAR_WIDTH * font_size)) as usize;
        let content: String = label.chars().take(max_chars).collect();
        frame.fill_text(Text {
            content,
            position: label_origin(rect),
            color,
            size: Pixels(font_size),
            shaping: iced::widget::text::Shaping::Advanced,
            ..Text::default()
        });
        font_size
    }

    /// Nested second-level silhouettes: colored rectangles without text.
    fn draw_nested(
        &self,
        frame: &mut Frame,
        palette: &BrickPalette,
        children: &[NodeId],
        rect: Rectangle,
        font_size: f32,
    ) {
        // Insets for the header: top += 4 + textSize + 4; left += 1; right −= 8.
        let content = Rectangle {
            x: rect.x + 1.0,
            y: rect.y + 4.0 + font_size + 4.0,
            width: rect.width - 1.0 - 8.0,
            height: rect.height - (4.0 + font_size + 4.0),
        };
        if content.width < MIN_CONTENT_SIDE || content.height < MIN_CONTENT_SIDE {
            return;
        }
        let weights: Vec<f32> = children
            .iter()
            .map(|&id| normalize_weight(self.tree.node(id).size))
            .collect();
        for (&id, r) in children.iter().zip(layout(&weights, content)) {
            let silhouette = Rectangle {
                x: r.x + SILHOUETTE_MARGIN,
                y: r.y,
                width: (r.width - SILHOUETTE_MARGIN).max(0.0),
                height: (r.height - SILHOUETTE_MARGIN).max(0.0),
            };
            let fill = if self.tree.node(id).is_dir {
                palette.nested_folder_fill
            } else {
                palette.nested_file_fill
            };
            frame.fill_rectangle(silhouette.position(), silhouette.size(), fill);
        }
    }
}

/// Label font size for the brick width; `None` — the brick is too small for a label.
/// The font size is strictly integral: cosmic-text rasterizes glyphs separately for
/// each f32 size (`CacheKey::font_size_bits`), and fractional sizes — distinct for
/// every brick and every progressive-scan snapshot — overflow iced's glyph
/// atlas (`PrepareError::AtlasFull` is silently ignored, text breaks).
fn label_font_size(char_count: usize, rect: Rectangle) -> Option<f32> {
    let fit = rect.width / (CHAR_WIDTH * char_count.max(1) as f32);
    // Down to an even integer: fewer distinct font sizes — a more stable atlas.
    let font_size = (fit.clamp(MIN_FONT, MAX_FONT) / 2.0).floor() * 2.0;
    (rect.height >= font_size + 8.0 && rect.width >= 2.0 * font_size).then_some(font_size)
}

/// Label origin aligned to whole pixels: a fractional position changes
/// each glyph's subpixel bin (`CacheKey::{x_bin,y_bin}`), while bricks
/// shift slightly on every scan snapshot — without rounding the same
/// letters are re-rasterized into new bins, and the glyph atlas keeps
/// evicting/reallocating regions (see the comment on [`label_font_size`]).
fn label_origin(rect: Rectangle) -> Point {
    Point::new((rect.x + 4.0).round(), (rect.y + 4.0).round())
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{Point, Size};

    fn rect(width: f32, height: f32) -> Rectangle {
        Rectangle::new(Point::ORIGIN, Size::new(width, height))
    }

    #[test]
    fn font_size_is_whole_even_pixels() {
        // fit = 200 / (0.6 · 15) = 22.22… — the font size rounds down to an even
        // integer: each distinct f32 size is rasterized in the glyph atlas
        // separately, and the fewer distinct sizes, the more stable the atlas.
        assert_eq!(label_font_size(15, rect(200.0, 100.0)), Some(22.0));
        // fit = 210 / (0.6 · 14) = 25.0 — an odd value is pushed down to 24.
        assert_eq!(label_font_size(14, rect(210.0, 100.0)), Some(24.0));
    }

    #[test]
    fn label_origin_is_whole_pixels() {
        // A fractional text position changes each glyph's subpixel bin;
        // bricks shift on every scan snapshot, and without rounding
        // every snapshot re-rasterizes the same letters into new bins.
        let brick = Rectangle::new(Point::new(10.6, 20.4), Size::new(100.0, 50.0));
        let origin = label_origin(brick);
        assert_eq!((origin.x, origin.y), (15.0, 24.0));
    }

    #[test]
    fn font_size_clamped_to_limits() {
        assert_eq!(label_font_size(5, rect(1000.0, 100.0)), Some(MAX_FONT));
        assert_eq!(label_font_size(40, rect(100.0, 100.0)), Some(MIN_FONT));
    }

    #[test]
    fn label_decides_actions_panel_visibility() {
        use crate::fs_tree::ScanNode;
        let tree = FsTree::from_arena(&[
            ScanNode {
                name: "root".into(),
                path: std::path::Path::new("/root").into(),
                size: 0,
                is_dir: true,
                parent: 0,
            },
            ScanNode {
                name: "data.bin".into(),
                path: std::path::Path::new("/root/data.bin").into(),
                size: 100,
                is_dir: false,
                parent: 0,
            },
        ]);
        let brick = NodeId(1);
        assert!(has_label(&tree, brick, rect(300.0, 100.0)));
        // A sliver brick: no caption fits — the hover actions are suppressed.
        assert!(!has_label(&tree, brick, rect(24.0, 10.0)));
    }

    #[test]
    fn map_bounds_inset_by_margin() {
        let bounds = map_bounds(Size::new(100.0, 60.0));
        assert_eq!(
            (bounds.x, bounds.y, bounds.width, bounds.height),
            (
                MAP_MARGIN,
                MAP_MARGIN,
                100.0 - 2.0 * MAP_MARGIN,
                60.0 - 2.0 * MAP_MARGIN
            )
        );
        // A canvas smaller than the margins must not produce negative sizes.
        let tiny = map_bounds(Size::new(MAP_MARGIN, MAP_MARGIN));
        assert_eq!((tiny.width, tiny.height), (0.0, 0.0));
    }

    #[test]
    fn palette_follows_theme_mode() {
        assert_eq!(
            palette(&iced::Theme::Dark).folder_text,
            DARK_PALETTE.folder_text
        );
        assert_eq!(
            palette(&iced::Theme::Light).folder_text,
            LIGHT_PALETTE.folder_text
        );
    }

    #[test]
    fn label_skipped_when_brick_too_small() {
        // Height under font size + 8 or width under two font sizes — no label.
        assert_eq!(label_font_size(10, rect(200.0, 15.0)), None);
        assert_eq!(label_font_size(10, rect(20.0, 100.0)), None);
    }
}

impl canvas::Program<Message> for DiskMap<'_> {
    /// The dark-mode flag of the last drawn frame: the cache keeps geometry
    /// with baked-in colors, so a system theme switch must invalidate it.
    type State = Cell<Option<bool>>;

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let hit = || {
            cursor
                .position_in(bounds)
                .and_then(|p| self.hit_test(bounds.size(), p))
        };
        match event {
            // A levitating cursor hovers the actions panel stacked above the
            // map: keep the active brick, otherwise the panel would vanish
            // right as the cursor reaches its buttons.
            Event::Mouse(mouse::Event::CursorMoved { .. }) if !cursor.is_levitating() => {
                let hovered = hit();
                (hovered != self.active).then(|| Action::publish(Message::SetActive(hovered)))
            }
            Event::Mouse(mouse::Event::CursorLeft) => self
                .active
                .is_some()
                .then(|| Action::publish(Message::SetActive(None))),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                hit().map(|id| Action::publish(Message::BrickPressed(id)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right | mouse::Button::Back,
            )) => Some(Action::publish(Message::GoBack).and_capture()),
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let palette = palette(theme);
        let is_dark = theme.extended_palette().is_dark;
        if state.replace(Some(is_dark)) != Some(is_dark) {
            self.cache.clear();
        }
        let map = self
            .cache
            .draw(renderer, bounds.size(), |frame| {
                self.draw_map(frame, palette)
            });
        let mut layers = vec![map];

        if let Some(active) = self.active
            && let Some((_, rect)) = level1(self.tree, self.current, bounds.size())
                .into_iter()
                .find(|&(id, _)| id == active)
        {
            let mut frame = Frame::new(renderer, bounds.size());
            let path =
                Path::rounded_rectangle(rect.position(), rect.size(), CORNER_RADIUS.into());
            frame.fill(&path, palette.highlight);
            layers.push(frame.into_geometry());
        }
        layers
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let over_brick = cursor
            .position_in(bounds)
            .and_then(|p| self.hit_test(bounds.size(), p))
            .is_some();
        if over_brick {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}
