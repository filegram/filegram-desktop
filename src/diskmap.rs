//! Disk map canvas widget: drawing bricks with labels and nested
//! silhouettes, hit-testing and active brick highlighting.
//! Map geometry is cached in a `canvas::Cache` (analog of the original's offscreen Bitmap),
//! the highlight is drawn as a separate layer on top.

use std::cell::Cell;
use std::collections::HashMap;
use std::time::Instant;

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

/// The brick caption: name and size. The folder entry count lives in the
/// status bar, not on the brick.
fn brick_label(tree: &FsTree, id: NodeId) -> String {
    let node = tree.node(id);
    format!("{} {}", node.name, crate::format::human_size(node.size))
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
    /// Mid-flight the bricks sit away from their targets, so the cursor
    /// resolves against what is actually drawn — the spring rectangles;
    /// `level1` is the fallback for events that arrive before the first
    /// `RedrawRequested` syncs the springs to this level and canvas size.
    fn hit_test(&self, state: &MapState, size: Size, point: Point) -> Option<NodeId> {
        let bricks = if state.covers(self.current, size) {
            state.bricks(Instant::now())
        } else {
            level1(self.tree, self.current, size)
        };
        bricks
            .into_iter()
            .find(|(_, rect)| rect.contains(point))
            .and_then(|(brick, _)| match brick {
                Brick::Node(id) => Some(id),
                Brick::Rest { .. } => None,
            })
    }

    fn draw_map(
        &self,
        frame: &mut Frame,
        palette: &BrickPalette,
        bricks: &[(Brick, Rectangle)],
    ) {
        frame.fill_rectangle(Point::ORIGIN, frame.size(), palette.map_background);
        for &(brick, rect) in bricks {
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

/// Stiffness of the brick spring, s⁻¹: a critically damped spring crosses
/// a full-map distance to within half a pixel in ≈0.35 s. Higher — snappier.
const STIFFNESS: f32 = 25.0;

/// A critically damped spring: the value and its velocity `t` seconds after
/// starting at (`pos`, `velocity`) and heading to `target`. Closed form —
/// evaluating at `t1 + t2` equals evaluating at `t1`, rebasing, and
/// evaluating at `t2`, so mid-flight retargets keep velocity seamlessly.
fn spring(pos: f32, velocity: f32, target: f32, t: f32) -> (f32, f32) {
    // x(t) = target + (d + b·t)·e^(−ω·t), where d is the initial
    // displacement and b = v₀ + ω·d; critical damping — no oscillation.
    let d = pos - target;
    let b = velocity + STIFFNESS * d;
    let decay = (-STIFFNESS * t).exp();
    (
        target + (d + b * t) * decay,
        (velocity - b * STIFFNESS * t) * decay,
    )
}

/// How a level change enters the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Zoom {
    /// No parent↔child relation (first layout, rescan, resize): instant.
    Snap,
    /// Into a child folder: the new level grows out of the clicked brick.
    In,
    /// Back to the parent: the map shrinks into the brick of the folder
    /// being left.
    Out,
}

/// The navigation direction between the displayed level and the new one.
/// A rescan replaces the tree wholesale and may leave an id from the
/// previous arena behind — an out-of-bounds id snaps instead of panicking.
fn zoom_direction(tree: &FsTree, old: Option<NodeId>, new: NodeId) -> Zoom {
    let Some(old) = old else { return Zoom::Snap };
    if old == new || old.0 >= tree.nodes.len() {
        return Zoom::Snap;
    }
    if tree.node(old).children.contains(&new) {
        Zoom::In
    } else if tree.node(new).children.contains(&old) {
        Zoom::Out
    } else {
        Zoom::Snap
    }
}

/// Affine remap: where `rect` lands when the `from` frame is mapped onto
/// the `to` frame. The caller guarantees `from` is non-degenerate.
fn map_rect(rect: Rectangle, from: Rectangle, to: Rectangle) -> Rectangle {
    let sx = to.width / from.width;
    let sy = to.height / from.height;
    Rectangle {
        x: to.x + (rect.x - from.x) * sx,
        y: to.y + (rect.y - from.y) * sy,
        width: rect.width * sx,
        height: rect.height * sy,
    }
}

/// The identity of a brick across snapshots: nodes match by id, while the
/// collapsed tail — whose counts and size change every snapshot — is always
/// the single trailing rest brick.
type BrickKey = Option<NodeId>;

fn brick_key(brick: Brick) -> BrickKey {
    match brick {
        Brick::Node(id) => Some(id),
        Brick::Rest { .. } => None,
    }
}

/// The spring is at rest when every scalar is within half a pixel of its
/// target and moves slower than [`REST_VELOCITY`].
const REST_DISTANCE: f32 = 0.5;
/// Velocity below which remaining motion is invisible (≈0.2 px per frame).
const REST_VELOCITY: f32 = 10.0;

/// Spring state of one animated rectangle: the four scalars (x, y, width,
/// height), each with a velocity in px/s.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Motion {
    rect: Rectangle,
    velocity: [f32; 4],
}

impl Motion {
    fn resting(rect: Rectangle) -> Self {
        Self { rect, velocity: [0.0; 4] }
    }
}

/// One brick in flight: where it was (with what velocity) at the last
/// rebase, and where it is heading.
struct BrickSpring {
    brick: Brick,
    /// State at [`MapState::started`].
    motion: Motion,
    target: Rectangle,
}

impl BrickSpring {
    /// Evaluated motion `t` seconds after the rebase.
    fn motion_at(&self, t: f32) -> Motion {
        let m = &self.motion;
        let (x, vx) = spring(m.rect.x, m.velocity[0], self.target.x, t);
        let (y, vy) = spring(m.rect.y, m.velocity[1], self.target.y, t);
        let (w, vw) = spring(m.rect.width, m.velocity[2], self.target.width, t);
        let (h, vh) = spring(m.rect.height, m.velocity[3], self.target.height, t);
        Motion {
            rect: Rectangle {
                x,
                y,
                width: w,
                height: h,
            },
            velocity: [vx, vy, vw, vh],
        }
    }

    /// The rectangle to draw `t` seconds after the rebase: an inherited
    /// shrinking velocity can briefly undershoot a small target — sizes
    /// are clamped so the path never receives a negative extent.
    fn rect_at(&self, t: f32) -> Rectangle {
        let rect = self.motion_at(t).rect;
        Rectangle {
            width: rect.width.max(0.0),
            height: rect.height.max(0.0),
            ..rect
        }
    }

    /// Close enough to the target to stop animating.
    fn settled(&self, t: f32) -> bool {
        let m = self.motion_at(t);
        (m.rect.x - self.target.x).abs() < REST_DISTANCE
            && (m.rect.y - self.target.y).abs() < REST_DISTANCE
            && (m.rect.width - self.target.width).abs() < REST_DISTANCE
            && (m.rect.height - self.target.height).abs() < REST_DISTANCE
            && m.velocity.iter().all(|v| v.abs() < REST_VELOCITY)
    }
}

/// Per-widget canvas state: the brick springs and the dark-mode flag of the
/// last drawn frame (the cache keeps geometry with baked-in colors, so a
/// theme switch must invalidate it).
pub struct MapState {
    theme_dark: Cell<Option<bool>>,
    /// One spring per brick of the latest layout, in draw order.
    springs: Vec<BrickSpring>,
    /// When the spring states were last rebased.
    started: Instant,
    animating: bool,
    /// The navigated-to node and canvas size of the current layout:
    /// a change of either snaps instead of animating.
    level: Option<NodeId>,
    size: Size,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            theme_dark: Cell::new(None),
            springs: Vec::new(),
            started: Instant::now(),
            animating: false,
            level: None,
            size: Size::ZERO,
        }
    }
}

