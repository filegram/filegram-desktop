//! Canvas-виджет карты диска: отрисовка кирпичей с подписями и вложенными
//! силуэтами (§3.2–3.5 ANALYSIS.md), hit-test и подсветка активного кирпича.
//! Геометрия карты кэшируется в `canvas::Cache` (аналог offscreen Bitmap),
//! подсветка рисуется отдельным слоем поверх.

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
// ARGB #4080CBC4 из оригинала: альфа 0x40 ≈ 0.25.
const NESTED_FILE_FILL: Color = Color::from_rgba8(0x80, 0xCB, 0xC4, 0.25);
const HIGHLIGHT: Color = Color::from_rgba8(0xFF, 0xFF, 0xFF, 0.5);

const CORNER_RADIUS: f32 = 8.0;
const MAX_FONT: f32 = 28.0;
const MIN_FONT: f32 = 12.0;
/// Эмпирическая средняя ширина глифа в долях кегля — для подбора шрифта
/// на кирпич (canvas не даёт дёшево измерить текст).
const CHAR_WIDTH: f32 = 0.6;
/// Минимальная площадь под вложенный контент.
const MIN_CONTENT_SIDE: f32 = 12.0;
/// Отступ вложенного силуэта (слева и снизу).
const SILHOUETTE_MARGIN: f32 = 6.0;

pub struct DiskMap<'a> {
    pub tree: &'a FsTree,
    pub current: NodeId,
    pub active: Option<NodeId>,
    pub cache: &'a canvas::Cache,
}

impl DiskMap<'_> {
    /// Layout первого уровня: дети текущего узла (уже отсортированы
    /// по убыванию размера) в локальных координатах канваса.
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
        // Папка — скруглённый rect, файл — обычный, как в оригинале.
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

    /// Подпись кирпича; размер шрифта подбирается на кирпич, а не глобально
    /// (фикс бага оригинала, §6.5.7). Возвращает использованный кегль.
    fn draw_label(&self, frame: &mut Frame, label: &str, rect: Rectangle) -> f32 {
        let fit = rect.width / (CHAR_WIDTH * label.len() as f32);
        let font_size = fit.clamp(MIN_FONT, MAX_FONT);
        if rect.height < font_size + 8.0 || rect.width < 2.0 * font_size {
            return 0.0;
        }
        // Если и минимальный кегль не влезает — обрезаем текст по ширине.
        let max_chars = (rect.width / (CHAR_WIDTH * font_size)) as usize;
        let content: String = label.chars().take(max_chars).collect();
        frame.fill_text(Text {
            content,
            position: Point::new(rect.x + 4.0, rect.y + 4.0),
            color: Color::WHITE,
            size: Pixels(font_size),
            shaping: iced::widget::text::Shaping::Advanced,
            ..Text::default()
        });
        font_size
    }

    /// Вложенные силуэты второго уровня: цветные прямоугольники без текста.
    fn draw_nested(
        &self,
        frame: &mut Frame,
        children: &[NodeId],
        rect: Rectangle,
        font_size: f32,
    ) {
        // Отступы под заголовок: top += 4 + textSize + 4; left += 1; right −= 8.
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
