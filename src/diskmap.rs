//! Disk map canvas widget: drawing bricks with labels and nested
//! silhouettes, hit-testing and active brick highlighting.
//! Map geometry is cached in a `canvas::Cache` (analog of the original's offscreen Bitmap),
//! the highlight is drawn as a separate layer on top.

use iced::widget::canvas::{self, Action, Event, Frame, Path, Stroke, Text};
use iced::{Color, Pixels, Point, Rectangle, Size, mouse};

use crate::Message;
use crate::fs_tree::{FsTree, NodeId};
use crate::treemap::{NESTED_DIVISOR, TOP_LEVEL_DIVISOR, layout, normalize_weight};

const FOLDER_FILL: Color = Color::from_rgb8(0xF9, 0xA8, 0x25);
const FOLDER_STROKE: Color = Color::from_rgb8(0x58, 0x2B, 0x04);
const FILE_FILL: Color = Color::from_rgb8(0x4D, 0xB6, 0xAC);
const FILE_STROKE: Color = Color::from_rgb8(0x00, 0x4D, 0x40);
const NESTED_FOLDER_FILL: Color = Color::from_rgb8(0xFB, 0xC0, 0x2D);
// ARGB #4080CBC4 from the original: alpha 0x40 ≈ 0.25.
const NESTED_FILE_FILL: Color = Color::from_rgba8(0x80, 0xCB, 0xC4, 0.25);
const HIGHLIGHT: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.5);

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

impl DiskMap<'_> {
    /// First-level layout: children of the current node (already sorted
    /// by size, descending) in local canvas coordinates.
    fn level1(&self, size: Size) -> Vec<(NodeId, Rectangle)> {
        let node = self.tree.node(self.current);
        let weights: Vec<f32> = node
            .children
            .iter()
            .map(|&id| normalize_weight(self.tree.node(id).size))
            .collect();
        let rects = layout(&weights, Rectangle::with_size(size), TOP_LEVEL_DIVISOR);
        node.children.iter().copied().zip(rects).collect()
    }

    fn hit_test(&self, size: Size, point: Point) -> Option<NodeId> {
        self.level1(size)
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(id, _)| id)
    }

    fn draw_map(&self, frame: &mut Frame) {
        for (id, rect) in self.level1(frame.size()) {
            self.draw_brick(frame, id, rect);
        }
    }

    fn draw_brick(&self, frame: &mut Frame, id: NodeId, rect: Rectangle) {
        let node = self.tree.node(id);
        let (fill, stroke) = if node.is_dir {
            (FOLDER_FILL, FOLDER_STROKE)
        } else {
            (FILE_FILL, FILE_STROKE)
        };
        // A folder is a rounded rect, a file a plain one, as in the original.
        let path = if node.is_dir {
            Path::rounded_rectangle(rect.position(), rect.size(), CORNER_RADIUS.into())
        } else {
            Path::rectangle(rect.position(), rect.size())
        };
        frame.fill(&path, fill);
        frame.stroke(&path, Stroke::default().with_color(stroke).with_width(1.0));

        let label = if node.is_dir {
            format!(
                "{} {} ({})",
                node.name,
                crate::format::human_size(node.size),
                node.children.len()
            )
        } else {
            format!("{} {}", node.name, crate::format::human_size(node.size))
        };
        let font_size = self.draw_label(frame, &label, rect);

        if node.is_dir {
            self.draw_nested(frame, node.children.as_slice(), rect, font_size);
        }
    }

    /// Brick label; the font size is fitted per brick rather than globally
    /// (fixes a bug of the original). Returns the font size used.
    fn draw_label(&self, frame: &mut Frame, label: &str, rect: Rectangle) -> f32 {
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
            color: Color::WHITE,
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
        for (&id, r) in children
            .iter()
            .zip(layout(&weights, content, NESTED_DIVISOR))
        {
            let silhouette = Rectangle {
                x: r.x + SILHOUETTE_MARGIN,
                y: r.y,
                width: (r.width - SILHOUETTE_MARGIN).max(0.0),
                height: (r.height - SILHOUETTE_MARGIN).max(0.0),
            };
            let fill = if self.tree.node(id).is_dir {
                NESTED_FOLDER_FILL
            } else {
                NESTED_FILE_FILL
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
    fn label_skipped_when_brick_too_small() {
        // Height under font size + 8 or width under two font sizes — no label.
        assert_eq!(label_font_size(10, rect(200.0, 15.0)), None);
        assert_eq!(label_font_size(10, rect(20.0, 100.0)), None);
    }
}

impl canvas::Program<Message> for DiskMap<'_> {
    type State = ();

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
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
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
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let map = self
            .cache
            .draw(renderer, bounds.size(), |frame| self.draw_map(frame));
        let mut layers = vec![map];

        if let Some(active) = self.active
            && let Some((_, rect)) = self
                .level1(bounds.size())
                .into_iter()
                .find(|&(id, _)| id == active)
        {
            let mut frame = Frame::new(renderer, bounds.size());
            let path =
                Path::rounded_rectangle(rect.position(), rect.size(), CORNER_RADIUS.into());
            frame.fill(&path, HIGHLIGHT);
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
