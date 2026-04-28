from __future__ import annotations

from datetime import datetime
from enum import Enum
from pathlib import Path
from typing import Any

from pydantic import BaseModel, Field


class CollectionStatus(str, Enum):
    pending = "pending"
    scanned = "scanned"
    processing = "processing"
    completed = "completed"
    error = "error"


class JobStatus(str, Enum):
    queued = "queued"
    processing = "processing"
    completed = "completed"
    failed = "failed"
    cancelled = "cancelled"


class AudioTrack(BaseModel):
    id: str | None = None
    video_file_id: str | None = None
    index: int
    codec: str = ""
    language: str = "und"
    language_full: str = "未知语言"
    channels: int | None = None
    sample_rate: int | None = None
    bitrate: int | None = None
    title: str = ""
    default: bool = False


class VideoFile(BaseModel):
    id: str
    collection_id: str | None = None
    filename: str
    filepath: str
    file_size: int = 0
    duration: float | None = None
    video_codec: str = ""
    resolution: str = ""
    audio_tracks: list[AudioTrack] = Field(default_factory=list)
    episode_number: int
    episode_title: str
    status: str = "pending"


class Collection(BaseModel):
    id: str
    name: str
    source_path: str
    output_path: str | None = None
    episode_count: int = 0
    status: CollectionStatus = CollectionStatus.scanned
    video_files: list[VideoFile] = Field(default_factory=list)
    created_at: datetime | None = None
    updated_at: datetime | None = None
    settings: dict[str, Any] = Field(default_factory=dict)


class AppSettings(BaseModel):
    scan_directories: list[str] = Field(default_factory=lambda: ["/app/input"])
    auto_scan_interval: int = 0
    video_extensions: list[str] = Field(
        default_factory=lambda: [
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
        ]
    )
    min_file_size_mb: float = 1.0
    ignored_extensions: list[str] = Field(default_factory=lambda: [".part", ".tmp", ".download", ".ds_store"])
    default_output_format: str = "mp3"
    default_quality: str = "standard"
    default_sample_rate: int = 44100
    default_language: str = "zh"
    tts_enabled: bool = True
    tts_provider: str = "edge"
    tts_voice: str = "zh-CN-XiaoxiaoNeural"
    tts_rate: str = "+0%"
    tts_volume_normalize: bool = True
    tts_failure_mode: str = "silent"
    intro_text_template: str = "{collection_name}"
    output_directory: str = "/app/output"
    filename_template: str = "{index}_{title}"
    padding_digits: str = "auto"
    filesystem_sorting: str = "ntfs"
    preserve_original_audio: bool = False
    max_concurrent_jobs: int = 2
    ffmpeg_threads: int = 4
    hardware_acceleration: str = "auto"
    hardware_acceleration_device: str = ""
    hardware_acceleration_fallback: bool = True


class ScanRequest(BaseModel):
    directories: list[str] | None = None
    source_paths: list[str] | None = None


class ScanResult(BaseModel):
    scan_id: str
    collections: list[Collection]
    files_found: int
    warnings: list[str] = Field(default_factory=list)


class ExtractRequest(BaseModel):
    collection_id: str | None = None
    source_path: str | None = None
    job_name: str | None = None
    track_index: int = 0
    output_format: str = "mp3"
    quality: str = "standard"
    sample_rate: int = 44100
    generate_intro: bool = True
    intro_voice: str = "zh-CN-XiaoxiaoNeural"
    tts_provider: str | None = None
    tts_rate: str | None = None
    tts_failure_mode: str | None = None
    intro_text: str | None = None
    selected_video_ids: list[str] | None = None
    trim_start_seconds: float = 0
    trim_end_seconds: float = 0
    hardware_acceleration: str | None = None
    filesystem_sorting: str | None = None
    padding_digits: str | None = None


class ExtractJob(BaseModel):
    id: str
    collection_id: str | None = None
    name: str = ""
    source_path: str = ""
    status: JobStatus = JobStatus.queued
    progress: int = 0
    current_file: str | None = None
    selected_track_index: int = 0
    output_format: str = "mp3"
    quality_setting: str = "standard"
    trim_start_seconds: float = 0
    trim_end_seconds: float = 0
    total_count: int = 0
    success_count: int = 0
    failure_count: int = 0
    error_message: str | None = None
    output_path: str | None = None
    summary: dict[str, Any] = Field(default_factory=dict)
    created_at: datetime | None = None
    started_at: datetime | None = None
    completed_at: datetime | None = None


class ExtractJobItem(BaseModel):
    id: str
    job_id: str
    video_file_id: str | None = None
    source_path: str
    output_path: str | None = None
    title: str
    status: str = "pending"
    error_message: str | None = None
    created_at: datetime | None = None
    completed_at: datetime | None = None


class ExtractJobDetail(ExtractJob):
    items: list[ExtractJobItem] = Field(default_factory=list)


class PreviewFile(BaseModel):
    source: str
    output: str
    title: str
    episode_number: int


class SystemStatus(BaseModel):
    version: str
    ffmpeg_available: bool
    ffprobe_available: bool
    database_path: str
    input_directories: list[str]
    output_directory: str
    hardware_acceleration: dict[str, Any] = Field(default_factory=dict)


def ensure_path(value: str) -> Path:
    return Path(value).expanduser().resolve()
