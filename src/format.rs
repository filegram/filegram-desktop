//! Человекочитаемые размеры и сокращение пути (порт StringUtils из оригинала).

/// Размер в бинарных единицах (делитель 1024), одна цифра после запятой.
/// До 1024 байт — целое число с суффиксом `B`.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = UNITS[0];
    for next in &UNITS[1..] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

/// Сокращает путь до `max_chars` символов, последовательно заменяя средние
/// сегменты на `..` (как `hideFolderNameInPath` в оригинале).
/// Последний сегмент не заменяется никогда.
pub fn shorten_path(path: &str, max_chars: usize) -> String {
    let mut segments: Vec<&str> = path.split('/').collect();
    let len = |segments: &[&str]| {
        segments.iter().map(|s| s.len()).sum::<usize>() + segments.len().saturating_sub(1)
    };
    while len(&segments) > max_chars {
        let last = segments.len() - 1;
        // Кандидаты — средние сегменты: не пустой корень и не последний.
        let Some(victim) = segments[..last]
            .iter()
            .position(|s| !s.is_empty() && *s != "..")
        else {
            break;
        };
        segments[victim] = "..";
    }
    segments.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_size_bytes() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
    }

    #[test]
    fn human_size_binary_units() {
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(1024 * 1024), "1.0 MB");
        assert_eq!(human_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(human_size(1024_u64.pow(4)), "1.0 TB");
        assert_eq!(human_size(2_621_440), "2.5 MB");
    }

    #[test]
    fn shorten_path_fits_unchanged() {
        assert_eq!(shorten_path("/a/b/c/d", 20), "/a/b/c/d");
    }

    #[test]
    fn shorten_path_replaces_middle_segments() {
        assert_eq!(
            shorten_path("/home/user/projects/filegram", 20),
            "/../../../filegram"
        );
    }

    #[test]
    fn shorten_path_keeps_last_segment() {
        // Даже если не влезает — последний сегмент не трогаем.
        assert_eq!(shorten_path("/home/user/filegram", 5), "/../../filegram");
    }
}
