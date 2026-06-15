//! Disk map canvas widget: drawing bricks with labels and nested
//! silhouettes, hit-testing and active brick highlighting.
//! Map geometry is cached in a `canvas::Cache` (analog of the original's offscreen Bitmap),
//! the highlight is drawn as a separate layer on top.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::time::Instant;

use iced::widget::canvas::{self, Action, Event, Frame, LineCap, LineJoin, Path, Stroke, Text};
use iced::{Color, Pixels, Point, Rectangle, Size, Vector, mouse};

use crate::Message;
use crate::fs_tree::{FsTree, NodeId};
use crate::treemap::{layout, normalize_weight};
use crate::ui::chrome::muted_color;

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
    /// Level-2 silhouettes are background detail: paler than level 1 so
    /// they barely tint the folder silhouette they sit on.
    nested_deep_folder_fill: Color,
    nested_deep_file_fill: Color,
    nested_deep_rest_fill: Color,
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
    nested_folder_fill: Color::from_rgba8(0xFB, 0xC0, 0x2D, 0.35),
    // From the original's ARGB #4080CBC4 (alpha ≈ 0.25), toned down.
    nested_file_fill: Color::from_rgba8(0x80, 0xCB, 0xC4, 0.105),
    nested_rest_fill: Color::from_rgba8(0x9E, 0x9E, 0x9E, 0.105),
    // Barely-there shading that almost blends into the folder silhouette.
    nested_deep_folder_fill: Color::from_rgba8(0x58, 0x2B, 0x04, 0.028),
    nested_deep_file_fill: Color::from_rgba8(0x80, 0xCB, 0xC4, 0.028),
    nested_deep_rest_fill: Color::from_rgba8(0x9E, 0x9E, 0x9E, 0.028),
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
    nested_folder_fill: Color::from_rgba8(0xFF, 0xEC, 0xB3, 0.42),
    nested_file_fill: Color::from_rgba8(0x80, 0xCB, 0xC4, 0.175),
    nested_rest_fill: Color::from_rgba8(0x75, 0x75, 0x75, 0.14),
    nested_deep_folder_fill: Color::from_rgba8(0xBA, 0x75, 0x17, 0.028),
    nested_deep_file_fill: Color::from_rgba8(0x80, 0xCB, 0xC4, 0.035),
    nested_deep_rest_fill: Color::from_rgba8(0x75, 0x75, 0x75, 0.028),
    highlight: Color::from_rgba8(0x00, 0x00, 0x00, 0.25),
};

fn palette(theme: &iced::Theme) -> &'static BrickPalette {
    if theme.extended_palette().is_dark {
        &DARK_PALETTE
    } else {
        &LIGHT_PALETTE
    }
}

