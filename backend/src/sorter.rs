use regex::Regex;
use std::{cmp::Ordering, path::Path, sync::LazyLock};

static SXXEXX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)S\d{1,2}E(?P<num>\d{1,4})").unwrap());
static CN_EPISODE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"第\s*(?P<num>\d{1,4})\s*[集话話]").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static SEPARATORS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\[\]()（）【】._-]+").unwrap());
static SEASON: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"第[一二三四五六七八九十\d]+季").unwrap());
static PARTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[/\\._\-\s]+").unwrap());

const TECH_TAGS: &[&str] = &[
    "1080p", "720p", "480p", "2160p", "4K", "8K", "WEB-DL", "WEBRip", "BluRay", "BDRip", "HDRip",
    "x264", "x265", "H264", "H265", "HEVC", "AVC", "AAC", "DTS", "DD5.1", "AC3", "CHS", "CHT",
    "GB", "BIG5", "简体", "繁体",
];

pub fn calculate_padding(total: usize, configured: &str) -> usize {
    if configured != "auto" {
        return configured.parse::<usize>().unwrap_or(3).max(1);
    }
    if total < 1000 { 3 } else { 4 }
}

pub fn sanitize_filename_part(value: &str) -> String {
    let mut cleaned = value
        .trim()
        .replace(['\\', '/'], "")
        .replace(':', "：")
        .replace('*', "")
        .replace('?', "？")
        .replace(['"', '<', '>', '|'], "");
    cleaned = SPACES
        .replace_all(&cleaned, " ")
        .trim_matches(&[' ', '.', '_', '-'][..])
        .chars()
        .take(50)
        .collect();
    if cleaned.is_empty() {
        "未命名".into()
    } else {
        cleaned
    }
}

pub fn parse_episode_number(filename: &str, fallback: i64) -> i64 {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or(filename);
    for pattern in [&*SXXEXX, &*CN_EPISODE] {
        if let Some(caps) = pattern.captures(stem)
            && let Ok(value) = caps.name("num").unwrap().as_str().parse()
        {
            return value;
        }
    }
    standalone_number(stem)
        .map(|(_, value)| value)
        .unwrap_or(fallback)
}

pub fn clean_title(filename: &str, collection_name: &str, fallback: &str) -> String {
    let stem = Path::new(filename)
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or(filename);
    let mut title = SXXEXX.replace_all(stem, " ").to_string();
    title = CN_EPISODE.replace_all(&title, " ").to_string();
    for tag in TECH_TAGS {
        let escaped = regex::escape(tag);
        let re = Regex::new(&format!(
            r"(?i)(^|[\s._\-\[\]()]){}($|[\s._\-\[\]()])",
            escaped
        ))
        .unwrap();
        title = re.replace_all(&title, " ").to_string();
    }
    if let Some((range, _)) = standalone_number(&title) {
        title.replace_range(range, " ");
    }
    if !collection_name.is_empty() {
        title = title.replace(collection_name, " ");
        let alias = SEASON.replace_all(collection_name, " ");
        for part in PARTS.split(&format!("{collection_name} {alias}")) {
            if !part.is_empty() {
                title = title.replace(part, " ");
            }
        }
    }
    title = SEPARATORS.replace_all(&title, " ").to_string();
    let cleaned = sanitize_filename_part(&title);
    if cleaned == "未命名" && !fallback.is_empty() {
        sanitize_filename_part(fallback)
    } else {
        cleaned
    }
}

fn standalone_number(value: &str) -> Option<(std::ops::Range<usize>, i64)> {
    let mut start = None;
    for (index, ch) in value
        .char_indices()
        .chain(std::iter::once((value.len(), ' ')))
    {
        if ch.is_ascii_digit() {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take() {
            let digits = &value[begin..index];
            if digits.len() <= 4 {
                return digits.parse().ok().map(|number| (begin..index, number));
            }
        }
    }
    None
}

pub fn generate_filename(index: usize, title: &str, extension: &str, padding: usize) -> String {
    format!(
        "{:0width$}_{}.{}",
        index,
        sanitize_filename_part(title),
        extension.trim_start_matches('.'),
        width = padding
    )
}

pub fn intro_filename(name: &str, extension: &str) -> String {
    format!(
        "000_{}.{}",
        sanitize_filename_part(name),
        extension.trim_start_matches('.')
    )
}

fn natural_chunks(value: &str) -> Vec<(bool, String)> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut digits = None;
    for ch in value.to_lowercase().chars() {
        let is_digit = ch.is_ascii_digit();
        if digits.is_some_and(|d| d != is_digit) {
            chunks.push((digits.unwrap(), std::mem::take(&mut current)));
        }
        digits = Some(is_digit);
        current.push(ch);
    }
    if !current.is_empty() {
        chunks.push((digits.unwrap_or(false), current));
    }
    chunks
}

pub fn compare_names(a: &str, b: &str, strategy: &str) -> Ordering {
    if strategy.eq_ignore_ascii_case("natural") {
        let left = natural_chunks(a);
        let right = natural_chunks(b);
        for (l, r) in left.iter().zip(&right) {
            let ord = match (l.0, r.0) {
                (true, true) => {
                    l.1.parse::<u64>()
                        .unwrap_or(0)
                        .cmp(&r.1.parse::<u64>().unwrap_or(0))
                }
                _ => l.1.cmp(&r.1),
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        return left.len().cmp(&right.len());
    }
    a.to_lowercase().cmp(&b.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_story_player_ordering() {
        assert_eq!(calculate_padding(9, "auto"), 3);
        assert_eq!(calculate_padding(1000, "auto"), 4);
        assert_eq!(parse_episode_number("萌鸡小队.S01E02.找妈妈.mp4", 9), 2);
        assert_eq!(parse_episode_number("第12集.泥坑.mp4", 9), 12);
        assert_eq!(parse_episode_number("故事20240805.mp4", 9), 9);
        assert_eq!(
            clean_title(
                "萌鸡小队.S01E02.找妈妈.1080p.WEB-DL.x264.mp4",
                "萌鸡小队",
                ""
            ),
            "找妈妈"
        );
        assert_eq!(
            generate_filename(2, "找/妈妈?", "mp3", 3),
            "002_找妈妈？.mp3"
        );
    }

    #[test]
    fn natural_sort_differs_from_name_sort() {
        let mut values = vec!["10_故事.mp3", "2_故事.mp3", "001_片头.mp3"];
        values.sort_by(|a, b| compare_names(a, b, "natural"));
        assert_eq!(values, ["001_片头.mp3", "2_故事.mp3", "10_故事.mp3"]);
    }
}
