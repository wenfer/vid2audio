from __future__ import annotations

import uuid
from pathlib import Path

from backend.app.core.media import probe_video
from backend.app.core.sorter import clean_title, parse_episode_number, sorted_for_filesystem
from backend.app.models.schemas import Collection, VideoFile


VIDEO_EXTENSIONS = {
    ".mp4",
    ".mkv",
    ".avi",
    ".mov",
    ".wmv",
    ".flv",
    ".webm",
    ".m4v",
    ".mpg",
    ".mpeg",
    ".ts",
    ".m2ts",
    ".vob",
}


class Scanner:
    def __init__(
        self,
        video_extensions: list[str] | None = None,
        min_file_size_mb: float = 0,
        ignored_extensions: list[str] | None = None,
        filesystem_sorting: str = "ntfs",
    ) -> None:
        self.video_extensions = {ext.lower() for ext in (video_extensions or VIDEO_EXTENSIONS)}
        self.min_file_size_bytes = int(max(min_file_size_mb, 0) * 1024 * 1024)
        self.ignored_extensions = {ext.lower() for ext in (ignored_extensions or [])}
        self.filesystem_sorting = filesystem_sorting

    def scan(self, paths: list[str]) -> tuple[list[Collection], list[str]]:
        warnings: list[str] = []
        groups: dict[Path, list[Path]] = {}
        for source in paths:
            root = Path(source).expanduser()
            if not root.exists():
                warnings.append(f"目录不存在: {root}")
                continue
            candidates = [root] if root.is_file() else root.rglob("*")
            for file_path in candidates:
                if not file_path.is_file():
                    continue
                if self._should_skip(file_path, warnings):
                    continue
                groups.setdefault(file_path.parent, []).append(file_path)

        collections: list[Collection] = []
        for folder, files in sorted(groups.items(), key=lambda item: str(item[0])):
            collection_id = str(uuid.uuid4())
            collection_name = self._collection_name(folder)
            filesystem_ordered = sorted_for_filesystem(files, self.filesystem_sorting)
            order = {path: index for index, path in enumerate(filesystem_ordered)}
            sorted_files = sorted(filesystem_ordered, key=lambda p: (parse_episode_number(p.name, 999999), order[p]))
            videos: list[VideoFile] = []
            for idx, file_path in enumerate(sorted_files, start=1):
                try:
                    metadata, tracks = probe_video(file_path)
                except Exception as exc:
                    metadata = {"duration": None, "video_codec": "", "resolution": ""}
                    tracks = []
                    warnings.append(f"无法解析 {file_path}: {exc}")
                episode_number = parse_episode_number(file_path.name, idx)
                title = clean_title(file_path.name, collection_name, fallback=f"第{episode_number:02d}集")
                video_id = str(uuid.uuid4())
                for track in tracks:
                    track.video_file_id = video_id
                    track.id = str(uuid.uuid4())
                videos.append(
                    VideoFile(
                        id=video_id,
                        collection_id=collection_id,
                        filename=file_path.name,
                        filepath=str(file_path.resolve()),
                        file_size=file_path.stat().st_size,
                        duration=metadata.get("duration"),
                        video_codec=str(metadata.get("video_codec") or ""),
                        resolution=str(metadata.get("resolution") or ""),
                        audio_tracks=tracks,
                        episode_number=episode_number,
                        episode_title=title,
                    )
                )
            collections.append(
                Collection(
                    id=collection_id,
                    name=collection_name,
                    source_path=str(folder.resolve()),
                    episode_count=len(videos),
                    video_files=videos,
                )
            )
        return collections, warnings

    @staticmethod
    def _collection_name(folder: Path) -> str:
        parent = folder.parent.name
        current = folder.name
        if current in {"第一季", "第二季", "第三季", "第四季", "第五季"} and parent:
            return f"{parent}{current}"
        return current

    def _should_skip(self, file_path: Path, warnings: list[str]) -> bool:
        suffix = file_path.suffix.lower()
        if suffix in self.ignored_extensions:
            warnings.append(f"已过滤后缀: {file_path}")
            return True
        if suffix not in self.video_extensions:
            return True
        size = file_path.stat().st_size
        if size < self.min_file_size_bytes:
            warnings.append(f"已过滤小文件: {file_path}")
            return True
        return False