/// The color a brick's caption is drawn in (folder vs file). Lets overlays
/// drawn over a brick — e.g. the hover action icons — match the label text.
pub fn brick_text_color(theme: &iced::Theme, is_dir: bool) -> Color {
    let palette = palette(theme);
    if is_dir {
        palette.folder_text
    } else {
        palette.file_text
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
/// How deep the nested preview goes: a folder brick shows its children and,
/// inside child-folder silhouettes, grandchildren.
const NESTED_DEPTH: u8 = 2;
/// Silhouette corner radius as a fraction of its shorter side, capped at
/// [`CORNER_RADIUS`]: rounding stays proportional like the original — tiny
/// tiles stay sharp, large ones round off.
const NESTED_CORNER_FRACTION: f32 = 0.15;

/// Faint file-type watermark, centered inside a file brick. It fills the brick
/// almost edge to edge: a square sized to the *shorter* side (so it stays
/// proportional in tall or wide bricks alike) inset by [`FILE_ICON_MARGIN`].
const FILE_ICON_MARGIN: f32 = 6.0;
/// Below this side (px) the glyph reads as clutter rather than a hint, so a
/// small brick draws no icon at all.
const FILE_ICON_MIN_SIDE: f32 = 18.0;
/// Opacity of the watermark over the brick fill — barely there, so the
/// centered name still reads on top of it.
const FILE_ICON_ALPHA: f32 = 0.12;
/// The glyphs are authored in a 24×24 box (like the bundled SVG icons) and
/// scaled to the brick; the stroke width is in that same space.
const ICON_VIEWBOX: f32 = 24.0;
const ICON_STROKE: f32 = 1.8;

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

/// "N word" with a naive English plural (an "s" suffix unless N == 1).
fn plural(n: u64, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// The rest brick caption: combined size and the collapsed entry counts.
fn rest_label(files: usize, dirs: usize, size: u64) -> String {
    let mut parts = Vec::new();
    if files > 0 {
        parts.push(plural(files as u64, "file"));
    }
    if dirs > 0 {
        parts.push(plural(dirs as u64, "folder"));
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

/// The brick caption split into its two parts: the name (drawn in the
/// brick's text color) and the human-readable size (drawn smaller and
/// muted). The folder entry count lives in the status bar, not on the brick.
fn brick_caption(tree: &FsTree, id: NodeId) -> (&str, String) {
    let node = tree.node(id);
    (&node.name, crate::format::human_size(node.size))
}

/// The full caption as one string — for fitting the font and deciding
/// whether the brick is large enough to carry a label at all.
fn brick_label(tree: &FsTree, id: NodeId) -> String {
    let (name, size) = brick_caption(tree, id);
    format!("{name} {size}")
}

/// Whether the brick is large enough to fit its caption. Tiny slivers get
/// no label, and the hover actions panel is suppressed for them too.
pub fn has_label(tree: &FsTree, id: NodeId, rect: Rectangle) -> bool {
    label_font_size(brick_label(tree, id).chars().count(), rect).is_some()
}

/// The content area inside a folder silhouette: inset on the top and the
/// right so the deeper level leaves the parent's fill visible as a frame
/// (the left and bottom gaps come from the children's own margins).
fn silhouette_content(r: Rectangle) -> Rectangle {
    Rectangle {
        x: r.x,
        y: r.y + SILHOUETTE_MARGIN,
        width: (r.width - SILHOUETTE_MARGIN).max(0.0),
        height: (r.height - SILHOUETTE_MARGIN).max(0.0),
    }
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
        state: &MapState,
        frame: &mut Frame,
        palette: &BrickPalette,
        bricks: &[(Brick, Rectangle)],
        draws: &[Option<ZoomDraw>],
        underlay: &[(Brick, Rectangle)],
    ) {
        frame.fill_rectangle(Point::ORIGIN, frame.size(), palette.map_background);
        // A zoom blows bricks far past the screen: an off-screen brick is not
        // worth filling, and — crucially — drawing its caption would rasterize
        // huge glyphs at many sizes into iced's glyph atlas, overflowing it and
        // breaking *other* text (`AtlasFull` is silently ignored).
        let view = Rectangle::new(Point::ORIGIN, frame.size());
        // The zoom-in underlay: the real child level grown into the pivot,
        // drawn beneath the dissolving parent bricks so the commit is seamless.
        for &(brick, rect) in underlay {
            if view.intersects(&rect) {
                self.draw_one(state, frame, palette, brick, rect, None);
            }
        }
        for (i, &(brick, rect)) in bricks.iter().enumerate() {
            if !view.intersects(&rect) {
                continue;
            }
            let zoom = draws.get(i).copied().flatten();
            self.draw_one(state, frame, palette, brick, rect, zoom);
        }
    }

    /// One brick, node or rest tail, with an optional zoom fade/scale.
    fn draw_one(
        &self,
        state: &MapState,
        frame: &mut Frame,
        palette: &BrickPalette,
        brick: Brick,
        rect: Rectangle,
        zoom: Option<ZoomDraw>,
    ) {
        match brick {
            Brick::Node(id) => self.draw_brick(state, frame, palette, id, rect, zoom),
            Brick::Rest { files, dirs, size } => {
                self.draw_rest(frame, palette, (files, dirs, size), rect, zoom);
            }
        }
    }

    /// The aggregate rest brick: neutral gray tones, no nested silhouettes.
    fn draw_rest(
        &self,
        frame: &mut Frame,
        palette: &BrickPalette,
        counts: (usize, usize, u64),
        rect: Rectangle,
        zoom: Option<ZoomDraw>,
    ) {
        let (files, dirs, size) = counts;
        let bare = zoom.map_or(0.0, |z| z.bare);
        let path = Path::rounded_rectangle(rect.position(), rect.size(), CORNER_RADIUS.into());
        frame.fill(&path, fade_color(palette.rest_fill, 1.0 - bare));
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(fade_color(palette.rest_stroke, 1.0 - bare))
                .with_width(1.0),
        );
        let cap = zoom.map(|z| (z.scale, z.caption));
        self.draw_label(frame, &rest_label(files, dirs, size), palette.rest_text, rect, cap);
    }

    fn draw_brick(
        &self,
        state: &MapState,
        frame: &mut Frame,
        palette: &BrickPalette,
        id: NodeId,
        rect: Rectangle,
        zoom: Option<ZoomDraw>,
    ) {
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
        // Bare the fill toward the background as a zoom-in pivot dissolves (and,
        // for zoom-out, while it is still blown up): the child squares (real
        // underlay or vivid silhouettes) read instead of a flat folder color.
        let bare = zoom.map_or(0.0, |z| z.bare);
        let path = Path::rounded_rectangle(rect.position(), rect.size(), CORNER_RADIUS.into());
        frame.fill(&path, fade_color(fill, 1.0 - bare));
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(fade_color(stroke, 1.0 - bare))
                .with_width(1.0),
        );

        // Silhouettes (folders) or a faint file-type watermark (files) draw
        // first, so the caption (size in the corner, name centered) lands on top.
        if node.is_dir {
            let lift = zoom.map_or((0.0, 1.0), |z| (z.vivid, z.silhouette));
            self.draw_nested(state, frame, palette, id, rect, lift);
        } else {
            let alpha = (1.0 - bare) * FILE_ICON_ALPHA;
            draw_file_icon(frame, FileKind::classify(&node.name), rect, text_color, alpha);
        }

        let (name, size) = brick_caption(self.tree, id);
        // Folders carry their file count beside the size when it fits; files
        // have no count to show.
        let files = node.is_dir.then_some(node.files);
        let cap = zoom.map(|z| (z.scale, z.caption));
        self.draw_brick_label(frame, (name, size, files), text_color, rect, cap);
    }

    /// Draws a node brick's caption: the name in `color` at the fitted size in
    /// the geometric center of the brick, and — on bricks at least three
    /// name-font lines tall — the size small and muted in the top-left corner.
    /// For a folder, `files` carries its file count, appended to the size line
    /// (`"1.2 MB · 42 files"`) when the combined text fits the brick width.
    /// The name font is fitted to the full "name size" caption so the
    /// panel-visibility decision in [`has_label`] stays consistent. The caption
    /// draws over the nested silhouettes, which now fill the brick to its top.
    ///
    /// The name is centered by an *estimated* offset rounded to whole pixels,
    /// not by [`Text`]'s own alignment (which lands glyphs on fractional pixels
    /// and shreds the glyph atlas) nor by a real measurement (shaping text to
    /// measure it inside the canvas draw corrupts the renderer). The fit already
    /// guarantees the *full* caption clears the brick width, so the shorter name
    /// alone always fits; the truncation below is only a defensive guard.
    fn draw_brick_label(
        &self,
        frame: &mut Frame,
        caption: (&str, String, Option<u64>),
        color: Color,
        rect: Rectangle,
        zoom: Option<(f32, f32)>,
    ) {
        let (name, size, files) = caption;
        // A zoom blows the caption up (scale) and fades it (alpha): a zoom-in
        // pivot grows its name and dissolves it; a zoom-out brick fades its name
        // back in at slot size. `(1.0, 1.0)` at rest — exactly the static label.
        let (scale, alpha) = zoom.unwrap_or((1.0, 1.0));
        // Skip a near-invisible caption outright: it would still rasterize its
        // (possibly huge) glyphs into the atlas for nothing.
        if alpha <= 0.01 {
            return;
        }
        // Count characters, not bytes: Cyrillic takes 2 bytes per glyph in
        // UTF-8. The font is fitted to the full caption so a brick draws a
        // label exactly when `has_label` (which measures the same string) does.
        let name_chars = name.chars().count();
        let size_chars = size.chars().count();
        let full_chars = name_chars + 1 + size_chars;
        let Some(font_size) = zoom_label_font_size(full_chars.max(1), rect, scale) else {
            return;
        };
        let name_color = fade_color(color, alpha);
        let size_color = fade_color(muted(color), alpha);

        // Size: top-left corner, small and muted — the lesser part of the label.
        // Dropped on short bricks (height below three name-font lines), where it
        // would crowd the centered name.
        if rect.height >= 3.0 * font_size {
            // For a folder, append the file count when the widened line still
            // clears the brick width (4 px pad each side); estimate the width
            // like elsewhere — measuring by shaping inside the draw is unsafe.
            let size_line = match files {
                Some(n) => {
                    let with_files = format!("{size} · {}", plural(n, "file"));
                    let fits = with_files.chars().count() as f32 * CHAR_WIDTH * SIZE_FONT
                        <= rect.width - 8.0;
                    if fits { with_files } else { size }
                }
                None => size,
            };
            frame.fill_text(Text {
                content: size_line,
                position: label_origin(rect),
                color: size_color,
                size: Pixels(SIZE_FONT),
                shaping: iced::widget::text::Shaping::Advanced,
                ..Text::default()
            });
        }

        // Name: centered in the brick. Centering is done by hand — `Text`'s own
        // `Alignment::Center` subtracts the shaped width/2 from the position,
        // landing glyphs on fractional pixels; that splits each letter across
        // subpixel bins and overflows the glyph atlas (text turns to mush, see
        // [`label_origin`] and [`label_font_size`]). Instead we *estimate* the
        // name width ([`CHAR_WIDTH`] per char, like elsewhere — shaping to
        // measure inside the draw corrupts the renderer) and round the
        // top-left origin to whole pixels, keeping `Text`'s default Left/Top.
        let max_name_chars = ((rect.width - 8.0) / (CHAR_WIDTH * font_size)).max(0.0) as usize;
        // Reuse `name_chars` rather than re-scanning the UTF-8 every redraw.
        let shown_chars = name_chars.min(max_name_chars);
        let name_shown: String = name.chars().take(shown_chars).collect();
        let name_w = shown_chars as f32 * CHAR_WIDTH * font_size;
        frame.fill_text(Text {
            content: name_shown,
            position: Point::new(
                (rect.x + (rect.width - name_w) / 2.0).round(),
                (rect.y + (rect.height - font_size) / 2.0).round(),
            ),
            color: name_color,
            size: Pixels(font_size),
            shaping: iced::widget::text::Shaping::Advanced,
            ..Text::default()
        });
    }

    /// Brick label; the font size is fitted per brick rather than globally
    /// (fixes a bug of the original). `zoom` is the caption's `(scale, alpha)`
    /// during a transition, `None` at rest.
    fn draw_label(
        &self,
        frame: &mut Frame,
        label: &str,
        color: Color,
        rect: Rectangle,
        zoom: Option<(f32, f32)>,
    ) {
        let (scale, alpha) = zoom.unwrap_or((1.0, 1.0));
        if alpha <= 0.01 {
            return;
        }
        // Count characters, not bytes: Cyrillic takes 2 bytes per glyph in UTF-8.
        let char_count = label.chars().count().max(1);
        let Some(font_size) = zoom_label_font_size(char_count, rect, scale) else {
            return;
        };
        // If even the minimum size does not fit, truncate the text to the width.
        let max_chars = (rect.width / (CHAR_WIDTH * font_size)) as usize;
        let content: String = label.chars().take(max_chars).collect();
        frame.fill_text(Text {
            content,
            position: label_origin(rect),
            color: fade_color(color, alpha),
            size: Pixels(font_size),
            shaping: iced::widget::text::Shaping::Advanced,
            ..Text::default()
        });
    }

    /// Nested silhouettes: colored rectangles without text, laid out by
    /// [`MapState::nested_silhouettes`] so they preview the post-zoom levels
    /// to [`NESTED_DEPTH`]. Full depth is drawn every frame, including
    /// mid-flight: the silhouettes are pure geometry (no glyph atlas) and
    /// the [`level1`] part is memoized, so they scale with an animating
    /// brick instead of vanishing and snapping back at rest.
    fn draw_nested(
        &self,
        state: &MapState,
        frame: &mut Frame,
        palette: &BrickPalette,
        dir: NodeId,
        rect: Rectangle,
        lift: (f32, f32),
    ) {
        let (vivid, alpha) = lift;
        if alpha <= 0.01 {
            return;
        }
        // The silhouettes fill the brick to its top edge (as they already do at
        // the bottom): the size line and centered name float over them rather
        // than reserving a header band. Horizontal frame only — left += 1,
        // right −= 8.
        let content = Rectangle {
            x: rect.x + 1.0,
            y: rect.y,
            width: rect.width - 1.0 - 8.0,
            height: rect.height,
        };
        // A tiny brick previews nothing: skip the recursion outright. The
        // same guard inside `nested_silhouettes` covers the deeper levels.
        if content.width < MIN_CONTENT_SIDE || content.height < MIN_CONTENT_SIDE {
            return;
        }
        // A zoom blows bricks far past the screen: only the on-screen part of
        // the preview is worth laying out and filling.
        let view = Rectangle::new(Point::ORIGIN, frame.size());
        if !view.intersects(&content) {
            return;
        }
        let mut silhouettes = Vec::new();
        state.nested_silhouettes(self.tree, dir, frame.size(), content, 1, &mut silhouettes);
        for (brick, silhouette, level) in silhouettes {
            if !view.intersects(&silhouette) {
                continue;
            }
            let (folder, file, rest) = if level == 1 {
                (
                    palette.nested_folder_fill,
                    palette.nested_file_fill,
                    palette.nested_rest_fill,
                )
            } else {
                (
                    palette.nested_deep_folder_fill,
                    palette.nested_deep_file_fill,
                    palette.nested_deep_rest_fill,
                )
            };
            let preview = match brick {
                Brick::Rest { .. } => rest,
                Brick::Node(id) if self.tree.node(id).is_dir => folder,
                Brick::Node(_) => file,
            };
            // Vivify the top level toward the real child colors as the level
            // "opens up"; deeper levels stay faint previews.
            let fill = if level == 1 {
                let real = match brick {
                    Brick::Rest { .. } => palette.rest_fill,
                    Brick::Node(id) if self.tree.node(id).is_dir => palette.folder_fill,
                    Brick::Node(_) => palette.file_fill,
                };
                lerp_color(preview, real, vivid)
            } else {
                preview
            };
            // Corners scale with the silhouette like the original.
            let radius =
                (silhouette.width.min(silhouette.height) * NESTED_CORNER_FRACTION).min(CORNER_RADIUS);
            let path =
                Path::rounded_rectangle(silhouette.position(), silhouette.size(), radius.into());
            frame.fill(&path, fade_color(fill, alpha));
        }
    }
}

/// Label font size for the brick width; `None` — the brick is too small for a label.
/// The font size is strictly integral: cosmic-text rasterizes glyphs separately for
/// each f32 size (`CacheKey::font_size_bits`), and fractional sizes — distinct for
/// every brick and every progressive-scan snapshot — overflow iced's glyph
/// atlas (`PrepareError::AtlasFull` is silently ignored, text breaks).
fn label_font_size(char_count: usize, rect: Rectangle) -> Option<f32> {
    zoom_label_font_size(char_count, rect, 1.0)
}

/// [`label_font_size`] with a zoom enlargement: while a brick is blown up
/// (`scale > 1`) the caption may grow past [`MAX_FONT`] up to [`ZOOM_MAX_FONT`],
/// tracking the brick. At `scale == 1` this is exactly [`label_font_size`], so a
/// settling caption never snaps to a different size.
fn zoom_label_font_size(char_count: usize, rect: Rectangle, scale: f32) -> Option<f32> {
    // Both sides bound the font: the width per character count and
    // the height minus the caption's vertical padding (8 px).
    let fit = (rect.width / (CHAR_WIDTH * char_count.max(1) as f32)).min(rect.height - 8.0);
    let cap = (MAX_FONT * scale).clamp(MIN_FONT, ZOOM_MAX_FONT);
    // Down to an even integer: fewer distinct font sizes — a more stable atlas.
    let font_size = (fit.clamp(MIN_FONT, cap) / 2.0).floor() * 2.0;
    (rect.height >= font_size + 8.0 && rect.width >= 2.0 * font_size).then_some(font_size)
}

/// Font size of the size caption — small and fixed, so it reads as the lesser
/// part of the label.
const SIZE_FONT: f32 = 10.0;

/// A muted variant of a label color: same hue, lower opacity, so it reads as
/// less prominent over the brick fill in either theme.
fn muted(color: Color) -> Color {
    Color {
        a: color.a * 0.6,
        ..color
    }
}

/// A brick's caption may grow past [`MAX_FONT`] while a zoom tween enlarges it
/// — the font tracks the brick and shrinks back to [`MAX_FONT`] at rest. Capped
/// so the glyph atlas sees only a handful of extra sizes.
const ZOOM_MAX_FONT: f32 = 48.0;

/// `ω·t` at which a critically damped spring is settled — the zoom's nominal
/// duration in units of `1/STIFFNESS` (matches the `≈10/ω s` in the
/// [`STIFFNESS`] note). Turns elapsed time into a 0→1 progress for the
/// time-keyed parts of a zoom-*in*.
const SETTLE_TURNS: f32 = 10.0;

/// Zoom-*out*: the enlargement (`current/normal` scale) at which a pivot is
/// fully vivid and bared; it folds back to an opaque preview brick as it
/// shrinks to its slot (scale → 1).
const VIVID_FULL_SCALE: f32 = 1.5;

/// Zoom-*in* vivify window (fraction of the duration): the silhouettes hold
/// their faint preview until [`VIVID_DELAY`] and lerp to fully vivid (the real
/// child colors) by [`VIVID_FULL`], so the level "opens up" mid-flight.
const VIVID_DELAY: f32 = 0.25;
const VIVID_FULL: f32 = 0.6;

/// Zoom-*in*: when (fraction of the duration) the stretching brick begins
/// dissolving to transparent — a short opaque "this is the folder" beat while
/// it grows, then a fade over the rest that reveals the child level the commit
/// snaps in underneath.
const DISSOLVE_START: f32 = 2.0 / 3.0;

/// Scales a color's opacity by `alpha` (clamped) — the per-frame fade of a
/// zoom transition. `alpha == 1.0` is a no-op.
fn fade_color(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha.clamp(0.0, 1.0),
        ..color
    }
}

/// A coarse file category, derived from a name's extension, that selects which
/// watermark glyph a file brick draws.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Image,
    Video,
    Audio,
    Document,
    Archive,
    Code,
    Generic,
}

