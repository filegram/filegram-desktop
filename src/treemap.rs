//! Strip treemap layout: rows are cut by an aspect-ratio criterion aiming
//! at the golden ratio. A pure function: layout is separated from drawing.

use iced::Rectangle;

/// Normalized weight: areas are proportional to the square root of the size,
/// `max(0.1, …)` guarantees a non-zero area for empty files.
pub fn normalize_weight(size_bytes: u64) -> f32 {
    (size_bytes as f32).sqrt().max(0.1)
}

/// Target brick aspect ratio (width:height) — the golden ratio.
const TARGET_RATIO: f32 = 1.618;

/// Minimum row height: anything thinner cannot fit even the smallest brick
/// caption — the value is coupled to `MIN_FONT + 8` in `diskmap` (12 + 8 px).
/// A last row thinner than this is merged into the previous one.
const MIN_ROW_HEIGHT: f32 = 20.0;

/// How far the brick shape is from the golden ratio (1.0 = ideal).
fn aspect_score(width: f32, height: f32) -> f32 {
    let r = width / height;
    (r / TARGET_RATIO).max(TARGET_RATIO / r)
}

/// The worst shape in a row of total weight `sum`: with the row height fixed,
/// brick width is proportional to weight, so the extremes are the heaviest
/// (`first`) and the lightest (`last`) items — the input is sorted descending.
fn worst_row_score(first: f32, last: f32, sum: f32, area_width: f32, ratio: f32) -> f32 {
    let row_height = sum * ratio / area_width;
    let score = |w: f32| aspect_score(w * ratio / row_height, row_height);
    score(first).max(score(last))
}

