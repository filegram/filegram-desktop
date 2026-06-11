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
    rest_fill: Color,
    rest_stroke: Color,
    rest_text: Color,
    nested_folder_fill: Color,
    nested_file_fill: Color,
    nested_rest_fill: Color,
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
    rest_fill: Color::from_rgb8(0x3C, 0x3C, 0x3C),
    rest_stroke: Color::from_rgb8(0x75, 0x75, 0x75),
    rest_text: Color::from_rgb8(0xB0, 0xB0, 0xB0),
    nested_folder_fill: Color::from_rgb8(0xFB, 0xC0, 0x2D),
    // ARGB #4080CBC4 from the original: alpha 0x40 ≈ 0.25.
    nested_file_fill: Color::from_rgba8(0x80, 0xCB, 0xC4, 0.25),
    nested_rest_fill: Color::from_rgba8(0x9E, 0x9E, 0x9E, 0.25),
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
    rest_fill: Color::from_rgb8(0xBD, 0xBD, 0xBD),
    rest_stroke: Color::from_rgb8(0x61, 0x61, 0x61),
    rest_text: Color::from_rgb8(0x42, 0x42, 0x42),
    nested_folder_fill: Color::from_rgb8(0xFF, 0xEC, 0xB3),
    nested_file_fill: Color::from_rgba8(0x80, 0xCB, 0xC4, 0.4),
    nested_rest_fill: Color::from_rgba8(0x75, 0x75, 0x75, 0.3),
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

/// A first-level map tile: either a real tree node or the aggregate
/// rest brick that collapses the tail of small items.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Brick {
    Node(NodeId),
    Rest { files: usize, dirs: usize, size: u64 },
}

/// Collapse threshold: a brick whose weight share (== map area share)
/// is below this fraction goes into the rest tail.
const REST_SHARE: f32 = 0.05;

/// How many trailing items to collapse by the weight-share criterion.
/// Collapsing a single item is pointless — the rest brick would occupy
/// exactly the same area — so anything under two items stays as is.
fn share_collapse_count(weights: &[f32]) -> usize {
    let total: f32 = weights.iter().sum();
    if total <= 0.0 {
        return 0;
    }
    let mut count = weights
        .iter()
        .rev()
        .take_while(|&&w| w / total < REST_SHARE)
        .count();
    // The tail must stay strictly lighter than the smallest displayed item:
    // otherwise a folder of uniform small entries would hide the whole area
    // behind the rest. While it outweighs one, the heaviest tail items are
    // released back.
    let mut rest: f32 = weights[weights.len() - count..].iter().sum();
    while count > 0
        && (count == weights.len() || rest >= weights[weights.len() - count - 1])
    {
        rest -= weights[weights.len() - count];
        count -= 1;
    }
    if count >= 2 { count } else { 0 }
}

/// The rest brick caption: combined size and the collapsed entry counts.
fn rest_label(files: usize, dirs: usize, size: u64) -> String {
    let plural = |n: usize, word: &str| {
        format!("{n} {word}{}", if n == 1 { "" } else { "s" })
    };
    let mut parts = Vec::new();
    if files > 0 {
        parts.push(plural(files, "file"));
    }
    if dirs > 0 {
        parts.push(plural(dirs, "folder"));
    }
    format!(
        "… {} ({})",
        crate::format::human_size(size),
        parts.join(", ")
    )
}