impl FileKind {
    /// Maps a file name to a category by its lowercased extension. Names with
    /// no extension — and any unknown one — fall back to [`FileKind::Generic`],
    /// so every file brick still shows a watermark.
    fn classify(name: &str) -> FileKind {
        // The part after the last dot; a leading-dot name ("/.gitignore") has an
        // empty stem and so classifies as Generic, which is what we want.
        let ext = name
            .rsplit_once('.')
            .map_or(String::new(), |(_, e)| e.to_ascii_lowercase());
        match ext.as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "svg" | "tif" | "tiff" | "heic"
            | "heif" | "ico" | "avif" | "raw" => FileKind::Image,
            "mp4" | "mkv" | "mov" | "avi" | "webm" | "flv" | "wmv" | "m4v" | "mpg" | "mpeg"
            | "3gp" | "m2ts" => FileKind::Video,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "m4a" | "wma" | "opus" | "aiff" | "mid" => {
                FileKind::Audio
            }
            "pdf" | "doc" | "docx" | "txt" | "md" | "rtf" | "odt" | "odp" | "ods" | "pages"
            | "xls" | "xlsx" | "ppt" | "pptx" | "csv" | "tex" | "epub" => FileKind::Document,
            "zip" | "rar" | "7z" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "iso" | "dmg" => {
                FileKind::Archive
            }
            "rs" | "js" | "ts" | "tsx" | "jsx" | "py" | "c" | "cc" | "cpp" | "cxx" | "h" | "hpp"
            | "java" | "go" | "rb" | "php" | "html" | "htm" | "css" | "scss" | "json" | "xml"
            | "yaml" | "yml" | "toml" | "sh" | "bash" | "zsh" | "sql" | "swift" | "kt" | "lua" => {
                FileKind::Code
            }
            _ => FileKind::Generic,
        }
    }
}

/// Draws the faint file-type watermark centered in a file brick: a line glyph
/// (Lucide style, like the bundled SVG icons) authored in a 24×24 box and
/// scaled to [`FILE_ICON_FRACTION`] of the brick's shorter side. Drawn before
/// the caption so the name reads on top; skipped on bricks too small to carry
/// it or when a zoom has nearly faded it out.
fn draw_file_icon(frame: &mut Frame, kind: FileKind, rect: Rectangle, color: Color, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    let side = rect.width.min(rect.height) - 2.0 * FILE_ICON_MARGIN;
    if side < FILE_ICON_MIN_SIDE {
        return;
    }
    let scale = side / ICON_VIEWBOX;
    // Round the box origin to whole pixels so the strokes land cleanly.
    let ox = (rect.x + (rect.width - side) / 2.0).round();
    let oy = (rect.y + (rect.height - side) / 2.0).round();
    let path = icon_path(kind);
    frame.with_save(|frame| {
        frame.translate(Vector::new(ox, oy));
        frame.scale(scale);
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(fade_color(color, alpha))
                .with_width(ICON_STROKE)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
        );
    });
}

