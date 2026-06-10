//! Row-based treemap layout (a port of the original's BrickDrawer).
//! A pure function: layout is separated from drawing.

use iced::Rectangle;

/// Row limit divisor for the top level of the chart.
pub const TOP_LEVEL_DIVISOR: f32 = 10.0;
/// Row limit divisor for nested silhouettes.
pub const NESTED_DIVISOR: f32 = 5.0;

/// Normalized weight: areas are proportional to the square root of the size,
/// `max(0.1, …)` guarantees a non-zero area for empty files.
pub fn normalize_weight(size_bytes: u64) -> f32 {
    (size_bytes as f32).sqrt().max(0.1)
}

/// Lays out items with weights `weights` (sorted in descending order)
/// inside rectangle `area`. Rows are stacked bottom-up, bricks within a row go
/// left to right. Returns rectangles in input order.
///
/// `limit_divisor` is [`TOP_LEVEL_DIVISOR`] or [`NESTED_DIVISOR`].
pub fn layout(weights: &[f32], area: Rectangle, limit_divisor: f32) -> Vec<Rectangle> {
    let total: f32 = weights.iter().sum();
    if weights.is_empty() || total <= 0.0 || area.width <= 0.0 || area.height <= 0.0 {
        return vec![Rectangle::new(area.position(), iced::Size::ZERO); weights.len()];
    }

    // Damper based on the file count: with large n the rows get "heavier".
    let file_count_ratio = (weights.len() as f32 / 10.0).powf(0.25).max(1.0);
    let row_limit = total / file_count_ratio / limit_divisor;
    // Pixels² per unit of normalized weight.
    let ratio = area.width * area.height / total;

    let mut rects = Vec::with_capacity(weights.len());
    let mut bottom = area.y + area.height;
    let mut row_start = 0;
    let mut stage_size = 0.0;

    for (i, &weight) in weights.iter().enumerate() {
        stage_size += weight;
        if stage_size < row_limit && i + 1 != weights.len() {
            continue;
        }
        let row_height = stage_size * ratio / area.width;
        let top = (bottom - row_height).max(area.y);
        let mut x = area.x;
        for &w in &weights[row_start..=i] {
            let width = (w * ratio / row_height).min(area.x + area.width - x);
            rects.push(Rectangle::new(
                iced::Point::new(x, top),
                iced::Size::new(width, bottom - top),
            ));
            x += width;
        }
        bottom = top;
        row_start = i + 1;
        stage_size = 0.0;
    }

    rects
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::{Point, Size};

    const EPS: f32 = 1e-3;

    fn area_100x100() -> Rectangle {
        Rectangle::new(Point::new(0.0, 0.0), Size::new(100.0, 100.0))
    }

    fn descending(n: usize) -> Vec<f32> {
        (0..n).map(|i| normalize_weight((1000 * (n - i)) as u64)).collect()
    }

    #[test]
    fn normalize_is_sqrt_with_floor() {
        assert_eq!(normalize_weight(0), 0.1);
        assert_eq!(normalize_weight(100), 10.0);
        assert_eq!(normalize_weight(4096), 64.0);
    }

    #[test]
    fn empty_input_gives_empty_layout() {
        assert!(layout(&[], area_100x100(), TOP_LEVEL_DIVISOR).is_empty());
    }

    #[test]
    fn single_item_fills_whole_area() {
        let rects = layout(&[42.0], area_100x100(), TOP_LEVEL_DIVISOR);
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        assert!((r.x - 0.0).abs() < EPS, "{r:?}");
        assert!((r.y - 0.0).abs() < EPS, "{r:?}");
        assert!((r.width - 100.0).abs() < EPS, "{r:?}");
        assert!((r.height - 100.0).abs() < EPS, "{r:?}");
    }

    #[test]
    fn total_area_is_preserved() {
        let weights = descending(37);
        let rects = layout(&weights, area_100x100(), TOP_LEVEL_DIVISOR);
        let total: f32 = rects.iter().map(|r| r.width * r.height).sum();
        assert!(
            (total - 100.0 * 100.0).abs() / (100.0 * 100.0) < 0.005,
            "total area {total}"
        );
    }

    #[test]
    fn all_rects_inside_area() {
        let weights = descending(37);
        let area = Rectangle::new(Point::new(10.0, 20.0), Size::new(300.0, 200.0));
        for r in layout(&weights, area, TOP_LEVEL_DIVISOR) {
            assert!(r.x >= area.x - EPS && r.y >= area.y - EPS, "{r:?}");
            assert!(r.x + r.width <= area.x + area.width + EPS, "{r:?}");
            assert!(r.y + r.height <= area.y + area.height + EPS, "{r:?}");
        }
    }

    #[test]
    fn rects_do_not_overlap() {
        let weights = descending(25);
        let rects = layout(&weights, area_100x100(), TOP_LEVEL_DIVISOR);
        for (i, a) in rects.iter().enumerate() {
            for b in rects.iter().skip(i + 1) {
                let x_overlap =
                    (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
                let y_overlap =
                    (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
                assert!(
                    x_overlap < EPS || y_overlap < EPS,
                    "overlap {a:?} vs {b:?}"
                );
            }
        }
    }

    #[test]
    fn largest_item_sits_in_bottom_row() {
        let weights = descending(25);
        let rects = layout(&weights, area_100x100(), TOP_LEVEL_DIVISOR);
        let first_bottom = rects[0].y + rects[0].height;
        assert!((first_bottom - 100.0).abs() < EPS, "{:?}", rects[0]);
    }

    #[test]
    fn zero_sized_items_get_nonzero_area() {
        let weights = vec![normalize_weight(1000), normalize_weight(0)];
        let rects = layout(&weights, area_100x100(), TOP_LEVEL_DIVISOR);
        for r in &rects {
            assert!(r.width > 0.0 && r.height > 0.0, "{r:?}");
            assert!(!r.width.is_nan() && !r.height.is_nan(), "{r:?}");
        }
    }
}