impl MapState {
    /// Whether the given layout is what the springs already head to.
    fn targets_match(&self, layout: &[(Brick, Rectangle)]) -> bool {
        self.springs.len() == layout.len()
            && self
                .springs
                .iter()
                .zip(layout)
                .all(|(s, &(brick, rect))| s.brick == brick && s.target == rect)
    }

    /// Seconds since the springs were last rebased.
    fn elapsed(&self, now: Instant) -> f32 {
        now.duration_since(self.started).as_secs_f32()
    }

    /// Whether the springs describe this level at this canvas size.
    fn covers(&self, level: NodeId, size: Size) -> bool {
        self.level == Some(level) && self.size == size
    }

    /// The rectangle pair (`from`, `to`) of a zoom transition: every spring
    /// starts at its target remapped from `from` onto `to`. `None` — the
    /// transition cannot be built (no zoom requested, the clicked brick is
    /// not on screen, the folder being left is collapsed into the rest
    /// tail, or the source frame is degenerate) — the caller snaps instead.
    fn zoom_frames(
        &self,
        level: NodeId,
        layout: &[(Brick, Rectangle)],
        zoom: Zoom,
        now: Instant,
    ) -> Option<(Rectangle, Rectangle)> {
        let bounds = map_bounds(self.size);
        let (from, to) = match zoom {
            Zoom::Snap => return None,
            // Entering a folder: the new level grows out of the clicked
            // brick, wherever it is drawn right now (possibly mid-flight).
            Zoom::In => {
                let brick = self
                    .springs
                    .iter()
                    .find(|s| s.brick == Brick::Node(level))?
                    .rect_at(self.elapsed(now));
                (bounds, brick)
            }
            // Leaving a folder: the map shrinks back into that folder's
            // brick on the parent level.
            Zoom::Out => {
                let old = self.level?;
                let &(_, target) = layout
                    .iter()
                    .find(|&&(brick, _)| brick == Brick::Node(old))?;
                (target, bounds)
            }
        };
        // The remap divides by `from`: a degenerate source frame (a sliver
        // brick) cannot anchor a transition.
        (from.width >= 1.0 && from.height >= 1.0).then_some((from, to))
    }