/// Connected stroke segment through `pts`, in the 24×24 icon box.
fn polyline(b: &mut canvas::path::Builder, pts: &[(f32, f32)]) {
    let mut pts = pts.iter();
    if let Some(&(x, y)) = pts.next() {
        b.move_to(Point::new(x, y));
        for &(x, y) in pts {
            b.line_to(Point::new(x, y));
        }
    }
}

/// The line-glyph for a category, built in the 24×24 icon box. Stroke-only, so
/// it tints to a single faint color cleanly.
fn icon_path(kind: FileKind) -> Path {
    Path::new(|b| match kind {
        FileKind::Image => {
            // Framed photo: border, sun, and a mountain ridge.
            polyline(b, &[(3.0, 5.0), (3.0, 19.0), (21.0, 19.0), (21.0, 5.0), (3.0, 5.0)]);
            b.circle(Point::new(8.5, 9.0), 1.8);
            polyline(b, &[(3.0, 17.0), (9.0, 11.0), (13.0, 15.0), (16.0, 12.0), (21.0, 17.0)]);
        }
        FileKind::Video => {
            // Screen with a centered play triangle.
            polyline(b, &[(3.0, 5.0), (3.0, 19.0), (21.0, 19.0), (21.0, 5.0), (3.0, 5.0)]);
            polyline(b, &[(10.0, 8.5), (16.0, 12.0), (10.0, 15.5), (10.0, 8.5)]);
        }
        FileKind::Audio => {
            // Musical note: a beamed stem with two note heads.
            polyline(b, &[(9.0, 17.0), (9.0, 5.0), (19.0, 3.0), (19.0, 15.0)]);
            b.circle(Point::new(6.5, 17.0), 2.5);
            b.circle(Point::new(16.5, 15.0), 2.5);
        }
        FileKind::Document => {
            // Page with a folded corner and a few text lines.
            polyline(b, &[(6.0, 3.0), (14.0, 3.0), (20.0, 9.0), (20.0, 21.0), (6.0, 21.0), (6.0, 3.0)]);
            polyline(b, &[(14.0, 3.0), (14.0, 9.0), (20.0, 9.0)]);
            polyline(b, &[(9.0, 13.0), (16.0, 13.0)]);
            polyline(b, &[(9.0, 16.0), (16.0, 16.0)]);
            polyline(b, &[(9.0, 19.0), (13.0, 19.0)]);
        }
        FileKind::Archive => {
            // Isometric box with its top seam and a front edge.
            polyline(b, &[
                (12.0, 2.5), (20.5, 7.25), (20.5, 16.75), (12.0, 21.5),
                (3.5, 16.75), (3.5, 7.25), (12.0, 2.5),
            ]);
            polyline(b, &[(3.5, 7.25), (12.0, 12.0), (20.5, 7.25)]);
            polyline(b, &[(12.0, 12.0), (12.0, 21.5)]);
        }
        FileKind::Code => {
            // Angle brackets around a slash: `< / >`.
            polyline(b, &[(8.5, 8.0), (4.5, 12.0), (8.5, 16.0)]);
            polyline(b, &[(15.5, 8.0), (19.5, 12.0), (15.5, 16.0)]);
            polyline(b, &[(13.5, 6.0), (10.5, 18.0)]);
        }
        FileKind::Generic => {
            // Blank page with a folded corner.
            polyline(b, &[(6.0, 3.0), (14.0, 3.0), (20.0, 9.0), (20.0, 21.0), (6.0, 21.0), (6.0, 3.0)]);
            polyline(b, &[(14.0, 3.0), (14.0, 9.0), (20.0, 9.0)]);
        }
    })
}

/// The tabbed-folder line glyph for the status-bar entry icon, in the 24×24
/// icon box — the directory counterpart to the file-type glyphs above.
fn folder_icon_path() -> Path {
    Path::new(|b| {
        polyline(b, &[
            (3.0, 6.0), (9.0, 6.0), (11.0, 8.0), (21.0, 8.0),
            (21.0, 19.0), (3.0, 19.0), (3.0, 6.0),
        ]);
    })
}

/// A small status-bar glyph: the folder outline for a directory, otherwise the
/// file-type glyph matching the brick watermark. Themed to the muted chrome
/// color so it sits quietly before the entry's name.
struct EntryIcon {
    is_dir: bool,
    kind: FileKind,
}

impl canvas::Program<Message> for EntryIcon {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        // The glyph is square; center it in the (square) bounds and scale the
        // 24-box to fit — the same authoring space as the brick watermark.
        let side = bounds.width.min(bounds.height);
        let scale = side / ICON_VIEWBOX;
        let ox = ((bounds.width - side) / 2.0).round();
        let oy = ((bounds.height - side) / 2.0).round();
        let path = if self.is_dir {
            folder_icon_path()
        } else {
            icon_path(self.kind)
        };
        frame.translate(Vector::new(ox, oy));
        frame.scale(scale);
        frame.stroke(
            &path,
            Stroke::default()
                .with_color(muted_color(theme))
                .with_width(ICON_STROKE)
                .with_line_cap(LineCap::Round)
                .with_line_join(LineJoin::Round),
        );
        vec![frame.into_geometry()]
    }
}

/// A status-bar icon for a tree entry: the folder outline for a directory or
/// the file-type glyph for a file, sized to sit inline before the entry's
/// name. Reuses the brick-watermark glyphs so the two never drift.
pub fn entry_icon<'a>(is_dir: bool, name: &str, size: f32) -> iced::Element<'a, Message> {
    iced::widget::canvas(EntryIcon {
        is_dir,
        kind: FileKind::classify(name),
    })
    .width(size)
    .height(size)
    .into()
}

/// Linear blend between two colors, component-wise; `t` clamped to `0..=1`.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

/// How much a rectangle is enlarged relative to its resting slot, as a linear
/// scale (√ of the area ratio): `1.0` at rest, `> 1` while a zoom blows it up.
fn zoom_scale(rect: Rectangle, normal: Rectangle) -> f32 {
    let area = rect.width.max(0.0) * rect.height.max(0.0);
    let base = normal.width.max(1.0) * normal.height.max(1.0);
    (area / base).max(0.0).sqrt()
}

/// Per-brick fade/scale state of a zoom transition, parallel to the springs.
/// All four alphas are `0..=1`; `scale` is the [`zoom_scale`] of the brick.
#[derive(Clone, Copy, Debug)]
struct ZoomDraw {
    /// Caption font multiplier (and how big the brick reads).
    scale: f32,
    /// How far the folder fill is bared toward the background (`1` — gone).
    bare: f32,
    /// Lerp of the nested silhouettes from faint preview to real child colors.
    vivid: f32,
    /// Overall opacity of the nested silhouettes (`1` — fully shown).
    silhouette: f32,
    /// Opacity of the caption.
    caption: f32,
}

/// Stiffness of the brick spring, s⁻¹: a critically damped spring crosses
/// a full-map distance to within half a pixel in ≈0.35 s. Higher — snappier.
const STIFFNESS: f32 = 25.0;

/// The opening reveal is dramatised: for the first [`WARP_WINDOW`] real
/// seconds of the scan the spring clock runs [`WARP_FACTOR`]× slower, so the
/// bricks drift out of the centre in slow motion before snapping to normal
/// speed. Past the window real time resumes one-to-one.
const WARP_FACTOR: f32 = 5.0;
const WARP_WINDOW: f32 = 3.0;

