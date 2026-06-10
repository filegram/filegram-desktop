# Filegram Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Десктопный анализатор диска по спеке [ANALYSIS.md](../../../ANALYSIS.md): скан каталога → дерево размеров → интерактивная treemap-диаграмма с навигацией.

**Architecture:** Арена `FsTree` (вместо parent-ссылок), параллельный скан через rayon с честным завершением и отменой через `AtomicBool`, построчный treemap-layout как чистая функция (§3.2 спеки), рендер через `iced::widget::canvas` с `canvas::Cache`, hit-test по прямоугольникам layout, навигация по стеку `NodeId`.

**Tech Stack:** Rust edition 2024, iced 0.14 (features: canvas), rayon, open; dev: tempfile.

---

## Структура файлов

| Файл | Ответственность |
|---|---|
| `src/format.rs` | `human_size` (бинарный, 1 знак), `shorten_path` (`/../`-сжатие) |
| `src/treemap.rs` | `normalize_weight`, чистая `fn layout(weights, area, limit_divisor) -> Vec<Rectangle>` |
| `src/fs_tree.rs` | Арена `FsTree`/`FsNode`/`NodeId`, сборка из `TempNode` с post-order агрегацией и сортировкой детей по убыванию |
| `src/scanner.rs` | `TempNode`, параллельный скан (rayon), прогресс-события через `iced::futures` mpsc, отмена |
| `src/diskmap.rs` | `canvas::Program`: отрисовка кирпичей/силуэтов/подписей, hit-test, подсветка |
| `src/main.rs` | `App`, `Message`, update/view/boot, открытие файлов |

## Константы (из §5 спеки)

- Нормализация: `max(0.1, sqrt(size_bytes))`
- Демпфер: `k = max(1, (n/10)^(1/4))`; лимит строки `L = S/k/10` (верхний уровень), `S/k/5` (вложенный)
- Размер директории как записи: 4096; глубина отрисовки: уровень 1 + силуэты уровня 2
- Мин. размер для вложенного контента: 12×12 px; отступ заголовка: top += 4+textSize+4, left += 1, right −= 8; отступ силуэта: 6 px (left, bottom)
- Шрифт 28 px, минимум 12 (на кирпич, не глобально — фикс бага оригинала)
- Цвета: папка `#F9A825`/`#582B04`, файл `#4DB6AC`/`#004D40`, вложенные `#FBC02D` / `#80CBC4` α=0x40, подсветка `#FFFFFF` α=0x80, скругление 8
- Потоки скана: rayon (без потолка 8; ошибки оригинала §6.5 не переносим)

### Task 1: Зависимости

**Files:** Modify: `Cargo.toml`

- [ ] Добавить `iced = { version = "0.14", features = ["canvas"] }`, `rayon = "1"`, `open = "5"`, `[dev-dependencies] tempfile = "3"`. `cargo check` → OK. Commit.

### Task 2: format.rs (TDD)

- [ ] Тесты: `human_size(0)=="0 B"`, `human_size(1023)=="1023 B"`, `human_size(1024)=="1.0 KB"`, `human_size(1536)=="1.5 KB"`, `human_size(1048576)=="1.0 MB"`, ГБ/ТБ; `shorten_path("/a/b/c/d", достаточно)` без изменений, при нехватке заменяет средние сегменты на `..` пока не влезет, корень не трогает.
- [ ] Реализация: делитель 1024, одна цифра после запятой; `shorten_path(path,max_chars)` — цикл замены первого не-`..` сегмента (кроме первого и последнего). Тесты зелёные, commit.

### Task 3: treemap.rs (TDD)

API:
```rust
pub fn normalize_weight(size: u64) -> f32; // max(0.1, sqrt(size))
/// items — веса по убыванию; limit_divisor: 10.0 верх / 5.0 вложенный.
pub fn layout(weights: &[f32], area: Rectangle, limit_divisor: f32) -> Vec<Rectangle>;
```
Алгоритм §3.2: `S=Σw`, `k=max(1,(n/10)^0.25)`, `L=S/k/limit_divisor`, `ratio=w*h/S`; копим строку пока `stage<L`, `row_h=stage*ratio/area.width`, ширина кирпича `w_i*ratio/row_h`, строки снизу вверх, кламп в `area`.

- [ ] Тесты: пустой вход → пусто; один элемент занимает всю область (±1e-3); N элементов: суммарная площадь == площади области (±0.5%), все rect внутри area, попарно не пересекаются (с эпсилоном), первый (крупнейший) элемент в нижней строке (`max y`); нулевые размеры не дают NaN/нулевой площади.
- [ ] Реализация, тесты зелёные, commit.

### Task 4: fs_tree.rs (TDD)

```rust
pub struct NodeId(pub usize);
pub struct FsNode { pub name: Box<str>, pub path: Box<str>, pub size: u64, pub is_dir: bool, pub children: Vec<NodeId> }
pub struct FsTree { pub nodes: Vec<FsNode>, pub root: NodeId }
impl FsTree { pub fn from_temp(root: TempNode) -> FsTree; pub fn node(&self, id: NodeId) -> &FsNode; }
```
`from_temp`: рекурсивная укладка в арену; size папки = 4096 + Σ детей (post-order); children отсортированы по убыванию size.