    /// Accepts a fresh layout. Scan snapshots and deletions (same level,
    /// same canvas) animate; navigation into a child or back to the parent
    /// zooms; other navigation and resize snap. Returns whether the
    /// on-screen picture changed — i.e. the geometry cache is stale.
    fn retarget(
        &mut self,
        level: NodeId,
        size: Size,
        layout: Vec<(Brick, Rectangle)>,
        zoom: Zoom,
        now: Instant,
    ) -> bool {
        if self.level != Some(level) || self.size != size {
            // A zoom transition only makes sense on the same canvas: a
            // simultaneous resize would anchor it to stale geometry.
            let frames = (self.size == size)
                .then(|| self.zoom_frames(level, &layout, zoom, now))
                .flatten();
            self.level = Some(level);
            self.size = size;
            if let Some((from, to)) = frames {
                self.springs = layout
                    .into_iter()
                    .map(|(brick, target)| BrickSpring {
                        brick,
                        motion: Motion::resting(map_rect(target, from, to)),
                        target,
                    })
                    .collect();
                self.started = now;
                self.animating = self.springs.iter().any(|s| !s.settled(0.0));
                return true;
            }
            let stale = self.animating || !self.targets_match(&layout);
            self.springs = layout
                .into_iter()
                .map(|(brick, rect)| BrickSpring {
                    brick,
                    motion: Motion::resting(rect),
                    target: rect,
                })
                .collect();
            self.started = now;
            self.animating = false;
            return stale;
        }
        if !self.targets_match(&layout) {
            // Springs may still be in flight: rebase each one to its state
            // at this very moment — position *and* velocity carry over, so
            // a retarget never causes a visible kink in the motion.
            let t = self.elapsed(now);
            let previous: HashMap<BrickKey, Motion> = self
                .springs
                .iter()
                .map(|s| (brick_key(s.brick), s.motion_at(t)))
                .collect();
            self.springs = layout
                .into_iter()
                .map(|(brick, target)| {
                    let motion = previous.get(&brick_key(brick)).copied().unwrap_or_else(
                        // A brick the scanner just discovered grows out
                        // of the center of its destination.
                        || Motion::resting(Rectangle::new(target.center(), Size::ZERO)),
                    );
                    BrickSpring { brick, motion, target }
                })
                .collect();
            self.started = now;
            // A change may be caption-only (the rest brick's counts drift
            // while its rectangle stays put): repaint once, but don't run
            // animation frames for springs that are already at rest.
            self.animating = self.springs.iter().any(|s| !s.settled(0.0));
            return true;
        }
        if self.animating {
            let t = self.elapsed(now);
            if self.springs.iter().all(|s| s.settled(t)) {
                for s in &mut self.springs {
                    s.motion = Motion::resting(s.target);
                }
                self.animating = false;
            }
            // The completing frame still repaints: the cache holds the
            // last in-flight picture.
            return true;
        }
        false
    }

    /// The bricks to display at `now`.
    fn bricks(&self, now: Instant) -> Vec<(Brick, Rectangle)> {
        let t = self.elapsed(now);
        self.springs
            .iter()
            .map(|s| {
                (
                    s.brick,
                    if self.animating { s.rect_at(t) } else { s.target },
                )
            })
            .collect()
    }

    fn is_animating(&self) -> bool {
        self.animating
    }
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
    use std::time::Duration;

    use super::*;
    use iced::{Point, Size};

    fn rect(width: f32, height: f32) -> Rectangle {
        Rectangle::new(Point::ORIGIN, Size::new(width, height))
    }

    impl MapState {
        /// Shorthand for tests: a retarget with no zoom transition requested.
        fn retarget_snap(
            &mut self,
            level: NodeId,
            size: Size,
            layout: Vec<(Brick, Rectangle)>,
            now: Instant,
        ) -> bool {
            self.retarget(level, size, layout, Zoom::Snap, now)
        }
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
    fn spring_starts_at_initial_state() {
        assert_eq!(spring(100.0, -30.0, 200.0, 0.0), (100.0, -30.0));
    }