/// Warped spring-clock seconds for `secs` real seconds since the reveal began:
/// the first [`WARP_WINDOW`] seconds are stretched [`WARP_FACTOR`]×, the rest
/// pass one-to-one. Monotonic and continuous, so the springs' seamless
/// rebasing still holds across the boundary.
fn warp(secs: f32) -> f32 {
    if secs <= WARP_WINDOW {
        secs / WARP_FACTOR
    } else {
        WARP_WINDOW / WARP_FACTOR + (secs - WARP_WINDOW)
    }
}

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
/// previous arena behind — out-of-bounds ids snap instead of panicking.
fn zoom_direction(tree: &FsTree, old: Option<NodeId>, new: NodeId) -> Zoom {
    let Some(old) = old else { return Zoom::Snap };
    if old == new || old.0 >= tree.nodes.len() || new.0 >= tree.nodes.len() {
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
    /// The brick's resting slot in the current level — the size it reads as at
    /// scale 1. Equals `target` except during a zoom-*in*, whose springs head
    /// to a blown-up `target` while `normal` stays the slot the caption and
    /// silhouettes are measured against ([`zoom_scale`]).
    normal: Rectangle,
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
    /// When the opening reveal began (the first layout ever shown), anchoring
    /// the [`warp`] slow-motion window. `None` until then — the spring clock
    /// runs unwarped. Past the window [`warp`] is the identity, so it stays
    /// `Some` without distorting any later animation.
    reveal_started: Option<Instant>,
    animating: bool,
    /// Whether the running animation is a zoom transition (drives the caption
    /// scale/fade and silhouette vivify); a scan snapshot or deletion is not.
    zoom: bool,
    /// While a zoom-*in* is in flight: the child folder the parent level is
    /// growing toward, committed to once the pivot fills the screen. `None`
    /// otherwise. Drives [`DiskMap::update`]'s routing and blocks navigation
    /// mid-transition.
    zooming_into: Option<NodeId>,
    /// The navigated-to node and canvas size of the current layout:
    /// a change of either snaps instead of animating.
    level: Option<NodeId>,
    size: Size,
    /// Memoized [`level1`] layouts of the folder bricks' contents (in map
    /// bounds coordinates): animation repaints every frame, while a
    /// nested layout is a pure function of the tree snapshot and the
    /// canvas size — the cache drops itself when either changes.
    nested: RefCell<NestedCache>,
}

/// See [`MapState::nested`]. The layouts are valid only for the tree
/// snapshot and canvas size they were computed for: node ids do not
/// survive a tree replacement, and rectangles live in the map-bounds
/// space of one canvas size.
#[derive(Default)]
struct NestedCache {
    /// [`FsTree::generation`] of the cached layouts; 0 matches no tree.
    generation: u64,
    canvas: Size,
    layouts: HashMap<NodeId, Vec<(Brick, Rectangle)>>,
}

impl Default for MapState {
    fn default() -> Self {
        Self {
            theme_dark: Cell::new(None),
            springs: Vec::new(),
            started: Instant::now(),
            reveal_started: None,
            animating: false,
            zoom: false,
            zooming_into: None,
            level: None,
            size: Size::ZERO,
            nested: RefCell::new(NestedCache::default()),
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

    /// Warped spring-clock seconds since the springs were last rebased. While
    /// the opening reveal's slow-motion window is open ([`reveal_started`]),
    /// both `started` and `now` are mapped through [`warp`] before
    /// subtracting, so every spring evaluated this frame shares one warped
    /// clock — the reveal and any scan-snapshot slides riding alongside it run
    /// the same [`WARP_FACTOR`]× slower for the first [`WARP_WINDOW`] seconds.
    fn elapsed(&self, now: Instant) -> f32 {
        match self.reveal_started {
            Some(origin) => {
                warp(now.duration_since(origin).as_secs_f32())
                    - warp(self.started.duration_since(origin).as_secs_f32())
            }
            None => now.duration_since(self.started).as_secs_f32(),
        }
    }

    /// Whether the springs describe this level at this canvas size.
    fn covers(&self, level: NodeId, size: Size) -> bool {
        self.level == Some(level) && self.size == size
    }

    /// Second-level silhouette layout: the folder's children are laid out
    /// by the same algorithm as the post-zoom level — [`level1`], which
    /// fills the map bounds (the canvas inset by [`MAP_MARGIN`]) — and
    /// that area is affinely compressed into the brick's content
    /// rectangle. A zoom transition thus morphs the silhouettes into the
    /// very bricks they previewed, keeping the picture proportional
    /// throughout. The expensive [`level1`] part is memoized per folder:
    /// an animation repaints every frame, while the layout is a pure
    /// function of the tree snapshot and the canvas size — a change of
    /// either purges the cache (stale node ids must not outlive their
    /// tree), and only the affine remap runs per frame.
    fn nested(
        &self,
        tree: &FsTree,
        dir: NodeId,
        canvas: Size,
        content: Rectangle,
    ) -> Vec<(Brick, Rectangle)> {
        let bounds = map_bounds(canvas);
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return Vec::new();
        }
        let mut cache = self.nested.borrow_mut();
        if cache.generation != tree.generation || cache.canvas != canvas {
            cache.layouts.clear();
            cache.generation = tree.generation;
            cache.canvas = canvas;
        }
        cache
            .layouts
            .entry(dir)
            .or_insert_with(|| level1(tree, dir, canvas))
            .iter()
            .map(|&(brick, rect)| (brick, map_rect(rect, bounds, content)))
            .collect()
    }

    /// The silhouettes of a folder preview, in paint order (a folder before
    /// its contents): the previewed brick, its rectangle, and the nesting
    /// level (1 — the folder's children, 2 — grandchildren inside a
    /// child-folder silhouette). Every level is the [`MapState::nested`]
    /// layout of its folder compressed into the parent silhouette, so the
    /// whole preview stays proportional to the post-zoom picture. The
    /// recursion stops past [`NESTED_DEPTH`], and a folder whose content
    /// area is below [`MIN_CONTENT_SIDE`] is not descended into (its own
    /// silhouettes still draw — only the level inside it is skipped).
    fn nested_silhouettes(
        &self,
        tree: &FsTree,
        dir: NodeId,
        canvas: Size,
        content: Rectangle,
        level: u8,
        out: &mut Vec<(Brick, Rectangle, u8)>,
    ) {
        if level > NESTED_DEPTH
            || content.width < MIN_CONTENT_SIDE
            || content.height < MIN_CONTENT_SIDE
        {
            return;
        }
        for (brick, r) in self.nested(tree, dir, canvas, content) {
            let silhouette = Rectangle {
                x: r.x + SILHOUETTE_MARGIN,
                y: r.y,
                width: (r.width - SILHOUETTE_MARGIN).max(0.0),
                height: (r.height - SILHOUETTE_MARGIN).max(0.0),
            };
            out.push((brick, silhouette, level));
            if let Brick::Node(id) = brick
                && tree.node(id).is_dir
            {
                self.nested_silhouettes(
                    tree,
                    id,
                    canvas,
                    silhouette_content(silhouette),
                    level + 1,
                    out,
                );
            }
        }
    }

    /// The rectangle pair (`from`, `to`) of a zoom transition: every spring
    /// starts at its target remapped from `from` onto `to`. `None` — the
    /// transition cannot be built (no zoom requested, the clicked brick is
    /// not on screen, the folder being left is collapsed into the rest
    /// tail, or the source frame is degenerate) — the caller snaps instead.
    fn zoom_frames(
        &self,
        _level: NodeId,
        layout: &[(Brick, Rectangle)],
        zoom: Zoom,
        _now: Instant,
    ) -> Option<(Rectangle, Rectangle)> {
        let bounds = map_bounds(self.size);
        let (from, to) = match zoom {
            // Zoom-*in* runs through the dedicated [`MapState::zoom_in`] path
            // (the parent level blows up and commits), not the generic remap.
            Zoom::Snap | Zoom::In => return None,
            // Leaving a folder: the map shrinks into that folder's brick on
            // the parent level.
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
            // The very first layout ever shown (no previous level): the bricks
            // are born as points at the centre of the map and spring out to
            // their slots — an opening reveal. A later snap (resize, navigation
            // to an unrelated level) already has springs and lands instantly.
            let first_ever = self.level.is_none();
            self.level = Some(level);
            self.size = size;
            if let Some((from, to)) = frames {
                // Zoom-out: springs start blown up (the pivot fills the screen)
                // and shrink to their resting slots — `normal == target`.
                self.springs = layout
                    .into_iter()
                    .map(|(brick, target)| BrickSpring {
                        brick,
                        motion: Motion::resting(map_rect(target, from, to)),
                        target,
                        normal: target,
                    })
                    .collect();
                self.started = now;
                self.zoom = true;
                self.zooming_into = None;
                self.animating = self.springs.iter().any(|s| !s.settled(0.0));
                return true;
            }
            if first_ever && !layout.is_empty() {
                // Every brick starts at its full slot size but stacked on top
                // of one another at the map centre, then springs out to its
                // slot. A plain geometry tween (no zoom scale/fade).
                let centre = map_bounds(size).center();
                self.springs = layout
                    .into_iter()
                    .map(|(brick, target)| BrickSpring {
                        brick,
                        motion: Motion::resting(Rectangle {
                            x: centre.x - target.width / 2.0,
                            y: centre.y - target.height / 2.0,
                            width: target.width,
                            height: target.height,
                        }),
                        target,
                        normal: target,
                    })
                    .collect();
                self.started = now;
                // Open the slow-motion window: the reveal (and any scan
                // snapshots arriving in its first seconds) drift out in slow
                // motion.
                self.reveal_started = Some(now);
                self.zoom = false;
                self.zooming_into = None;
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
                    normal: rect,
                })
                .collect();
            self.started = now;
            self.zoom = false;
            self.zooming_into = None;
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
                    BrickSpring { brick, motion, target, normal: target }
                })
                .collect();
            self.started = now;
            // A scan snapshot / deletion is a plain slide, not a zoom.
            self.zoom = false;
            self.zooming_into = None;
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
                self.zoom = false;
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

    /// The child folder a zoom-*in* is currently growing toward, if any.
    fn zooming_into(&self) -> Option<NodeId> {
        self.zooming_into
    }

    /// The memoized [`level1`] of a folder in map-bounds coordinates (an
    /// identity remap through [`MapState::nested`], reusing its per-tree
    /// cache). Used for the zoom-*in* underlay and to warm the destination
    /// layouts before a zoom-*out* lands them all at once.
    fn cached_level1(&self, tree: &FsTree, dir: NodeId, canvas: Size) -> Vec<(Brick, Rectangle)> {
        self.nested(tree, dir, canvas, map_bounds(canvas))
    }

    /// Per-brick [`ZoomDraw`], parallel to [`bricks`]; `None` at rest or for a
    /// plain scan-snapshot slide.
    ///
    /// **Zoom-out** is size-keyed: a pivot stays vivid and bared while it still
    /// fills much of the screen, folding back to an opaque preview brick — its
    /// caption fading in at slot size — as it shrinks (`scale → 1`).
    ///
    /// **Zoom-in** is time-keyed (the spring's ease-out would reach a given
    /// fraction far too early): the silhouettes hold their faint preview through
    /// the first quarter and vivify across the middle; the stretching brick
    /// stays opaque while it grows, then dissolves to transparent over the last
    /// third ([`bare`] and the caption fade together) — handing off to the
    /// child level the commit snaps in underneath.
    fn zoom_draw(&self, now: Instant) -> Vec<Option<ZoomDraw>> {
        if !(self.animating && self.zoom) {
            return vec![None; self.springs.len()];
        }
        let t = self.elapsed(now);
        if self.zooming_into.is_some() {
            let linear = (t * STIFFNESS / SETTLE_TURNS).clamp(0.0, 1.0);
            let dissolve = ((linear - DISSOLVE_START) / (1.0 - DISSOLVE_START)).clamp(0.0, 1.0);
            let vivid = ((linear - VIVID_DELAY) / (VIVID_FULL - VIVID_DELAY)).clamp(0.0, 1.0);
            return self
                .springs
                .iter()
                .map(|s| {
                    Some(ZoomDraw {
                        scale: zoom_scale(s.rect_at(t), s.normal),
                        bare: dissolve,
                        vivid,
                        // Silhouettes fade with the shell — the real child
                        // underlay (see `draw`) takes their place.
                        silhouette: 1.0 - dissolve,
                        caption: 1.0 - dissolve,
                    })
                })
                .collect();
        }
        self.springs
            .iter()
            .map(|s| {
                let scale = zoom_scale(s.rect_at(t), s.normal);
                let v = ((scale - 1.0) / (VIVID_FULL_SCALE - 1.0)).clamp(0.0, 1.0);
                // Zoom-out keeps its silhouettes solid: the bared pivot shows the
                // vivid child squares as it shrinks back into its slot, and the
                // caption fades in at slot size (the mirror of zoom-in).
                Some(ZoomDraw {
                    scale,
                    bare: v,
                    vivid: v,
                    silhouette: 1.0,
                    caption: 1.0 - v,
                })
            })
            .collect()
    }

    /// Drive a zoom-*in* into child folder `target`: animate the *parent*
    /// level (still displayed) so the pivot's slot grows to fill the screen —
    /// the time-reverse of a zoom-out. The pivot brick (and its vivifying
    /// silhouettes) thus grows out of the parent exactly as a zoom-out shrinks
    /// into it. When the springs reach the blown-up layout the transition
    /// commits ([`commit_zoom_in`]) to `target`'s own level; a pivot that is
    /// off-screen or a sliver commits immediately. Returns whether the picture
    /// changed (the geometry cache is stale).
    fn zoom_in(&mut self, tree: &FsTree, parent: NodeId, target: NodeId, size: Size, now: Instant) -> bool {
        let bounds = map_bounds(size);
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return self.commit_zoom_in(tree, target, size, now);
        }
        let p_layout = self.cached_level1(tree, parent, size);
        let f_slot = p_layout
            .iter()
            .find(|&&(brick, _)| brick == Brick::Node(target))
            .map(|&(_, rect)| rect)
            .filter(|r| r.width >= 1.0 && r.height >= 1.0);
        let Some(f_slot) = f_slot else {
            // The folder collapsed into the rest tail or is a sliver — no slot
            // to grow out of, so jump straight to its level.
            return self.commit_zoom_in(tree, target, size, now);
        };
        let fresh = self.zooming_into != Some(target)
            || self.level != Some(parent)
            || self.size != size;
        if fresh {
            // First frame: springs start at rest (the parent map) and head to
            // the blown-up layout where the pivot slot fills the screen.
            self.level = Some(parent);
            self.size = size;
            self.zooming_into = Some(target);
            self.zoom = true;
            self.started = now;
            self.springs = p_layout
                .into_iter()
                .map(|(brick, slot)| BrickSpring {
                    brick,
                    motion: Motion::resting(slot),
                    target: map_rect(slot, f_slot, bounds),
                    normal: slot,
                })
                .collect();
            self.animating = self.springs.iter().any(|s| !s.settled(0.0));
            if !self.animating {
                // The slot already fills the screen: nothing to animate.
                return self.commit_zoom_in(tree, target, size, now);
            }
            return true;
        }
        // Continuing: commit once the pivot has filled the screen.
        let t = self.elapsed(now);
        if self.springs.iter().all(|s| s.settled(t)) {
            return self.commit_zoom_in(tree, target, size, now);
        }
        true
    }

    /// Land a zoom-*in* on `target`'s own level, at rest. The child map has
    /// already grown to fill the screen as the zoom-in underlay, so the
    /// hand-off is seamless.
    fn commit_zoom_in(&mut self, tree: &FsTree, target: NodeId, size: Size, now: Instant) -> bool {
        let layout = self.cached_level1(tree, target, size);
        self.level = Some(target);
        self.size = size;
        self.zooming_into = None;
        self.zoom = false;
        self.started = now;
        self.springs = layout
            .into_iter()
            .map(|(brick, rect)| BrickSpring {
                brick,
                motion: Motion::resting(rect),
                target: rect,
                normal: rect,
            })
            .collect();
        self.animating = false;
        true
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

    /// Reference implementation of [`MapState::nested`], sans memoization:
    /// the [`level1`] layout of the folder compressed from the map bounds
    /// into the brick's content rectangle.
    fn nested_layout(
        tree: &FsTree,
        dir: NodeId,
        canvas: Size,
        content: Rectangle,
    ) -> Vec<(Brick, Rectangle)> {
        let bounds = map_bounds(canvas);
        level1(tree, dir, canvas)
            .into_iter()
            .map(|(brick, rect)| (brick, map_rect(rect, bounds, content)))
            .collect()
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
    fn nested_layout_is_level1_compressed_into_content() {
        // The nested silhouettes are the post-zoom level squeezed into the
        // brick: same bricks in the same order, and each rectangle sits at
        // the same relative position inside `content` as its level1
        // counterpart inside the full map bounds.
        let tree = tree_with_children(&[
            (500_000_000, true),
            (300_000_000, false),
            (120_000_000, true),
            (50_000_000, false),
            (10_000_000, false),
            (2_000_000, false),
            (1_000_000, false),
        ]);
        let canvas = Size::new(800.0, 500.0);
        let content = Rectangle::new(Point::new(100.0, 50.0), Size::new(200.0, 125.0));
        let full = level1(&tree, tree.root, canvas);
        let nested = nested_layout(&tree, tree.root, canvas, content);
        assert_eq!(full.len(), nested.len(), "{nested:?}");
        let bounds = map_bounds(canvas);
        let relative = |v: f32, origin: f32, extent: f32| (v - origin) / extent;
        for (&(brick, r), &(nested_brick, n)) in full.iter().zip(&nested) {
            assert_eq!(brick, nested_brick);
            let pairs = [
                (relative(n.x, content.x, content.width), relative(r.x, bounds.x, bounds.width)),
                (relative(n.y, content.y, content.height), relative(r.y, bounds.y, bounds.height)),
                (n.width / content.width, r.width / bounds.width),
                (n.height / content.height, r.height / bounds.height),
            ];
            for (got, expected) in pairs {
                assert!((got - expected).abs() < 1e-3, "{n:?} vs {r:?}");
            }
        }
    }

    #[test]
    fn nested_layout_collapses_tail_exactly_like_level1() {
        // A decaying tail collapses into a rest brick by level1's rules —
        // the silhouettes must mirror that, not run their own collapse
        // heuristic on the tiny content rectangle.
        let tree = tree_with_children(&[
            (1_000_000, false),
            (10_000, false),
            (1_600, false),
            (324, false),
            (64, false),
            (9, false),
        ]);
        let canvas = Size::new(800.0, 500.0);
        let content = Rectangle::new(Point::new(10.0, 10.0), Size::new(60.0, 40.0));
        let full: Vec<Brick> = level1(&tree, tree.root, canvas)
            .into_iter()
            .map(|(brick, _)| brick)
            .collect();
        let nested: Vec<Brick> = nested_layout(&tree, tree.root, canvas, content)
            .into_iter()
            .map(|(brick, _)| brick)
            .collect();
        assert_eq!(full, nested);
    }

    #[test]
    fn classify_maps_extensions_to_kinds() {
        let cases = [
            ("photo.JPG", FileKind::Image),
            ("clip.mp4", FileKind::Video),
            ("song.flac", FileKind::Audio),
            ("report.PDF", FileKind::Document),
            ("backup.tar.gz", FileKind::Archive),
            ("main.rs", FileKind::Code),
            ("page.ts", FileKind::Code),
            ("README", FileKind::Generic),
            (".gitignore", FileKind::Generic),
            ("data.unknownext", FileKind::Generic),
        ];
        for (name, expected) in cases {
            assert!(
                FileKind::classify(name) == expected,
                "classify({name:?}) mismatched"
            );
        }
    }

    #[test]
    fn nested_layout_of_empty_folder_is_empty() {
        let tree = tree_with_children(&[(0, true)]);
        let folder = tree.node(tree.root).children[0];
        let canvas = Size::new(800.0, 500.0);
        let content = Rectangle::new(Point::new(10.0, 10.0), Size::new(100.0, 60.0));
        assert!(nested_layout(&tree, folder, canvas, content).is_empty());
    }

    /// A tree of root → one folder → `sizes` files; returns the folder id.
    fn tree_with_grandchildren(sizes: &[u64]) -> (FsTree, NodeId) {
        use crate::fs_tree::ScanNode;
        let mut arena = vec![
            ScanNode {
                name: "root".into(),
                path: std::path::Path::new("/root").into(),
                size: 0,
                is_dir: true,
                parent: 0,
            },
            ScanNode {
                name: "dir".into(),
                path: std::path::Path::new("/root/dir").into(),
                size: 0,
                is_dir: true,
                parent: 0,
            },
        ];
        for (i, &size) in sizes.iter().enumerate() {
            arena.push(ScanNode {
                name: format!("f{i}").into(),
                path: std::path::Path::new("/root/dir/f").into(),
                size,
                is_dir: false,
                parent: 1,
            });
        }
        let tree = FsTree::from_arena(&arena);
        let dir = tree.node(tree.root).children[0];
        (tree, dir)
    }

    #[test]
    fn nested_cache_is_dropped_when_the_tree_is_replaced() {
        // A rescan can produce a tree whose top-level layout is identical
        // (so the springs never retarget), yet the folders' contents — and
        // even the node ids — belong to a different snapshot. The cache is
        // keyed to the snapshot identity, never serving rectangles of a
        // tree that is no longer rendered.
        let canvas = Size::new(800.0, 500.0);
        let content = Rectangle::new(Point::new(10.0, 10.0), Size::new(200.0, 125.0));
        let (before, dir) = tree_with_grandchildren(&[500_000_000, 300_000_000]);
        let (after, _) =
            tree_with_grandchildren(&[500_000_000, 300_000_000, 120_000_000, 50_000_000]);
        let state = MapState::default();
        let first = state.nested(&before, dir, canvas, content);
        assert_eq!(first, nested_layout(&before, dir, canvas, content));
        assert_eq!(
            state.nested(&after, dir, canvas, content),
            nested_layout(&after, dir, canvas, content)
        );
        // The replaced snapshot's layouts are purged, not accumulated.
        assert_eq!(state.nested.borrow().layouts.len(), 1);
    }

    #[test]
    fn nested_cache_is_dropped_when_the_canvas_resizes() {
        // Cached rectangles live in the map-bounds space of one canvas
        // size: remapping them from another size would skew proportions.
        let content = Rectangle::new(Point::new(10.0, 10.0), Size::new(200.0, 125.0));
        let (tree, dir) = tree_with_grandchildren(&[500_000_000, 300_000_000, 120_000_000]);
        let state = MapState::default();
        state.nested(&tree, dir, Size::new(800.0, 500.0), content);
        let resized = Size::new(400.0, 600.0);
        assert_eq!(
            state.nested(&tree, dir, resized, content),
            nested_layout(&tree, dir, resized, content)
        );
    }

    /// The silhouette margins, as applied to a [`MapState::nested`] rect.
    fn with_margin(r: Rectangle) -> Rectangle {
        Rectangle {
            x: r.x + SILHOUETTE_MARGIN,
            y: r.y,
            width: (r.width - SILHOUETTE_MARGIN).max(0.0),
            height: (r.height - SILHOUETTE_MARGIN).max(0.0),
        }
    }

    /// The full-depth silhouettes of `nested_tree`'s root preview.
    fn root_silhouettes(
        state: &MapState,
        tree: &FsTree,
        content: Rectangle,
    ) -> Vec<(Brick, Rectangle, u8)> {
        let mut out = Vec::new();
        state.nested_silhouettes(tree, tree.root, CANVAS, content, 1, &mut out);
        out
    }

    #[test]
    fn nested_silhouettes_recurse_into_folder_silhouettes() {
        // Root previews "sub" and "c" at level 1; inside the "sub"
        // silhouette, its own files appear at level 2, laid out as the
        // nested layout of "sub" compressed into that silhouette.
        let tree = nested_tree();
        let state = MapState::default();
        let content = Rectangle::new(Point::new(10.0, 10.0), Size::new(400.0, 250.0));
        let out = root_silhouettes(&state, &tree, content);
        let level1: Vec<Brick> = out
            .iter()
            .filter(|&&(_, _, level)| level == 1)
            .map(|&(brick, _, _)| brick)
            .collect();
        assert_eq!(level1, [Brick::Node(NodeId(1)), Brick::Node(NodeId(4))], "{out:?}");
        let &(_, sub_rect, _) = out
            .iter()
            .find(|&&(brick, _, _)| brick == Brick::Node(NodeId(1)))
            .unwrap();
        let expected: Vec<(Brick, Rectangle, u8)> = state
            .nested(&tree, NodeId(1), CANVAS, silhouette_content(sub_rect))
            .into_iter()
            .map(|(brick, r)| (brick, with_margin(r), 2))
            .collect();
        let level2: Vec<(Brick, Rectangle, u8)> =
            out.into_iter().filter(|&(_, _, level)| level == 2).collect();
        assert_eq!(level2, expected);
        assert_eq!(level2.len(), 2, "{level2:?}");
    }

    #[test]
    fn nested_silhouettes_paint_a_folder_before_its_contents() {
        // Paint order: a level-2 silhouette goes after its level-1 folder,
        // on top of it — never before.
        let tree = nested_tree();
        let state = MapState::default();
        let content = Rectangle::new(Point::new(10.0, 10.0), Size::new(400.0, 250.0));
        let out = root_silhouettes(&state, &tree, content);
        let sub = out
            .iter()
            .position(|&(brick, _, _)| brick == Brick::Node(NodeId(1)))
            .unwrap();
        for (i, &(_, _, level)) in out.iter().enumerate() {
            if level == 2 {
                assert!(i > sub, "{out:?}");
            }
        }
    }

    #[test]
    fn nested_silhouettes_skip_grandchildren_in_tiny_silhouettes() {
        // The content fits level 1, but the "sub" silhouette inside it is
        // narrower than MIN_CONTENT_SIDE — recursion prunes level 2.
        let tree = nested_tree();
        let state = MapState::default();
        let content = Rectangle::new(Point::new(10.0, 10.0), Size::new(20.0, 14.0));
        let out = root_silhouettes(&state, &tree, content);
        assert!(!out.is_empty());
        assert!(out.iter().all(|&(_, _, level)| level == 1), "{out:?}");
    }

    #[test]
    fn nested_cache_memoizes_within_one_snapshot() {
        let canvas = Size::new(800.0, 500.0);
        let content = Rectangle::new(Point::new(10.0, 10.0), Size::new(200.0, 125.0));
        let (tree, dir) = tree_with_grandchildren(&[500_000_000, 300_000_000]);
        let state = MapState::default();
        let first = state.nested(&tree, dir, canvas, content);
        assert!(state.nested.borrow().layouts.contains_key(&dir));
        // A repeated frame of the same snapshot reuses the entry.
        assert_eq!(state.nested(&tree, dir, canvas, content), first);
        assert_eq!(state.nested.borrow().layouts.len(), 1);
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

    /// A settled baseline at `level` on the [`CANVAS`], established *without*
    /// the opening reveal — the springs sit at rest on `layout` and the
    /// slow-motion [`warp`] never anchors. Spring-dynamics tests use this so
    /// their clock stays one-to-one; the reveal itself is covered separately.
    fn seed(state: &mut MapState, level: NodeId, layout: &Layout, now: Instant) {
        state.level = Some(level);
        state.size = CANVAS;
        state.springs = layout
            .iter()
            .map(|&(brick, rect)| BrickSpring {
                brick,
                motion: Motion::resting(rect),
                target: rect,
                normal: rect,
            })
            .collect();
        state.started = now;
        state.animating = false;
    }

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
    fn map_state_reveals_first_layout_from_centre() {
        // The opening reveal: the very first layout is born stacked at full
        // size on the centre of the map and springs out to its slots.
        let (l1, _) = spring_layouts();
        let mut state = MapState::default();
        let now = Instant::now();
        assert!(state.retarget_snap(NodeId(0), CANVAS, l1.clone(), now));
        assert!(state.is_animating());
        // At t0 every brick has its full slot size but is centred at the map
        // centre, stacked on top of the others.
        let centre = map_bounds(CANVAS).center();
        for ((_, r), (_, target)) in state.bricks(now).iter().zip(&l1) {
            assert!((r.width - target.width).abs() < 0.01, "{r:?}");
            assert!((r.height - target.height).abs() < 0.01, "{r:?}");
            assert!((r.center().x - centre.x).abs() < 0.01, "{r:?}");
            assert!((r.center().y - centre.y).abs() < 0.01, "{r:?}");
        }
        // Once settled the bricks reach their slots. The slow-motion window
        // stretches the spring clock, so the reveal takes several real seconds.
        state.retarget_snap(NodeId(0), CANVAS, l1.clone(), now + Duration::from_secs(4));
        assert_eq!(state.bricks(now + Duration::from_secs(4)), l1);
    }

    #[test]
    fn map_state_springs_toward_changed_layout() {
        let (l1, l2) = spring_layouts();
        let mut state = MapState::default();
        let t0 = Instant::now();
        seed(&mut state, NodeId(0), &l1, t0);
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
        seed(&mut state, NodeId(0), &l1, t0);
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
        seed(&mut state, NodeId(0), &l1, t0);
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
        seed(&mut state, NodeId(0), &l1, t0);
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
        seed(&mut state, NodeId(0), &l1, t0);
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
        seed(&mut state, NodeId(0), &l1, t0);
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
        // `parked` is the at-rest drawn layout; retarget to `target` and probe
        // mid-flight.
        seed(&mut state, tree.root, &parked, now);
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
        seed(&mut state, NodeId(0), &l1, now);
        // The same layout again (an ordinary idle redraw): nothing to repaint.
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
        assert_eq!(zoom_direction(&tree, Some(root), NodeId(99)), Zoom::Snap);
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
    fn zoom_in_grows_parent_into_child_then_commits() {
        // Zoom-in is the mirror of zoom-out: the parent level (still shown)
        // blows up so the clicked folder's slot fills the screen, then commits
        // to the child level.
        let tree = nested_tree();
        let mut state = MapState::default();
        let t0 = Instant::now();
        let root_layout = level1(&tree, NodeId(0), CANVAS);
        seed(&mut state, NodeId(0), &root_layout, t0);
        // Click into "sub" (NodeId 1).
        assert!(state.zoom_in(&tree, NodeId(0), NodeId(1), CANVAS, t0));
        assert!(state.is_animating());
        assert_eq!(state.zooming_into(), Some(NodeId(1)));
        // It starts at the resting parent map (scale 1)…
        assert_rects_close(&state.bricks(t0), &root_layout);
        // …and grows so the pivot slot fills the whole map.
        let sub_slot = root_layout
            .iter()
            .find(|&&(b, _)| b == Brick::Node(NodeId(1)))
            .unwrap()
            .1;
        let blown: Layout = root_layout
            .iter()
            .map(|&(b, t)| (b, map_rect(t, sub_slot, canvas_bounds())))
            .collect();
        assert_eq!(blown[0].1, canvas_bounds());
        assert_rects_close(&state.bricks(t0 + Duration::from_secs(1)), &blown);
        // A far-future frame commits to "sub"'s own level, at rest.
        let done = t0 + Duration::from_secs(2);
        assert!(state.zoom_in(&tree, NodeId(0), NodeId(1), CANVAS, done));
        assert_eq!(state.zooming_into(), None);
        assert_eq!(state.level, Some(NodeId(1)));
        assert!(!state.is_animating());
        assert_eq!(state.bricks(done), level1(&tree, NodeId(1), CANVAS));
    }

    #[test]
    fn zoom_in_commits_immediately_on_degenerate_canvas() {
        // No room to grow a pivot out of: jump straight to the child level.
        let tree = nested_tree();
        let mut state = MapState::default();
        let t0 = Instant::now();
        let tiny = Size::new(1.0, 1.0);
        assert!(state.zoom_in(&tree, NodeId(0), NodeId(1), tiny, t0));
        assert!(!state.is_animating());
        assert_eq!(state.zooming_into(), None);
        assert_eq!(state.level, Some(NodeId(1)));
    }

    #[test]
    fn zoom_out_shrinks_level_into_parent_brick() {
        let folder = Brick::Node(NodeId(1));
        let children = halves_layout(Brick::Node(NodeId(2)), Brick::Node(NodeId(3)));
        let parent = halves_layout(folder, Brick::Node(NodeId(4)));
        let mut state = MapState::default();
        let t0 = Instant::now();
        seed(&mut state, NodeId(1), &children, t0);
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
                // A zoom-*in* runs its own path: keep showing the parent level
                // and grow the clicked folder's slot to fill the screen, then
                // commit to the child. `zooming_into` keeps driving it across
                // the frames after the first.
                let entering = state
                    .zooming_into()
                    .or_else(|| (zoom == Zoom::In).then_some(self.current));
                let stale = if let Some(target) = entering {
                    let parent = state.level.unwrap_or(self.current);
                    state.zoom_in(self.tree, parent, target, size, *now)
                } else {
                    let layout = level1(self.tree, self.current, size);
                    // Warm the destination folders' nested layouts on this light
                    // first frame of a zoom-out, so they aren't all computed at
                    // once when the siblings land (that burst froze the hand-off).
                    if zoom == Zoom::Out {
                        for &(brick, _) in &layout {
                            if let Brick::Node(id) = brick
                                && self.tree.node(id).is_dir
                            {
                                let _ = state.cached_level1(self.tree, id, size);
                            }
                        }
                    }
                    state.retarget(self.current, size, layout, zoom, *now)
                };
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
            // Navigation is blocked mid zoom-in: a second transition would
            // fight the one in flight (and its pending commit).
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if state.zooming_into().is_none() =>
            {
                hit(state).map(|id| Action::publish(Message::BrickPressed(id)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Right | mouse::Button::Back,
            )) if state.zooming_into().is_none() => {
                Some(Action::publish(Message::GoUp).and_capture())
            }
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
        let now = Instant::now();
        let bricks = state.bricks(now);
        let draws = state.zoom_draw(now);
        // Zoom-in underlay: the real child level grown into the pivot's current
        // rectangle, so the dissolving parent brick hands off to it seamlessly.
        // Built only once the pivot starts baring (before that it hides it).
        let underlay = state
            .zooming_into()
            .and_then(|target| {
                let i = bricks.iter().position(|&(b, _)| b == Brick::Node(target))?;
                let bare = draws.get(i).copied().flatten().map_or(0.0, |z| z.bare);
                (bare > 0.0).then(|| state.nested(self.tree, target, bounds.size(), bricks[i].1))
            })
            .unwrap_or_default();
        let map = self
            .cache
            .draw(renderer, bounds.size(), |frame| {
                self.draw_map(state, frame, palette, &bricks, &draws, &underlay)
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