- [ ] Тесты: дерево из файлов 100/200/300 в папке → size папки 4096+600, children по убыванию; вложенные папки агрегируются к корню; пустая папка size 4096.
- [ ] Реализация, тесты зелёные, commit.

### Task 5: scanner.rs

```rust
pub struct TempNode { pub name: String, pub path: PathBuf, pub size: u64, pub is_dir: bool, pub children: Vec<TempNode> }
pub enum ScanEvent { Progress { current: String, files: u64 }, Finished(FsTree), Canceled }
pub fn start_scan(root: PathBuf, cancel: Arc<AtomicBool>) -> impl Stream<Item = ScanEvent>;
```
- `read_dir` + `file_type()`; симлинки пропускаем; `DT_UNKNOWN`-fallback на `metadata()`; файл — `metadata().len()`, директория — 4096; ошибки чтения → пустая ветка.
- Параллелизм: рекурсия, поддиректории через `rayon::par_iter` (естественное завершение, без эвристик с таймаутами).
- Прогресс: `AtomicU64` счётчик файлов + текущий путь, отправка в unbounded-канал не чаще 100 мс; отмена — проверка `cancel` в каждом каталоге.
- `start_scan` поднимает `std::thread`, возвращает receiver-стрим для `Task::run`.

- [ ] Тесты (tempfile): скан фикстуры (папка с 2 файлами и подпапкой) даёт корректные размеры/структуру; отмена (выставленный заранее `cancel`) даёт `Canceled`.
- [ ] Реализация, тесты зелёные, commit.

### Task 6: diskmap.rs — canvas::Program

```rust
pub struct DiskMap<'a> { pub app: &'a App }
impl canvas::Program<Message> for DiskMap<'_> { type State = (); ... }
```
- `draw`: уровень 1 — `layout` детей текущего узла (divisor 10); кирпичи: скруглённый rect (радиус 8) c цветами по типу; подпись (имя + размер + число детей для папок), размер шрифта на кирпич: оценка ширины `0.6*len*size`, кламп 12..28; силуэты уровня 2 внутри папок (divisor 5, отступы заголовка и 6 px, порог 12×12). Всё в `canvas::Cache`; подсветка `active` — поверх, вне кэша.
- `update`: `CursorMoved` → hit-test (layout пересчитывается от `bounds`, дёшево) → `SetActive`; `ButtonPressed(Left)` → `BrickPressed(id)`; `ButtonPressed(Right|Back)` → `GoBack`.

- [ ] Реализация + `cargo check`; commit. (Юнит-тестов нет — поведение проверяется визуально в Task 8.)

### Task 7: main.rs — App

```rust
struct App { tree: Option<Arc<FsTree>>, current: NodeId, nav_stack: Vec<NodeId>, active: Option<NodeId>,
             scan: ScanState, path_input: String, cache: canvas::Cache, cancel: Arc<AtomicBool> }
enum ScanState { Idle, Running { current: String, files: u64 }, Done }
enum Message { PathChanged(String), StartScan, CancelScan, Scan(ScanEvent),
               BrickPressed(NodeId), SetActive(Option<NodeId>), GoBack }
```
- boot: `path_input` = `$HOME`/`%USERPROFILE%`/`"."`.
- update: `StartScan` → сброс cancel, `Task::run(start_scan(...), Message::Scan)`; `Scan(Finished)` → tree/current/Done, `cache.clear()`; `BrickPressed`: папка с детьми → push в стек + смена current + `cache.clear()`; пустая папка → игнор; файл → `open::that`; `GoBack` → pop (на пустом стеке — ничего); `SetActive` — без очистки кэша.
- view: Idle — поле пути + кнопка «Сканировать»; Running — текущий путь + счётчик + «Отмена»; Done — строка: «← Назад», сокращённый путь (`shorten_path`), «Новый скан»; ниже `canvas(DiskMap)` на всю область.
- [ ] Реализация, `cargo build` без ошибок, commit.

### Task 8: Верификация

- [ ] `cargo test` — все зелёные; `cargo build --release`; запуск `cargo run`, скан реального каталога, проверка: карта рисуется, hover-подсветка, клик в папку углубляется, назад работает, отмена скана работает. Финальный commit.

## Self-review

- Покрытие спеки: §2 скан → Task 5 (+исправления §6.5: rayon-завершение, отмена реально останавливает, DT_UNKNOWN-fallback, без потолка 8); §3.1 трансляция → сортировка/нормализация в fs_tree+treemap+diskmap; §3.2–3.4 → Task 3/6 (шрифт на кирпич — фикс §6.5.7); §3.5 кэш → canvas::Cache; §4 навигация → Task 6/7 (тап=клик мыши, back=кнопка/ПКМ); §4.3 путь → Task 2.
- Иконки SVG (§3.4) — сознательное упрощение: текстовые подписи без иконок (спека §6.3 допускает «просто текстовые глифы»).
- Жесты тача (порог 200 px, click latency) не переносятся — на десктопе семантика клика родная.
