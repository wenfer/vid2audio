from __future__ import annotations

import re
from pathlib import Path
from typing import Iterable


INVALID_CHARS = {
    "\\": "",
    "/": "",
    ":": "：",
    "*": "",
    "?": "？",
    '"': "",
    "<": "",
    ">": "",
    "|": "",
}

TECH_TAGS = [
    "1080p",
    "720p",
    "480p",
    "2160p",
    "4K",
    "8K",
    "WEB-DL",
    "WEBRip",
    "BluRay",
    "BDRip",
    "HDRip",
    "x264",
    "x265",
    "H264",
    "H265",
    "HEVC",
    "AVC",
    "AAC",
    "DTS",
    "DD5.1",
    "AC3",
    "CHS",
    "CHT",
    "GB",
    "BIG5",
    "简体",
    "繁体",
]

EPISODE_PATTERNS = [
    re.compile(r"S\d{1,2}E(?P<num>\d{1,4})", re.IGNORECASE),
    re.compile(r"第\s*(?P<num>\d{1,4})\s*[集话話]"),
    re.compile(r"(?<!\d)(?P<num>\d{1,4})(?!\d)"),
]


def calculate_padding(total_episodes: int, padding_digits: str = "auto") -> int:
    if padding_digits != "auto":
        try:
            return max(int(padding_digits), 1)
        except ValueError:
            return 3
    if total_episodes < 100:
        return 3
    if total_episodes < 1000:
        return 3
    return 4


def sanitize_filename_part(value: str, max_length: int = 50) -> str:
    cleaned = value.strip()
    for old, new in INVALID_CHARS.items():
        cleaned = cleaned.replace(old, new)
    cleaned = re.sub(r"\s+", " ", cleaned)
    cleaned = cleaned.strip(" ._-")
    return cleaned[:max_length] or "未命名"


def parse_episode_number(filename: str, fallback: int) -> int:
    stem = Path(filename).stem
    for pattern in EPISODE_PATTERNS:
        match = pattern.search(stem)
        if match:
            return int(match.group("num"))
    return fallback


def clean_title(filename: str, collection_name: str = "", fallback: str = "") -> str:
    title = Path(filename).stem
    title = re.sub(r"S\d{1,2}E\d{1,4}", " ", title, flags=re.IGNORECASE)
    title = re.sub(r"第\s*\d{1,4}\s*[集话話]", " ", title)
    for tag in TECH_TAGS:
        title = re.sub(rf"(?i)(^|[\s._\-\[\]()]){re.escape(tag)}($|[\s._\-\[\]()])", " ", title)
    title = re.sub(r"(?<!\d)\d{1,4}(?!\d)", " ", title, count=1)
    if collection_name:
        title = title.replace(collection_name, " ")
        collection_alias = re.sub(r"第[一二三四五六七八九十\d]+季", " ", collection_name)
        for part in re.split(r"[/\\._\-\s]+", f"{collection_name} {collection_alias}"):
            if part:
                title = title.replace(part, " ")
    title = re.sub(r"[\[\]()（）【】._-]+", " ", title)
    title = sanitize_filename_part(title)
    if title in {"未命名", ""} and fallback:
        return sanitize_filename_part(fallback)
    return title


def generate_filename(episode_num: int, title: str, extension: str, padding: int) -> str:
    clean = sanitize_filename_part(title)
    ext = extension.lstrip(".")
    return f"{str(episode_num).zfill(padding)}_{clean}.{ext}"


def intro_filename(collection_name: str, extension: str) -> str:
    clean = sanitize_filename_part(collection_name)
    return f"000_{clean}.{extension.lstrip('.')}"


def sort_key(value: str | Path, strategy: str = "ntfs") -> tuple:
    name = value.name if isinstance(value, Path) else str(value)
    strategy = (strategy or "ntfs").lower()
    if strategy == "natural":
        parts = re.split(r"(\d+)", name.casefold())
        return tuple((0, int(part)) if part.isdigit() else (1, part) for part in parts)
    if strategy == "name":
        return (name.casefold(),)
    return tuple(ord(char) for char in name.casefold())


def sorted_for_filesystem(values: Iterable[str | Path], strategy: str = "ntfs") -> list:
    return sorted(values, key=lambda item: sort_key(item, strategy))


def verify_sorting(output_dir: str | Path, extension: str) -> list[str]:
    ext = "." + extension.lstrip(".")
    files = [p.name for p in Path(output_dir).iterdir() if p.suffix.lower() == ext.lower()]
    sorted_files = sorted_for_filesystem(files, "ntfs")
    numeric_prefixes = [name.split("_", 1)[0] for name in sorted_files]
    if numeric_prefixes != sorted(numeric_prefixes):
        raise ValueError(f"排序验证失败: {sorted_files}")
    return sorted_files