/// Lays out items with weights `weights` (sorted in descending order)
/// inside rectangle `area`. Rows are stacked bottom-up, bricks within a row go
/// left to right. Returns rectangles in input order.
///
/// A row keeps accepting items while that improves its worst aspect score
/// (strip treemap aiming at [`TARGET_RATIO`]).
pub fn layout(weights: &[f32], area: Rectangle) -> Vec<Rectangle> {
    let total: f32 = weights.iter().sum();
    if weights.is_empty() || total <= 0.0 || area.width <= 0.0 || area.height <= 0.0 {
        return vec![Rectangle::new(area.position(), iced::Size::ZERO); weights.len()];
    }

    // Pixels² per unit of normalized weight.
    let ratio = area.width * area.height / total;

    // Row boundaries: (index of the first item, total row weight).
    let mut rows: Vec<(usize, f32)> = Vec::new();
    let mut row_start = 0;
    while row_start < weights.len() {
        let first = weights[row_start];
        let mut sum = first;
        let mut row_end = row_start + 1;
        while row_end < weights.len() {
            let next = weights[row_end];
            let current = worst_row_score(first, weights[row_end - 1], sum, area.width, ratio);
            let extended = worst_row_score(first, next, sum + next, area.width, ratio);
            if extended > current {
                break;
            }
            sum += next;
            row_end += 1;
        }
        rows.push((row_start, sum));
        row_start = row_end;
    }

    // A last row thinner than the minimum caption height is unreadable —
    // merge it into the previous row (repeat until the merged row is thick
    // enough).
    while rows.len() > 1 && rows.last().unwrap().1 * ratio / area.width < MIN_ROW_HEIGHT {
        let (_, thin_sum) = rows.pop().unwrap();
        rows.last_mut().unwrap().1 += thin_sum;
    }

    let mut rects = Vec::with_capacity(weights.len());
    let mut bottom = area.y + area.height;
    for (i, &(start, sum)) in rows.iter().enumerate() {
        let end = rows
            .get(i + 1)
            .map_or(weights.len(), |&(next_start, _)| next_start);
        let row_height = sum * ratio / area.width;
        let top = (bottom - row_height).max(area.y);
        let mut x = area.x;
        for &w in &weights[start..end] {
            let width = (w * ratio / row_height).min(area.x + area.width - x);
            rects.push(Rectangle::new(
                iced::Point::new(x, top),
                iced::Size::new(width, bottom - top),
            ));
            x += width;
        }
        bottom = top;
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

    /// Вытянутость кирпича: отношение длинной стороны к короткой.
    fn elongation(r: &Rectangle) -> f32 {
        (r.width / r.height).max(r.height / r.width)
    }

    /// Веса с доминирующим элементом (≈половина суммарного веса):
    /// √размеров файлов 900, 350, 200, … МБ.
    fn dominant() -> Vec<f32> {
        [900, 350, 200, 120, 80, 60, 40, 25, 15, 10, 8, 5, 3, 2, 1]
            .map(|mb: u64| normalize_weight(mb * 1_000_000))
            .to_vec()
    }

    #[test]
    fn equal_items_in_square_form_two_rows_of_two() {
        // Четыре равных веса в квадрате: ряд из одного — лежачий «блин» 4:1,
        // ряд из трёх — стоячие щепки; оптимум критерия — два ряда по два.
        let rects = layout(&[1.0, 1.0, 1.0, 1.0], area_100x100());
        for r in &rects {
            assert!((r.width - 50.0).abs() < EPS, "{r:?}");
            assert!((r.height - 50.0).abs() < EPS, "{r:?}");
        }
    }

    #[test]
    fn dominant_item_does_not_span_full_width() {
        let area = Rectangle::new(Point::ORIGIN, Size::new(320.0, 200.0));
        let rects = layout(&dominant(), area);
        // Гигант делит нижний ряд с соседом, а не занимает всю ширину блином.
        assert!(rects[0].width < 0.7 * area.width, "{:?}", rects[0]);
    }

    #[test]
    fn worst_elongation_is_bounded() {
        let area = Rectangle::new(Point::ORIGIN, Size::new(320.0, 200.0));
        let rects = layout(&dominant(), area);
        let worst = rects.iter().map(elongation).fold(0.0, f32::max);
        // Со старой эвристикой row_limit здесь выходило ≈19:1.
        assert!(worst <= 5.0, "worst elongation {worst}");
    }

    #[test]
    fn thin_last_row_merges_into_previous() {
        // Хвост [0.5, 0.4] лёг бы верхним рядом толщиной ~8 px — нечитаемая
        // полоса сливается с нижним рядом, и все кирпичи занимают полную
        // высоту области.
        let rects = layout(&[10.0, 0.5, 0.4], area_100x100());
        for r in &rects {
            assert!((r.height - 100.0).abs() < EPS, "{r:?}");
        }
    }

    #[test]
    fn normalize_is_sqrt_with_floor() {
        assert_eq!(normalize_weight(0), 0.1);
        assert_eq!(normalize_weight(100), 10.0);
        assert_eq!(normalize_weight(4096), 64.0);
    }

    #[test]
    fn empty_input_gives_empty_layout() {
        assert!(layout(&[], area_100x100()).is_empty());
    }

    #[test]
    fn single_item_fills_whole_area() {
        let rects = layout(&[42.0], area_100x100());
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
        let rects = layout(&weights, area_100x100());
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
        for r in layout(&weights, area) {
            assert!(r.x >= area.x - EPS && r.y >= area.y - EPS, "{r:?}");
            assert!(r.x + r.width <= area.x + area.width + EPS, "{r:?}");
            assert!(r.y + r.height <= area.y + area.height + EPS, "{r:?}");
        }
    }

    #[test]
    fn rects_do_not_overlap() {
        let weights = descending(25);
        let rects = layout(&weights, area_100x100());
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
        let rects = layout(&weights, area_100x100());
        let first_bottom = rects[0].y + rects[0].height;
        assert!((first_bottom - 100.0).abs() < EPS, "{:?}", rects[0]);
    }

    #[test]
    fn zero_sized_items_get_nonzero_area() {
        let weights = vec![normalize_weight(1000), normalize_weight(0)];
        let rects = layout(&weights, area_100x100());
        for r in &rects {
            assert!(r.width > 0.0 && r.height > 0.0, "{r:?}");
            assert!(!r.width.is_nan() && !r.height.is_nan(), "{r:?}");
        }
    }
}