    #[test]
    fn spring_settles_at_target() {
        let (pos, vel) = spring(0.0, 0.0, 100.0, 1.0);
        assert!((pos - 100.0).abs() < 1e-3, "{pos}");
        assert!(vel.abs() < 1e-2, "{vel}");
    }

    #[test]
    fn spring_from_rest_never_overshoots() {
        // Critically damped: heading from 0 to 100 stays within [0, 100]
        // the whole way.
        for i in 0..=100 {
            let t = i as f32 * 0.01;
            let (pos, _) = spring(0.0, 0.0, 100.0, t);
            assert!((0.0..=100.0).contains(&pos), "t={t} pos={pos}");
        }
    }

    #[test]
    fn spring_rebase_is_seamless() {
        // The closed form composes: evaluating 150 ms straight equals
        // evaluating 100 ms, restarting from that state, and evaluating
        // 50 ms more — a mid-flight retarget keeps position and velocity.
        let (p1, v1) = spring(0.0, 0.0, 100.0, 0.1);
        let (direct, direct_vel) = spring(0.0, 0.0, 100.0, 0.15);
        let (rebased, rebased_vel) = spring(p1, v1, 100.0, 0.05);
        assert!((rebased - direct).abs() < 1e-3, "{rebased} vs {direct}");
        assert!(
            (rebased_vel - direct_vel).abs() < 1e-2,
            "{rebased_vel} vs {direct_vel}"
        );
    }

    /// A first-level map layout, as produced by [`level1`].
    type Layout = Vec<(Brick, Rectangle)>;

    /// Two layouts of the same brick: the spring test bed.
    fn spring_layouts() -> (Layout, Layout) {
        let brick = Brick::Node(NodeId(1));
        (
            vec![(brick, rect(100.0, 50.0))],
            vec![(
                brick,
                Rectangle::new(Point::new(40.0, 20.0), Size::new(200.0, 100.0)),
            )],
        )
    }

    const CANVAS: Size = Size::new(800.0, 500.0);

    /// Component-wise comparison with a tolerance: spring evaluation goes
    /// through `exp`, exact f32 equality would be brittle.
    fn assert_rects_close(actual: &[(Brick, Rectangle)], expected: &[(Brick, Rectangle)]) {
        assert_eq!(actual.len(), expected.len(), "{actual:?} vs {expected:?}");
        for ((brick, rect), (expected_brick, expected_rect)) in actual.iter().zip(expected) {
            assert_eq!(brick, expected_brick);
            for (got, want) in [
                (rect.x, expected_rect.x),
                (rect.y, expected_rect.y),
                (rect.width, expected_rect.width),
                (rect.height, expected_rect.height),
            ] {
                assert!((got - want).abs() < 1e-2, "{rect:?} vs {expected_rect:?}");
            }
        }
    }

    #[test]
    fn map_state_snaps_on_first_layout() {
        let (l1, _) = spring_layouts();
        let mut state = MapState::default();
        let now = Instant::now();
        assert!(state.retarget_snap(NodeId(0), CANVAS, l1.clone(), now));
        assert!(!state.is_animating());
        assert_eq!(state.bricks(now), l1);
    }

    #[test]
    fn map_state_springs_toward_changed_layout() {
        let (l1, l2) = spring_layouts();
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1.clone(), t0);
        let t1 = t0 + Duration::from_secs(1);
        assert!(state.retarget_snap(NodeId(0), CANVAS, l2.clone(), t1));
        assert!(state.is_animating());
        // The motion starts where the bricks were…
        assert_rects_close(&state.bricks(t1), &l1);
        // …100 ms in, every scalar follows the closed-form spring…
        let expected = Rectangle {
            x: spring(0.0, 0.0, 40.0, 0.1).0,
            y: spring(0.0, 0.0, 20.0, 0.1).0,
            width: spring(100.0, 0.0, 200.0, 0.1).0,
            height: spring(50.0, 0.0, 100.0, 0.1).0,
        };
        assert_rects_close(
            &state.bricks(t1 + Duration::from_millis(100)),
            &[(l2[0].0, expected)],
        );
        // …and converges on the new layout.
        assert_rects_close(&state.bricks(t1 + Duration::from_secs(1)), &l2);
    }

