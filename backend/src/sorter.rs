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

/// 不可见/格式类 Unicode：零宽字符、bidi 控制符、软连字符、行分隔符等。
/// 这些字符合法但看不见，混进文件名既难排查又会让部分程序（如 ffmpeg）
/// 打开文件失败，一律过滤。
fn is_invisible_unicode(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FEFF}'
    )
}

/// 文件名白名单：ASCII、CJK（中日韩统一表意/假名/谚文）、CJK 全角标点与
/// 全角形式（？％等）。其余字符（Latin-1 扩展、希伯来文/阿拉伯文、U+FFFD
/// 替换符等）几乎都是 GBK 文件名错误转码的乱码产物，Windows 的 ANSI 代码页
/// 无法表示它们，会被 ffmpeg 命令行转码成 `?` 导致打不开输出文件。
fn is_safe_filename_char(c: char) -> bool {
    c.is_ascii()
        || matches!(
            c,
            '\u{00B7}' // 间隔号 ·
                | '\u{2013}'..='\u{2026}' // 常用中文标点：– — ‘ ’ “ ” …
                | '\u{3000}'..='\u{303F}' // CJK 符号和标点（全角空格、。、《》等）
                | '\u{3040}'..='\u{30FF}' // 平假名、片假名
                | '\u{3400}'..='\u{4DBF}' // CJK 扩展 A
                | '\u{4E00}'..='\u{9FFF}' // CJK 统一表意文字
                | '\u{AC00}'..='\u{D7AF}' // 谚文
                | '\u{F900}'..='\u{FAFF}' // CJK 兼容表意
                | '\u{FF00}'..='\u{FFEF}' // 全角形式（！？％等）
                | '\u{20000}'..='\u{2FA1F}' // CJK 扩展 B 及以上
        )
}

pub fn sanitize_filename_part(value: &str) -> String {
    let mut cleaned = value
        .trim()
        .replace(['\\', '/'], "")
        .replace(':', "：")
        .replace('*', "")
        .replace('?', "？")
        .replace('%', "％") // ffmpeg 把输出文件名里的 % 当序列号格式解析，非法 %X 会报 Invalid argument
        .replace(['"', '<', '>', '|'], "")
        .chars()
        // 白名单过滤：只保留 ASCII、CJK/全角字符、常见标点。文件名可能来自
        // NAS/SMB/旧设备，是 GBK 字节被错误转码的产物（如 "蓝猫淘气" 的 GBK
        // 字节按 UTF-8 解码成 "��è����"），混入 Latin-1（å è）、希伯来文
        // 音符（ֵ）、替换符 U+FFFD 等 GBK 代码页无法表示的字符——Windows 上
        // ffmpeg（ANSI main）把命令行转系统代码页时这些字符变 `?`，打开输出
        // 文件直接报 Invalid argument。
        .filter(|c| !c.is_control() && !is_invisible_unicode(*c) && is_safe_filename_char(*c))
        .collect::<String>();
    cleaned = SPACES
        .replace_all(&cleaned, " ")
        .trim_matches(&[' ', '.', '_', '-'][..])
        .chars()
        .take(50)
        .collect::<String>();
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
    fn strips_control_and_invisible_characters() {
        // 换行/制表符/零宽字符/软连字符都会让 Windows 文件名非法或不可见，
        // ffmpeg 打开输出文件时报 Invalid argument。
        assert_eq!(
            generate_filename(1, "小猪佩奇\n第1集\t（\u{200B}泥坑\u{AD}）", "mp3", 3),
            "001_小猪佩奇第1集（泥坑）.mp3"
        );
        assert_eq!(sanitize_filename_part("a\r\nb"), "ab");
        // U+FFFD（lossy 转换产物）在 Windows 代码页（GBK）里无法表示，
        // ffmpeg 的命令行转码会把它变成 `?` 导致输出文件打不开。
        assert_eq!(sanitize_filename_part("蓝猫\u{FFFD}淘气"), "蓝猫淘气");
        // GBK 文件名被错误转码的乱码（Latin-1/希伯来文等）也一并过滤，
        // 与用户实测的 "蓝猫淘气&??!! MAX 1440x1080" 场景一致。
        assert_eq!(
            generate_filename(
                19,
                "蓝猫淘气&\u{FFFD}?\u{05B5}\u{00E5}!! MAX 1440x1080",
                "mp3",
                3
            ),
            "019_蓝猫淘气&？!! MAX 1440x1080.mp3"
        );
        // 合法的全角标点与英文保留。
        assert_eq!(
            sanitize_filename_part("皮皮鲁（3D版）·鲁西西！"),
            "皮皮鲁（3D版）·鲁西西！"
        );
    }

    #[test]
    fn natural_sort_differs_from_name_sort() {
        let mut values = vec!["10_故事.mp3", "2_故事.mp3", "001_片头.mp3"];
        values.sort_by(|a, b| compare_names(a, b, "natural"));
        assert_eq!(values, ["001_片头.mp3", "2_故事.mp3", "10_故事.mp3"]);
    }
}
