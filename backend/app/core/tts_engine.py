from __future__ import annotations

import subprocess
from pathlib import Path

from backend.app.core.media import command_available, require_command

# Default Piper voice model for Chinese
PIPER_DEFAULT_MODEL = "zh_CN-huayan-medium"
PIPER_MODELS_DIR = Path("/app/data/piper-voices")


class TextToSpeech:
    def __init__(
        self,
        voice: str = "zh-CN-XiaoxiaoNeural",
        rate: str = "+0%",
        provider: str = "piper",
        failure_mode: str = "silent",
    ) -> None:
        self.voice = voice
        self.rate = rate
        self.provider = provider
        self.failure_mode = failure_mode

    def generate(self, text: str, output_path: str | Path, bitrate: str = "128k", sample_rate: int = 44100) -> str | None:
        output = Path(output_path)
        output.parent.mkdir(parents=True, exist_ok=True)

        if self.provider == "disabled":
            return "片头语音已禁用。"
        if self.provider == "silent":
            self._silent_placeholder(output, bitrate, sample_rate)
            return "已按配置使用静音片头占位。"

        # Try the configured provider, fall back through the chain
        if self.provider == "piper":
            return self._try_piper(text, output, bitrate, sample_rate)
        if self.provider == "edge":
            return self._try_edge(text, output, bitrate, sample_rate)

        # Unknown provider, try piper then edge
        result = self._try_piper(text, output, bitrate, sample_rate)
        if result is None:
            return None
        return self._try_edge(text, output, bitrate, sample_rate)

    def _try_piper(self, text: str, output: Path, bitrate: str, sample_rate: int) -> str | None:
        """Generate speech using Piper TTS (offline, local neural TTS)."""
        if not _piper_available():
            return self._handle_failure(
                output, bitrate, sample_rate,
                "Piper TTS 未安装。请运行: pip install piper-tts 并下载语音模型。"
            )

        raw = output.with_suffix(".piper.tmp.wav")
        try:
            model_path = _resolve_piper_model(self.voice)
            cmd = [
                "piper",
                "--model", str(model_path),
                "--output_file", str(raw),
            ]
            subprocess.run(
                cmd,
                input=text,
                check=True,
                capture_output=True,
                text=True,
            )
            self._normalize(raw, output, bitrate, sample_rate)
            raw.unlink(missing_ok=True)
            return None
        except FileNotFoundError:
            raw.unlink(missing_ok=True)
            return self._handle_failure(
                output, bitrate, sample_rate,
                f"Piper 语音模型未找到: {self.voice}。请下载模型到 {PIPER_MODELS_DIR}。"
            )
        except subprocess.CalledProcessError as exc:
            raw.unlink(missing_ok=True)
            return self._handle_failure(output, bitrate, sample_rate, _subprocess_error_message(exc))
        except Exception as exc:
            raw.unlink(missing_ok=True)
            return self._handle_failure(output, bitrate, sample_rate, str(exc))

    def _try_edge(self, text: str, output: Path, bitrate: str, sample_rate: int) -> str | None:
        """Generate speech using Edge TTS (online, requires network)."""
        if not command_available("edge-tts"):
            return self._handle_failure(output, bitrate, sample_rate, "edge-tts 命令不可用。")

        raw = output.with_suffix(".tts.tmp.mp3")
        try:
            subprocess.run(
                [
                    "edge-tts",
                    "--text", text,
                    "--voice", self.voice,
                    "--rate", self.rate,
                    "--write-media", str(raw),
                ],
                check=True,
                capture_output=True,
                text=True,
            )
            self._normalize(raw, output, bitrate, sample_rate)
            raw.unlink(missing_ok=True)
            return None
        except subprocess.CalledProcessError as exc:
            raw.unlink(missing_ok=True)
            return self._handle_failure(output, bitrate, sample_rate, _subprocess_error_message(exc))

    def _handle_failure(self, output: Path, bitrate: str, sample_rate: int, reason: str) -> str:
        if self.failure_mode == "fail":
            raise RuntimeError(reason)
        if self.failure_mode == "skip":
            return f"片头语音生成失败，已跳过片头: {reason}"
        self._silent_placeholder(output, bitrate, sample_rate)
        return f"片头语音生成失败，已使用 1 秒静音占位: {reason}"

    def _normalize(self, source: Path, output: Path, bitrate: str, sample_rate: int) -> None:
        require_command("ffmpeg")
        subprocess.run(
            [
                "ffmpeg", "-y",
                "-i", str(source),
                "-af", "loudnorm=I=-16:TP=-1.5:LRA=11",
                "-c:a", "libmp3lame",
                "-b:a", bitrate,
                "-ar", str(sample_rate),
                "-ac", "2",
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
                "ffmpeg", "-y",
                "-f", "lavfi",
                "-i", f"anullsrc=channel_layout=stereo:sample_rate={sample_rate}",
                "-t", "1",
                "-c:a", "libmp3lame",
                "-b:a", bitrate,
                str(output),
            ],
            check=True,
            capture_output=True,
            text=True,
        )


def _piper_available() -> bool:
    """Check if the piper CLI is available."""
    return command_available("piper")


def _resolve_piper_model(voice: str) -> Path:
    """Resolve a Piper voice model path.

    Accepts:
    - An absolute path to a .onnx model file
    - A model name like 'zh_CN-huayan-medium' (looked up in PIPER_MODELS_DIR)
    - An edge-tts style voice name (mapped to a Piper model)
    """
    # If it's already an absolute path to a model file
    path = Path(voice)
    if path.is_absolute() and path.exists():
        return path

    # Map common edge-tts voice names to Piper models
    piper_name = _edge_voice_to_piper(voice)

    # Look in the models directory
    models_dir = PIPER_MODELS_DIR
    if not models_dir.exists():
        # Also check a local fallback path
        local_dir = Path("data/piper-voices")
        if local_dir.exists():
            models_dir = local_dir

    # Try exact match: model_name.onnx
    model_file = models_dir / f"{piper_name}.onnx"
    if model_file.exists():
        return model_file

    # Try in a subdirectory: model_name/model_name.onnx
    model_subdir = models_dir / piper_name / f"{piper_name}.onnx"
    if model_subdir.exists():
        return model_subdir

    # Try any .onnx file in the models directory as fallback
    if models_dir.exists():
        onnx_files = list(models_dir.rglob("*.onnx"))
        if onnx_files:
            return onnx_files[0]

    raise FileNotFoundError(f"Piper model not found: {piper_name} in {models_dir}")


def _edge_voice_to_piper(voice: str) -> str:
    """Map edge-tts voice names to Piper model names."""
    mapping = {
        "zh-CN-XiaoxiaoNeural": PIPER_DEFAULT_MODEL,
        "zh-CN-YunxiNeural": PIPER_DEFAULT_MODEL,
        "zh-CN-YunyangNeural": PIPER_DEFAULT_MODEL,
    }
    return mapping.get(voice, voice if not voice.endswith("Neural") else PIPER_DEFAULT_MODEL)


def _subprocess_error_message(exc: subprocess.CalledProcessError) -> str:
    stderr = (exc.stderr or "").strip()
    if stderr:
        return stderr.splitlines()[-1][-500:]
    stdout = (exc.stdout or "").strip()
    if stdout:
        return stdout.splitlines()[-1][-500:]
    return str(exc)