    #[test]
    fn spring_outlives_a_display_frame() {
        // A 60 Hz frame is ~16.7 ms. The motion must still be in flight a
        // frame after a layout change — otherwise every change becomes an
        // instant jump and the map visibly twitches during a scan.
        let (l1, l2) = spring_layouts();
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1.clone(), t0);
        let t1 = t0 + Duration::from_secs(1);
        state.retarget_snap(NodeId(0), CANVAS, l2.clone(), t1);
        let next_frame = state.bricks(t1 + Duration::from_micros(16_700));
        assert_ne!(next_frame, l1, "the spring has not moved at all");
        assert_ne!(next_frame, l2, "the spring skipped to the end in one frame");
    }

    #[test]
    fn map_state_settles_and_goes_idle() {
        let (l1, l2) = spring_layouts();
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1, t0);
        state.retarget_snap(NodeId(0), CANVAS, l2.clone(), t0 + Duration::from_secs(1));
        // The frame the spring settles on still redraws (the cache holds
        // the last in-flight frame), the next one is clean.
        let done = t0 + Duration::from_secs(2);
        assert!(state.retarget_snap(NodeId(0), CANVAS, l2.clone(), done));
        assert!(!state.is_animating());
        assert_eq!(state.bricks(done), l2);
        assert!(!state.retarget_snap(NodeId(0), CANVAS, l2, done + Duration::from_secs(1)));
    }

    #[test]
    fn map_state_snaps_on_level_change() {
        let (l1, l2) = spring_layouts();
        let mut state = MapState::default();
        let now = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1, now);
        // Navigation: a different node is displayed — no animation.
        assert!(state.retarget_snap(NodeId(7), CANVAS, l2.clone(), now));
        assert!(!state.is_animating());
        assert_eq!(state.bricks(now), l2);
    }

    #[test]
    fn map_state_snaps_on_resize() {
        let (l1, l2) = spring_layouts();
        let mut state = MapState::default();
        let now = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1, now);
        assert!(state.retarget_snap(NodeId(0), Size::new(400.0, 300.0), l2.clone(), now));
        assert!(!state.is_animating());
        assert_eq!(state.bricks(now), l2);
    }

    #[test]
    fn map_state_retargets_mid_flight_without_jump() {
        let (l1, l2) = spring_layouts();
        let l3 = vec![(Brick::Node(NodeId(1)), rect(300.0, 200.0))];
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1.clone(), t0);
        let t1 = t0 + Duration::from_secs(1);
        state.retarget_snap(NodeId(0), CANVAS, l2.clone(), t1);
        // A new snapshot lands mid-flight: the bricks continue from where
        // they are drawn, not from the previous snapshot's layout.
        let mid = t1 + Duration::from_millis(100);
        let displayed = state.bricks(mid);
        assert_ne!(displayed, l1);
        assert_ne!(displayed, l2);
        assert!(state.retarget_snap(NodeId(0), CANVAS, l3.clone(), mid));
        assert_rects_close(&state.bricks(mid), &displayed);
        // …and the springs converge on the newest target.
        assert_rects_close(&state.bricks(mid + Duration::from_secs(1)), &l3);
    }

    #[test]
    fn map_state_grows_new_brick_from_target_center() {
        let first = Brick::Node(NodeId(1));
        let second = Brick::Node(NodeId(2));
        let l1 = vec![(first, rect(100.0, 50.0))];
        let l2 = vec![
            (first, rect(60.0, 50.0)),
            (
                second,
                Rectangle::new(Point::new(10.0, 20.0), Size::new(40.0, 60.0)),
            ),
        ];
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1, t0);
        let t1 = t0 + Duration::from_secs(1);
        state.retarget_snap(NodeId(0), CANVAS, l2.clone(), t1);
        // The just-discovered brick starts as a point at the center of its
        // destination (30, 50) and inflates from there.
        assert_rects_close(
            &state.bricks(t1)[1..],
            &[(second, Rectangle::new(Point::new(30.0, 50.0), Size::ZERO))],
        );
        assert_rects_close(&state.bricks(t1 + Duration::from_secs(1))[1..], &l2[1..]);
    }

    #[test]
    fn map_state_drops_vanished_brick() {
        let kept = Brick::Node(NodeId(1));
        let gone = Brick::Node(NodeId(2));
        let l1 = vec![(kept, rect(50.0, 50.0)), (gone, rect(30.0, 30.0))];
        let l2 = vec![(kept, rect(100.0, 100.0))];
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1, t0);
        let t1 = t0 + Duration::from_secs(1);
        state.retarget_snap(NodeId(0), CANVAS, l2, t1);
        let bricks = state.bricks(t1);
        assert_eq!(bricks.len(), 1, "{bricks:?}");
        assert_eq!(bricks[0].0, kept);
    }

    #[test]
    fn map_state_matches_rest_across_snapshots() {
        // The collapsed tail changes contents every snapshot, but visually
        // it is the same brick — it slides from its old rectangle instead
        // of re-growing; the caption is the target one right away.
        let l1 = vec![(
            Brick::Rest { files: 1, dirs: 0, size: 10 },
            rect(100.0, 50.0),
        )];
        let rest = Brick::Rest { files: 3, dirs: 1, size: 40 };
        let l2 = vec![(
            rest,
            Rectangle::new(Point::new(40.0, 20.0), Size::new(200.0, 100.0)),
        )];
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1, t0);
        let t1 = t0 + Duration::from_secs(1);
        state.retarget_snap(NodeId(0), CANVAS, l2, t1);
        assert_rects_close(&state.bricks(t1), &[(rest, rect(100.0, 50.0))]);
    }

    #[test]
    fn map_state_repaints_metadata_change_without_animating() {
        // Only the rest brick's caption data changes; the geometry is
        // already at rest. The frame repaints (the caption is baked into
        // the cache) but no animation frames are requested.
        let rect_a = rect(100.0, 50.0);
        let l1 = vec![(Brick::Rest { files: 1, dirs: 0, size: 10 }, rect_a)];
        let l2 = vec![(Brick::Rest { files: 2, dirs: 0, size: 20 }, rect_a)];
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1, t0);
        let t1 = t0 + Duration::from_secs(1);
        assert!(state.retarget_snap(NodeId(0), CANVAS, l2.clone(), t1));
        assert!(!state.is_animating());
        assert_eq!(state.bricks(t1), l2);
    }

    #[test]
    fn hit_test_follows_animated_bricks() {
        // Two children: the final layout tiles the whole map, but the
        // previous snapshot parked both bricks as 10×10 tiles in the
        // top-left corner and the springs have only just departed.
        let tree = tree_with_children(&[(100_000_000, false), (90_000_000, false)]);
        let cache = canvas::Cache::new();
        let map = DiskMap {
            tree: &tree,
            current: tree.root,
            active: None,
            cache: &cache,
        };
        let target = level1(&tree, tree.root, CANVAS);
        let parked: Layout = target
            .iter()
            .enumerate()
            .map(|(i, &(brick, _))| {
                (
                    brick,
                    Rectangle::new(Point::new(i as f32 * 10.0, 0.0), Size::new(10.0, 10.0)),
                )
            })
            .collect();
        let mut state = MapState::default();
        let now = Instant::now();
        state.retarget_snap(tree.root, CANVAS, parked.clone(), now);
        state.retarget_snap(tree.root, CANVAS, target.clone(), now);
        assert!(state.is_animating());
        // The probe sits inside the *drawn* rectangle of the first brick
        // (parked at x 0..10); make sure the final layout disagrees there,
        // then the cursor must hit what the user currently sees.
        let probe = Point::new(5.0, 5.0);
        let (drawn, _) = *parked.iter().find(|(_, r)| r.contains(probe)).unwrap();
        let (future, _) = *target.iter().find(|(_, r)| r.contains(probe)).unwrap();
        assert_ne!(drawn, future, "the probe does not separate the layouts");
        assert_eq!(map.hit_test(&state, CANVAS, probe), Some(NodeId(1)));
    }

    #[test]
    fn map_state_idle_frame_is_clean() {
        let (l1, _) = spring_layouts();
        let mut state = MapState::default();
        let now = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, l1.clone(), now);
        // The same layout again (an ordinary redraw): nothing to repaint.
        assert!(!state.retarget_snap(NodeId(0), CANVAS, l1, now + Duration::from_secs(1)));
    }

    /// A two-level tree: root (0) → folder "sub" (1) with files (2, 3),
    /// plus a file (4) directly in the root.
    fn nested_tree() -> FsTree {
        use crate::fs_tree::ScanNode;
        let node = |name: &str, parent: usize, size: u64, is_dir: bool| ScanNode {
            name: name.into(),
            path: std::path::Path::new("/root").into(),
            size,
            is_dir,
            parent,
        };
        FsTree::from_arena(&[
            node("root", 0, 0, true),
            node("sub", 0, 0, true),
            node("a", 1, 700, false),
            node("b", 1, 300, false),
            node("c", 0, 500, false),
        ])
    }

    #[test]
    fn zoom_direction_detects_parent_child_navigation() {
        let tree = nested_tree();
        let (root, sub, file) = (NodeId(0), NodeId(1), NodeId(2));
        assert_eq!(zoom_direction(&tree, Some(root), sub), Zoom::In);
        assert_eq!(zoom_direction(&tree, Some(sub), root), Zoom::Out);
        // First layout, same level, a jump over two levels, and a stale id
        // from a replaced tree all snap.
        assert_eq!(zoom_direction(&tree, None, root), Zoom::Snap);
        assert_eq!(zoom_direction(&tree, Some(root), root), Zoom::Snap);
        assert_eq!(zoom_direction(&tree, Some(file), NodeId(4)), Zoom::Snap);
        assert_eq!(zoom_direction(&tree, Some(NodeId(99)), root), Zoom::Snap);
    }

    #[test]
    fn map_rect_remaps_between_frames() {
        let bounds = rect(800.0, 400.0);
        assert_eq!(map_rect(bounds, bounds, bounds), bounds);
        let target = Rectangle::new(Point::new(400.0, 0.0), Size::new(400.0, 200.0));
        let brick = Rectangle::new(Point::new(100.0, 50.0), Size::new(200.0, 100.0));
        // The right top quarter of the map lands in the right top quarter
        // of the brick.
        assert_eq!(
            map_rect(target, bounds, brick),
            Rectangle::new(Point::new(200.0, 50.0), Size::new(100.0, 50.0))
        );
        // …and the inverse remap restores it.
        assert_eq!(
            map_rect(
                Rectangle::new(Point::new(200.0, 50.0), Size::new(100.0, 50.0)),
                brick,
                bounds
            ),
            target
        );
    }

    /// The map bounds of the test canvas, as drawn (inset by the margin).
    fn canvas_bounds() -> Rectangle {
        map_bounds(CANVAS)
    }

    /// A two-brick layout tiling the map bounds left/right.
    fn halves_layout(left: Brick, right: Brick) -> Layout {
        let b = canvas_bounds();
        let half = Size::new(b.width / 2.0, b.height);
        vec![
            (left, Rectangle::new(b.position(), half)),
            (
                right,
                Rectangle::new(Point::new(b.x + half.width, b.y), half),
            ),
        ]
    }

    #[test]
    fn zoom_in_grows_level_out_of_clicked_brick() {
        let folder = Brick::Node(NodeId(1));
        let source = Rectangle::new(Point::new(40.0, 20.0), Size::new(200.0, 100.0));
        let children = halves_layout(Brick::Node(NodeId(2)), Brick::Node(NodeId(3)));
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, vec![(folder, source)], t0);
        let t1 = t0 + Duration::from_secs(1);
        assert!(state.retarget(NodeId(1), CANVAS, children.clone(), Zoom::In, t1));
        assert!(state.is_animating());
        // The new level starts compressed into the clicked brick…
        let expected: Layout = children
            .iter()
            .map(|&(brick, target)| (brick, map_rect(target, canvas_bounds(), source)))
            .collect();
        assert_rects_close(&state.bricks(t1), &expected);
        // …and the springs expand it to the full map.
        assert_rects_close(&state.bricks(t1 + Duration::from_secs(1)), &children);
    }

    #[test]
    fn zoom_in_starts_from_the_brick_mid_flight_position() {
        // The clicked brick is still flying toward `landing`: the new level
        // grows out of where the brick is drawn, not where it will land.
        let folder = Brick::Node(NodeId(1));
        let parked = rect(100.0, 50.0);
        let landing = Rectangle::new(Point::new(40.0, 20.0), Size::new(200.0, 100.0));
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, vec![(folder, parked)], t0);
        let t1 = t0 + Duration::from_secs(1);
        state.retarget_snap(NodeId(0), CANVAS, vec![(folder, landing)], t1);
        let mid = t1 + Duration::from_millis(100);
        let (_, drawn) = state.bricks(mid)[0];
        let children = halves_layout(Brick::Node(NodeId(2)), Brick::Node(NodeId(3)));
        state.retarget(NodeId(1), CANVAS, children.clone(), Zoom::In, mid);
        let expected: Layout = children
            .iter()
            .map(|&(brick, target)| (brick, map_rect(target, canvas_bounds(), drawn)))
            .collect();
        assert_rects_close(&state.bricks(mid), &expected);
    }

    #[test]
    fn zoom_out_shrinks_level_into_parent_brick() {
        let folder = Brick::Node(NodeId(1));
        let children = halves_layout(Brick::Node(NodeId(2)), Brick::Node(NodeId(3)));
        let parent = halves_layout(folder, Brick::Node(NodeId(4)));
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(1), CANVAS, children, t0);
        let t1 = t0 + Duration::from_secs(1);
        assert!(state.retarget(NodeId(0), CANVAS, parent.clone(), Zoom::Out, t1));
        assert!(state.is_animating());
        // The folder being left starts blown up to the whole map (the
        // inverse of the zoom-in remap) and shrinks into its brick.
        let (_, folder_target) = parent[0];
        let expected: Layout = parent
            .iter()
            .map(|&(brick, target)| (brick, map_rect(target, folder_target, canvas_bounds())))
            .collect();
        assert_eq!(expected[0].1, canvas_bounds());
        assert_rects_close(&state.bricks(t1), &expected);
        assert_rects_close(&state.bricks(t1 + Duration::from_secs(1)), &parent);
    }

    #[test]
    fn zoom_in_snaps_when_clicked_brick_not_on_screen() {
        // The springs hold a different brick (e.g. the folder was inside
        // the collapsed rest tail): no anchor — an instant transition.
        let children = halves_layout(Brick::Node(NodeId(2)), Brick::Node(NodeId(3)));
        let mut state = MapState::default();
        let t0 = Instant::now();
        let elsewhere = vec![(Brick::Node(NodeId(9)), rect(100.0, 50.0))];
        state.retarget_snap(NodeId(0), CANVAS, elsewhere, t0);
        let t1 = t0 + Duration::from_secs(1);
        assert!(state.retarget(NodeId(1), CANVAS, children.clone(), Zoom::In, t1));
        assert!(!state.is_animating());
        assert_eq!(state.bricks(t1), children);
    }

    #[test]
    fn zoom_out_snaps_when_folder_collapsed_into_rest() {
        // The parent level has no brick for the folder being left — it sits
        // inside the rest tail: nothing to shrink into.
        let children = halves_layout(Brick::Node(NodeId(2)), Brick::Node(NodeId(3)));
        let parent = halves_layout(
            Brick::Node(NodeId(4)),
            Brick::Rest { files: 2, dirs: 1, size: 10 },
        );
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(1), CANVAS, children, t0);
        let t1 = t0 + Duration::from_secs(1);
        assert!(state.retarget(NodeId(0), CANVAS, parent.clone(), Zoom::Out, t1));
        assert!(!state.is_animating());
        assert_eq!(state.bricks(t1), parent);
    }

    #[test]
    fn zoom_snaps_when_canvas_size_changed() {
        // Navigation and a resize in the same frame: the source rectangle
        // belongs to the old canvas — anchor nothing, snap.
        let folder = Brick::Node(NodeId(1));
        let children = halves_layout(Brick::Node(NodeId(2)), Brick::Node(NodeId(3)));
        let mut state = MapState::default();
        let t0 = Instant::now();
        state.retarget_snap(NodeId(0), CANVAS, vec![(folder, rect(100.0, 50.0))], t0);
        let resized = Size::new(400.0, 300.0);
        let t1 = t0 + Duration::from_secs(1);
        assert!(state.retarget(NodeId(1), resized, children.clone(), Zoom::In, t1));
        assert!(!state.is_animating());
        assert_eq!(state.bricks(t1), children);
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
    type State = MapState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let hit = |state: &MapState| {
            cursor
                .position_in(bounds)
                .and_then(|p| self.hit_test(state, bounds.size(), p))
        };
        match event {
            // Every frame starts here: feed the fresh layout to the tween.
            // Scan snapshots and deletions slide the bricks into their new
            // rectangles, parent↔child navigation zooms; while the tween
            // runs, the next frame is requested and the cached geometry is
            // repainted.
            Event::Window(iced::window::Event::RedrawRequested(now)) => {
                let size = bounds.size();
                let zoom = zoom_direction(self.tree, state.level, self.current);
                let stale = state.retarget(
                    self.current,
                    size,
                    level1(self.tree, self.current, size),
                    zoom,
                    *now,
                );
                if stale {
                    self.cache.clear();
                }
                state.is_animating().then(Action::request_redraw)
            }
            // A levitating cursor hovers the actions panel stacked above the
            // map: keep the active brick, otherwise the panel would vanish
            // right as the cursor reaches its buttons.
            Event::Mouse(mouse::Event::CursorMoved { .. }) if !cursor.is_levitating() => {
                let hovered = hit(state);
                (hovered != self.active).then(|| Action::publish(Message::SetActive(hovered)))
            }
            Event::Mouse(mouse::Event::CursorLeft) => self
                .active
                .is_some()
                .then(|| Action::publish(Message::SetActive(None))),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                hit(state).map(|id| Action::publish(Message::BrickPressed(id)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right | mouse::Button::Back,
            )) => Some(Action::publish(Message::GoUp).and_capture()),
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
        if state.theme_dark.replace(Some(is_dark)) != Some(is_dark) {
            self.cache.clear();
        }
        // The springs hold the current layout: `update` receives
        // `RedrawRequested` and retargets them before every frame, so
        // re-running `level1` here would only repeat that work.
        let bricks = state.bricks(Instant::now());
        let map = self
            .cache
            .draw(renderer, bounds.size(), |frame| {
                self.draw_map(frame, palette, &bricks)
            });
        let mut layers = vec![map];

        if let Some(active) = self.active
            && let Some(&(_, rect)) = bricks
                .iter()
                .find(|&&(brick, _)| brick == Brick::Node(active))
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
        state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let over_brick = cursor
            .position_in(bounds)
            .and_then(|p| self.hit_test(state, bounds.size(), p))
            .is_some();
        if over_brick {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}
