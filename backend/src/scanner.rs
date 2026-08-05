use crate::{
    media::probe_video,
    models::{AppSettings, Collection, VideoFile},
    sorter::{clean_title, compare_names, parse_episode_number},
};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;

pub fn scan_paths(paths: &[String], settings: &AppSettings) -> (Vec<Collection>, Vec<String>) {
    let allowed: HashSet<String> = settings
        .video_extensions
        .iter()
        .map(|v| v.to_lowercase())
        .collect();
    let ignored: HashSet<String> = settings
        .ignored_extensions
        .iter()
        .map(|v| v.to_lowercase())
        .collect();
    let min_size = (settings.min_file_size_mb.max(0.0) * 1024.0 * 1024.0) as u64;
    let mut warnings = Vec::new();
    let mut groups: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    for source in paths {
        let root = expand_home(source);
        if !root.exists() {
            warnings.push(format!("目录不存在: {}", root.display()));
            continue;
        }
        let candidates: Box<dyn Iterator<Item = PathBuf>> = if root.is_file() {
            Box::new(std::iter::once(root))
        } else {
            Box::new(
                WalkDir::new(root)
                    .into_iter()
                    .filter_map(Result::ok)
                    .map(|e| e.into_path()),
            )
        };
        for path in candidates.filter(|p| p.is_file()) {
            let suffix = path
                .extension()
                .and_then(|v| v.to_str())
                .map(|v| format!(".{}", v.to_lowercase()))
                .unwrap_or_default();
            if ignored.contains(&suffix) {
                warnings.push(format!("已过滤后缀: {}", path.display()));
                continue;
            }
            if !allowed.contains(&suffix) {
                continue;
            }
            let size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if size < min_size {
                warnings.push(format!("已过滤小文件: {}", path.display()));
                continue;
            }
            groups
                .entry(path.parent().unwrap_or(Path::new(".")).to_path_buf())
                .or_default()
                .push(path);
        }
    }
    let mut grouped: Vec<_> = groups.into_iter().collect();
    grouped.sort_by(|a, b| a.0.cmp(&b.0));
    let mut collections = Vec::new();
    for (folder, mut files) in grouped {
        let collection_id = Uuid::new_v4().to_string();
        let name = collection_name(&folder);
        files.sort_by(|a, b| {
            let an = a.file_name().and_then(|v| v.to_str()).unwrap_or_default();
            let bn = b.file_name().and_then(|v| v.to_str()).unwrap_or_default();
            parse_episode_number(an, i64::MAX)
                .cmp(&parse_episode_number(bn, i64::MAX))
                .then_with(|| compare_names(an, bn, &settings.filesystem_sorting))
        });
        let mut videos = Vec::new();
        for (position, path) in files.into_iter().enumerate() {
            let filename = path
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
                .to_string();
            let episode = parse_episode_number(&filename, position as i64 + 1);
            let video_id = Uuid::new_v4().to_string();
            let (duration, codec, resolution, mut tracks) = match probe_video(&path) {
                Ok(result) => result,
                Err(error) => {
                    warnings.push(format!("无法解析 {}: {error}", path.display()));
                    (None, String::new(), String::new(), vec![])
                }
            };
            for track in &mut tracks {
                track.id = Some(Uuid::new_v4().to_string());
                track.video_file_id = Some(video_id.clone());
            }
            videos.push(VideoFile {
                id: video_id,
                collection_id: Some(collection_id.clone()),
                filename: filename.clone(),
                filepath: absolute_path(&path),
                file_size: fs::metadata(&path).map(|m| m.len() as i64).unwrap_or(0),
                duration,
                video_codec: codec,
                resolution,
                audio_tracks: tracks,
                episode_number: episode,
                episode_title: clean_title(&filename, &name, &format!("第{episode:02}集")),
                status: "pending".into(),
            });
        }
        collections.push(Collection {
            id: collection_id,
            name,
            source_path: absolute_path(&folder),
            episode_count: videos.len() as i64,
            status: "scanned".into(),
            video_files: videos,
            ..Default::default()
        });
    }
    (collections, warnings)
}

fn collection_name(folder: &Path) -> String {
    let current = folder
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or_default();
    if ["第一季", "第二季", "第三季", "第四季", "第五季"].contains(&current)
        && let Some(parent) = folder
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|v| v.to_str())
    {
        return format!("{parent}{current}");
    }
    current.into()
}

fn absolute_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn expand_home(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scanner_groups_and_filters() {
        let root = std::env::temp_dir().join(format!("vid2audio-test-{}", Uuid::new_v4()));
        let folder = root.join("萌鸡小队第一季");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("萌鸡小队.S01E01.植树节.1080p.mp4"), b"video").unwrap();
        fs::write(folder.join("tiny.S01E02.mp4"), b"x").unwrap();
        let settings = AppSettings {
            min_file_size_mb: 0.000002,
            video_extensions: vec![".mp4".into()],
            ..Default::default()
        };
        let (collections, warnings) = scan_paths(&[root.to_string_lossy().into()], &settings);
        assert_eq!(collections.len(), 1);
        assert_eq!(collections[0].episode_count, 1);
        assert!(!warnings.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
