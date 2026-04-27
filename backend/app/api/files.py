from __future__ import annotations

from pathlib import Path

from fastapi import APIRouter, HTTPException

from backend.app.api.deps import get_db
from backend.app.services.settings_service import load_settings

router = APIRouter(prefix="/files", tags=["files"])


@router.get("")
def browse(path: str | None = None):
    settings = load_settings(get_db())
    current = Path(path or (settings.scan_directories[0] if settings.scan_directories else "/app/input")).expanduser()
    if not current.exists():
        raise HTTPException(status_code=404, detail="路径不存在")
    if current.is_file():
        current = current.parent
    entries = []
    video_extensions = {item.lower() for item in settings.video_extensions}
    ignored_extensions = {item.lower() for item in settings.ignored_extensions}
    min_size = int(max(settings.min_file_size_mb, 0) * 1024 * 1024)
    try:
        children = sorted(current.iterdir(), key=lambda item: (not item.is_dir(), item.name.lower()))
    except PermissionError as exc:
        raise HTTPException(status_code=403, detail=f"没有权限读取目录: {current}") from exc
    for child in children:
        if child.name.startswith("."):
            continue
        is_dir = child.is_dir()
        suffix = child.suffix.lower()
        size = child.stat().st_size if child.is_file() else 0
        is_video = child.is_file() and suffix in video_extensions
        ignored = child.is_file() and suffix in ignored_extensions
        too_small = child.is_file() and is_video and size < min_size
        selectable = is_dir or (is_video and not ignored and not too_small)
        reason = ""
        if ignored:
            reason = "已按后缀过滤"
        elif too_small:
            reason = "小于最小文件大小"
        elif child.is_file() and not is_video:
            reason = "非视频文件"
        entries.append(
            {
                "name": child.name,
                "path": str(child.resolve()),
                "type": "directory" if is_dir else "file",
                "size": size,
                "extension": suffix,
                "is_video": is_video,
                "selectable": selectable,
                "reason": reason,
            }
        )
    return {
        "path": str(current.resolve()),
        "parent": str(current.parent.resolve()) if current.parent != current else None,
        "entries": entries,
    }
