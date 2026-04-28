from __future__ import annotations

import subprocess
from pathlib import Path
from typing import Callable

from backend.app.core.media import ffmpeg_acceleration_args, require_command, resolve_hardware_acceleration
from backend.app.core.sorter import calculate_padding, generate_filename, intro_filename, verify_sorting
from backend.app.core.tts_engine import TextToSpeech
from backend.app.models.schemas import Collection, ExtractRequest


QUALITY_BITRATES = {
    "economy": "64k",
    "standard": "128k",
    "premium": "192k",
    "lossless": "320k",
}

CODECS = {
    "mp3": "libmp3lame",
    "m4a": "aac",
    "aac": "aac",
    "ogg": "libvorbis",
    "flac": "flac",
    "wav": "pcm_s16le",
    "opus": "libopus",
}


class Extractor:
    def __init__(
        self,
        output_directory: str,
        ffmpeg_threads: int = 4,
        hardware_acceleration: str = "auto",
        hardware_acceleration_device: str = "",
        hardware_acceleration_fallback: bool = True,
    ) -> None:
        self.output_directory = Path(output_directory).expanduser()
        self.ffmpeg_threads = ffmpeg_threads
        self.hardware_acceleration = hardware_acceleration
        self.hardware_acceleration_device = hardware_acceleration_device
        self.hardware_acceleration_fallback = hardware_acceleration_fallback
        self.acceleration_events: list[dict[str, str]] = []
        self.warnings: list[str] = []

    def extract_collection(
        self,
        collection: Collection,
        request: ExtractRequest,
        on_item_start: Callable[[str, int, int], None] | None = None,
        on_item_done: Callable[[str, Path], None] | None = None,
        on_item_failed: Callable[[str, str], None] | None = None,
    ) -> tuple[Path, list[str], list[dict[str, str]]]:
        require_command("ffmpeg")
        extension = request.output_format.lower().lstrip(".")
        if extension == "aac":
            extension = "m4a"
        bitrate = QUALITY_BITRATES.get(request.quality, QUALITY_BITRATES["standard"])
        request_acceleration = resolve_hardware_acceleration(request.hardware_acceleration or self.hardware_acceleration)
        videos = collection.video_files
        if request.selected_video_ids:
            selected = set(request.selected_video_ids)
            videos = [video for video in videos if video.id in selected]
        videos = sorted(videos, key=lambda item: (item.episode_number, item.filename))
        output_dir = self.output_directory / collection.name
        output_dir.mkdir(parents=True, exist_ok=True)
        padding = calculate_padding(len(videos))

        if request.generate_intro:
            intro_path = output_dir / intro_filename(collection.name, extension)
            warning = TextToSpeech(request.intro_voice).generate(collection.name, intro_path, bitrate, request.sample_rate)
            if warning:
                self.warnings.append(warning)

        generated: list[str] = []
        failures: list[dict[str, str]] = []
        for display_index, video in enumerate(videos, start=1):
            output_name = generate_filename(display_index, video.episode_title, extension, padding)
            output_path = output_dir / output_name
            if on_item_start:
                on_item_start(video.id, display_index, len(videos))
            try:
                self._extract_one(
                    video.filepath,
                    request.track_index,
                    output_path,
                    extension,
                    bitrate,
                    request.sample_rate,
                    video.duration,
                    request.trim_start_seconds,
                    request.trim_end_seconds,
                    request_acceleration,
                )
                generated.append(output_name)
                if on_item_done:
                    on_item_done(video.id, output_path)
            except Exception as exc:
                message = _subprocess_error_message(exc)
                failures.append({"source": video.filepath, "title": video.episode_title, "error": message})
                if on_item_failed:
                    on_item_failed(video.id, message)

        ordered = verify_sorting(output_dir, extension)
        return output_dir, ordered or generated, failures

    def preview(
        self,
        source: str,
        track_index: int,
        output: str | Path,
        duration: int = 10,
        trim_start_seconds: float = 0,
        hardware_acceleration: str | None = None,
    ) -> Path:
        require_command("ffmpeg")
        output_path = Path(output)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        mode = resolve_hardware_acceleration(hardware_acceleration or self.hardware_acceleration)
        command = ["ffmpeg", "-y"]
        if trim_start_seconds > 0:
            command.extend(["-ss", str(trim_start_seconds)])
        command.extend(ffmpeg_acceleration_args(mode, self.hardware_acceleration_device))
        command.extend(
            [
                "-i",
                source,
                "-map",
                f"0:{track_index}",
                "-t",
                str(duration),
                "-c:a",
                "libmp3lame",
                "-b:a",
                "128k",
                str(output_path),
            ]
        )
        self._run_with_optional_fallback(command, mode, source, output_path)
        return output_path

    def _extract_one(
        self,
        source: str,
        track_index: int,
        output_path: Path,
        extension: str,
        bitrate: str,
        sample_rate: int,
        duration: float | None,
        trim_start_seconds: float,
        trim_end_seconds: float,
        hardware_acceleration: str,
    ) -> None:
        codec = CODECS.get(extension)
        if not codec:
            raise ValueError(f"不支持的输出格式: {extension}")
        command = ["ffmpeg", "-y"]
        if trim_start_seconds > 0:
            command.extend(["-ss", str(trim_start_seconds)])
        command.extend(ffmpeg_acceleration_args(hardware_acceleration, self.hardware_acceleration_device))
        command.extend(
            [
                "-i",
                source,
                "-map",
                f"0:{track_index}",
                "-c:a",
                codec,
            ]
        )
        output_duration = None
        if duration is not None:
            output_duration = max(duration - trim_start_seconds - trim_end_seconds, 0)
        if output_duration:
            command.extend(["-t", f"{output_duration:.3f}"])
        if extension not in {"flac", "wav"}:
            command.extend(["-b:a", bitrate])
        command.extend(["-ar", str(sample_rate), "-ac", "2", "-threads", str(self.ffmpeg_threads), str(output_path)])
        self._run_with_optional_fallback(command, hardware_acceleration, source, output_path)

    def _run_with_optional_fallback(
        self,
        command: list[str],
        hardware_acceleration: str,
        source: str,
        output_path: Path,
    ) -> None:
        try:
            subprocess.run(command, check=True, capture_output=True, text=True)
        except subprocess.CalledProcessError as exc:
            if not self.hardware_acceleration_fallback or not ffmpeg_acceleration_args(
                hardware_acceleration, self.hardware_acceleration_device
            ):
                raise
            fallback = _remove_hwaccel_args(command)
            subprocess.run(fallback, check=True, capture_output=True, text=True)
            self.acceleration_events.append(
                {
                    "source": source,
                    "output": str(output_path),
                    "mode": resolve_hardware_acceleration(hardware_acceleration),
                    "requested_mode": str(hardware_acceleration),
                    "message": f"{resolve_hardware_acceleration(hardware_acceleration)} 失败，已自动回退到 CPU。",
                    "reason": _subprocess_error_message(exc),
                }
            )


def _subprocess_error_message(exc: Exception) -> str:
    if isinstance(exc, subprocess.CalledProcessError):
        stderr = (exc.stderr or "").strip()
        if stderr:
            return stderr.splitlines()[-1][-500:]
    return str(exc)


def _remove_hwaccel_args(command: list[str]) -> list[str]:
    cleaned: list[str] = []
    skip_next = False
    options_with_values = {"-hwaccel", "-hwaccel_device", "-hwaccel_output_format"}
    for item in command:
        if skip_next:
            skip_next = False
            continue
        if item in options_with_values:
            skip_next = True
            continue
        cleaned.append(item)
    return cleaned