/// First-level layout: children of `current` (already sorted by size,
/// descending) in local canvas coordinates; the tail of items that are
/// tiny (share < [`REST_SHARE`]) or can't fit their caption is collapsed
/// into a single trailing [`Brick::Rest`]. Public so the application can
/// anchor overlays (the hover actions panel) to a brick's rectangle.
pub fn level1(tree: &FsTree, current: NodeId, size: Size) -> Vec<(Brick, Rectangle)> {
    let children = tree.node(current).children.as_slice();
    let weights: Vec<f32> = children
        .iter()
        .map(|&id| normalize_weight(tree.node(id).size))
        .collect();
    let total: f32 = weights.iter().sum();
    let bounds = map_bounds(size);

    // suffix[k] is the combined weight of items k.. — the would-be rest.
    let mut suffix = vec![0.0f32; weights.len() + 1];
    for k in (0..weights.len()).rev() {
        suffix[k] = suffix[k + 1] + weights[k];
    }

    // Displayed weights for a given tail size; the buffer is reused
    // across iterations to avoid re-allocating per snapshot.
    let mut shown: Vec<f32> = Vec::with_capacity(weights.len());
    let mut rects_for = |collapsed: usize| {
        let kept = weights.len() - collapsed;
        shown.clear();
        shown.extend_from_slice(&weights[..kept]);
        if collapsed > 0 {
            shown.push(suffix[kept]);
        }
        layout(&shown, bounds)
    };

    // Phase 1 — unreadable bricks: collapse the tail from the first brick
    // whose caption cannot fit. The tail only grows, so the loop converges
    // in ≤ n steps (2–3 in practice: each step jumps straight to the first
    // unlabeled brick); after every extension the layout is recomputed and
    // the remaining bricks are re-checked in their new rectangles.
    let mut collapsed = 0;
    loop {
        let rects = rects_for(collapsed);
        let kept = children.len() - collapsed;
        let grown = match (0..kept).find(|&i| !has_label(tree, children[i], rects[i])) {
            Some(first_unlabeled) => children.len() - first_unlabeled,
            None => collapsed,
        }
        .max(collapsed);
        if grown == collapsed {
            break;
        }
        collapsed = grown;
    }

    // Phase 2 — small shares: the tail extends with items whose share is
    // below [`REST_SHARE`], but the rest must stay strictly smaller than
    // the smallest displayed brick — otherwise a folder of near-equal items
    // (or a mid-scan snapshot, while folder aggregates are still counting
    // up) would be swallowed whole; heavier items stay regular bricks.
    // The unreadable tail from phase 1 is exempt from this cap, but not
    // from the under-two rule below.
    let share_tail = weights
        .iter()
        .rev()
        .take_while(|&&w| w / total < REST_SHARE)
        .count();
    let mut target = collapsed.max(share_tail);
    while target > collapsed
        && (target == children.len()
            || suffix[children.len() - target] >= weights[children.len() - target - 1])
    {
        target -= 1;
    }

    // A single-item tail is not worth a rest brick: it would occupy the
    // exact same rectangle, equally unlabeled, while a regular brick at
    // least stays clickable — so anything under two items is shown as is.
    let collapsed = if target >= 2 { target } else { 0 };
    let rects = rects_for(collapsed);
    let kept = children.len() - collapsed;
    let mut bricks: Vec<Brick> = children[..kept].iter().map(|&id| Brick::Node(id)).collect();
    if collapsed > 0 {
        let (mut files, mut dirs, mut size) = (0, 0, 0u64);
        for &id in &children[kept..] {
            let node = tree.node(id);
            if node.is_dir {
                dirs += 1;
            } else {
                files += 1;
            }
            size += node.size;
        }
        bricks.push(Brick::Rest { files, dirs, size });
    }
    bricks.into_iter().zip(rects).collect()
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
    /// The rest brick is inert: the cursor over it hits nothing.
    fn hit_test(&self, size: Size, point: Point) -> Option<NodeId> {
        level1(self.tree, self.current, size)
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .and_then(|(brick, _)| match brick {
                Brick::Node(id) => Some(id),
                Brick::Rest { .. } => None,
            })
    }

    fn draw_map(&self, frame: &mut Frame, palette: &BrickPalette) {
        frame.fill_rectangle(Point::ORIGIN, frame.size(), palette.map_background);
        for (brick, rect) in level1(self.tree, self.current, frame.size()) {
            match brick {
                Brick::Node(id) => self.draw_brick(frame, palette, id, rect),
                Brick::Rest { files, dirs, size } => {
                    self.draw_rest(frame, palette, files, dirs, size, rect);
                }
            }
        }
    }

    /// The aggregate rest brick: neutral gray tones, no nested silhouettes.
    fn draw_rest(
        &self,
        frame: &mut Frame,
        palette: &BrickPalette,
        files: usize,
        dirs: usize,
        size: u64,
        rect: Rectangle,
    ) {
        let path = Path::rounded_rectangle(rect.position(), rect.size(), CORNER_RADIUS.into());
        frame.fill(&path, palette.rest_fill);
        frame.stroke(
            &path,
            Stroke::default().with_color(palette.rest_stroke).with_width(1.0),
        );
        self.draw_label(frame, &rest_label(files, dirs, size), palette.rest_text, rect);
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
        // The tail of small silhouettes collapses into one rectangle too.
        let kept = children.len() - share_collapse_count(&weights);
        let mut shown = weights[..kept].to_vec();
        if kept < children.len() {
            shown.push(weights[kept..].iter().sum());
        }
        for (i, r) in layout(&shown, content).into_iter().enumerate() {
            let silhouette = Rectangle {
                x: r.x + SILHOUETTE_MARGIN,
                y: r.y,
                width: (r.width - SILHOUETTE_MARGIN).max(0.0),
                height: (r.height - SILHOUETTE_MARGIN).max(0.0),
            };
            let fill = if i >= kept {
                palette.nested_rest_fill
            } else if self.tree.node(children[i]).is_dir {
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
    // Both sides bound the font: the width per character count and
    // the height minus the caption's vertical padding (8 px).
    let fit = (rect.width / (CHAR_WIDTH * char_count.max(1) as f32)).min(rect.height - 8.0);
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

    /// A tree: the root and its children with the given (size, is_dir).
    fn tree_with_children(entries: &[(u64, bool)]) -> FsTree {
        use crate::fs_tree::ScanNode;
        let mut arena = vec![ScanNode {
            name: "root".into(),
            path: std::path::Path::new("/root").into(),
            size: 0,
            is_dir: true,
            parent: 0,
        }];
        for (i, &(size, is_dir)) in entries.iter().enumerate() {
            arena.push(ScanNode {
                name: format!("e{i}").into(),
                path: std::path::Path::new("/root/e").into(),
                size,
                is_dir,
                parent: 0,
            });
        }
        FsTree::from_arena(&arena)
    }

    #[test]
    fn tail_of_tiny_items_collapses_into_rest() {
        use crate::fs_tree::DIR_ENTRY_SIZE;
        // Three large files and a small tail: 4 files of 100 KB and 2 empty
        // folders — every tail item's weight share is ≈1% < 5%.
        let mut entries = vec![(100_000_000, false); 3];
        entries.extend([(100_000, false); 4]);
        entries.extend([(0, true); 2]);
        let tree = tree_with_children(&entries);
        let bricks = level1(&tree, tree.root, Size::new(800.0, 500.0));
        assert_eq!(bricks.len(), 4, "{bricks:?}");
        for (brick, _) in &bricks[..3] {
            assert!(matches!(brick, Brick::Node(_)), "{brick:?}");
        }
        let (rest, rect) = bricks[3];
        assert_eq!(
            rest,
            Brick::Rest {
                files: 4,
                dirs: 2,
                size: 4 * 100_000 + 2 * DIR_ENTRY_SIZE,
            }
        );
        assert!(rect.width > 0.0 && rect.height > 0.0, "{rect:?}");
    }

    #[test]
    fn single_tiny_item_stays_a_node() {
        // A single collapse candidate: the rest brick would occupy the same
        // area, so the item stays a regular brick.
        let tree = tree_with_children(&[
            (100_000_000, false),
            (100_000_000, false),
            (100_000_000, false),
            (100_000, false),
        ]);
        let bricks = level1(&tree, tree.root, Size::new(800.0, 500.0));
        assert_eq!(bricks.len(), 4, "{bricks:?}");
        assert!(
            bricks
                .iter()
                .all(|(brick, _)| matches!(brick, Brick::Node(_))),
            "{bricks:?}"
        );
    }

    #[test]
    fn all_large_items_stay_nodes() {
        let tree = tree_with_children(&[(100_000_000, false); 4]);
        let bricks = level1(&tree, tree.root, Size::new(800.0, 500.0));
        assert_eq!(bricks.len(), 4, "{bricks:?}");
        assert!(
            bricks
                .iter()
                .all(|(brick, _)| matches!(brick, Brick::Node(_))),
            "{bricks:?}"
        );
    }

    #[test]
    fn label_unfit_tail_collapses_even_with_large_shares() {
        // A thimble-sized canvas: shares of 25%, but the 31×16 bricks are
        // below the caption minimum (height < font + 8) — everything
        // collapses into a single rest brick.
        let tree = tree_with_children(&[(100_000, false); 4]);
        let bricks = level1(&tree, tree.root, Size::new(70.0, 40.0));
        assert_eq!(bricks.len(), 1, "{bricks:?}");
        assert!(
            matches!(bricks[0].0, Brick::Rest { files: 4, dirs: 0, .. }),
            "{bricks:?}"
        );
    }

    #[test]
    fn uniform_children_are_not_swallowed_by_rest() {
        // 25 equal files: each share is 4% < 5%, but hiding the whole map
        // behind one rest brick is wrong — the heaviest tail items are
        // released back.
        let tree = tree_with_children(&[(1_000_000, false); 25]);
        let bricks = level1(&tree, tree.root, Size::new(800.0, 500.0));
        assert_eq!(bricks.len(), 25, "{bricks:?}");
        assert!(
            bricks
                .iter()
                .all(|(brick, _)| matches!(brick, Brick::Node(_))),
            "{bricks:?}"
        );
    }

    #[test]
    fn equal_tail_items_outweighing_each_other_stay_nodes() {
        // One large file and 30 equal mid-size items: a rest of any two of
        // them would outweigh each of the others — the tail does not
        // collapse at all.
        let mut entries = vec![(100_000_000, false)];
        entries.extend([(1_000_000, false); 30]);
        let tree = tree_with_children(&entries);
        let bricks = level1(&tree, tree.root, Size::new(800.0, 500.0));
        assert_eq!(bricks.len(), 31, "{bricks:?}");
        assert!(
            bricks
                .iter()
                .all(|(brick, _)| matches!(brick, Brick::Node(_))),
            "{bricks:?}"
        );
    }

    #[test]
    fn decaying_tail_collapses_into_rest_smaller_than_any_brick() {
        // A sharply decaying tail (√size weights: 1000, 100, 40, 18, 8, 3):
        // the rest = 40+18+8+3 = 69 — lighter than the smallest displayed
        // brick (100).
        let tree = tree_with_children(&[
            (1_000_000, false),
            (10_000, false),
            (1_600, false),
            (324, false),
            (64, false),
            (9, false),
        ]);
        let bricks = level1(&tree, tree.root, Size::new(800.0, 500.0));
        let &(rest, rest_rect) = bricks.last().unwrap();
        assert_eq!(
            rest,
            Brick::Rest {
                files: 4,
                dirs: 0,
                size: 1_600 + 324 + 64 + 9,
            },
            "{bricks:?}"
        );
        // Geometrically the rest brick is smaller than every regular brick.
        let rest_area = rest_rect.width * rest_rect.height;
        for (brick, r) in &bricks[..bricks.len() - 1] {
            assert!(
                rest_area < r.width * r.height,
                "{brick:?} {r:?} vs rest {rest_rect:?}"
            );
        }
    }

    #[test]
    fn rest_label_lists_size_and_counts() {
        assert_eq!(rest_label(4, 2, 408_192), "… 398.6 KB (4 files, 2 folders)");
        assert_eq!(rest_label(1, 0, 500), "… 500 B (1 file)");
        assert_eq!(rest_label(0, 3, 12_288), "… 12.0 KB (3 folders)");
    }

    #[test]
    fn share_collapse_needs_two_small_items() {
        // A tail of three sub-5% items collapses…
        assert_eq!(share_collapse_count(&[10.0, 0.3, 0.2, 0.1]), 3);
        // …while a single small item does not.
        assert_eq!(share_collapse_count(&[10.0, 0.3]), 0);
        assert_eq!(share_collapse_count(&[1.0, 1.0, 1.0]), 0);
        assert_eq!(share_collapse_count(&[]), 0);
    }

    #[test]
    fn share_collapse_keeps_rest_below_smallest_kept_item() {
        // A tail of equal items would outweigh each remaining one —
        // nothing collapses.
        let mut weights = vec![4.0];
        weights.extend([1.0; 26]);
        assert_eq!(share_collapse_count(&weights), 0);
        assert_eq!(share_collapse_count(&[1.0; 25]), 0);
        // A sharply decaying tail: 0.45 is lighter than the smallest kept
        // item (1.0).
        assert_eq!(share_collapse_count(&[4.0, 1.0, 0.2, 0.15, 0.1]), 3);
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
    fn wide_short_brick_fits_label_with_smaller_font() {
        // A wide but short brick: the width-derived font would be 20 px and
        // fail the height check (25 < 28) — the height bounds the font.
        assert_eq!(label_font_size(31, rect(400.0, 25.0)), Some(16.0));
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
                .find(|&(brick, _)| brick == Brick::Node(active))
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
