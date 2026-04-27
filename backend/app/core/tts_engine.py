from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

from backend.app.core.media import command_available, require_command


class TextToSpeech:
    def __init__(self, voice: str = "zh-CN-XiaoxiaoNeural", rate: str = "+0%") -> None:
        self.voice = voice
        self.rate = rate

    def generate(self, text: str, output_path: str | Path, bitrate: str = "128k", sample_rate: int = 44100) -> None:
        output = Path(output_path)
        output.parent.mkdir(parents=True, exist_ok=True)
        if command_available("edge-tts"):
            raw = output.with_suffix(".tts.tmp.mp3")
            subprocess.run(
                [
                    "edge-tts",
                    "--text",
                    text,
                    "--voice",
                    self.voice,
                    "--rate",
                    self.rate,
                    "--write-media",
                    str(raw),
                ],
                check=True,
                capture_output=True,
                text=True,
            )
            self._normalize(raw, output, bitrate, sample_rate)
            raw.unlink(missing_ok=True)
            return
        self._silent_placeholder(output, bitrate, sample_rate)

    def _normalize(self, source: Path, output: Path, bitrate: str, sample_rate: int) -> None:
        require_command("ffmpeg")
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-i",
                str(source),
                "-af",
                "loudnorm=I=-16:TP=-1.5:LRA=11",
                "-c:a",
                "libmp3lame",
                "-b:a",
                bitrate,
                "-ar",
                str(sample_rate),
                "-ac",
                "2",
                str(output),
            ],
            check=True,
            capture_output=True,
            text=True,
        )

    def _silent_placeholder(self, output: Path, bitrate: str, sample_rate: int) -> None:
        require_command("ffmpeg")
        subprocess.run(
            [
                "ffmpeg",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=channel_layout=stereo:sample_rate=%s" % sample_rate,
                "-t",
                "1",
                "-c:a",
                "libmp3lame",
                "-b:a",
                bitrate,
                str(output),
            ],
            check=True,
            capture_output=True,
            text=True,
        )


def tts_available() -> bool:
    return shutil.which("edge-tts") is not None
