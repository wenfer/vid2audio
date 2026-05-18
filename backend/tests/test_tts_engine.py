from pathlib import Path
import subprocess

from backend.app.core.tts_engine import TextToSpeech


def test_piper_unavailable_falls_back_to_silent_placeholder(monkeypatch, tmp_path):
    """When Piper is not installed, the default provider falls back to silent placeholder."""
    calls: list[list[str]] = []

    def fake_command_available(command: str) -> bool:
        # Piper not available
        return command == "ffmpeg"

    def fake_run(command, **kwargs):
        calls.append(command)
        Path(command[-1]).write_bytes(b"silent")

    monkeypatch.setattr("backend.app.core.tts_engine.command_available", fake_command_available)
    monkeypatch.setattr("backend.app.core.tts_engine.require_command", lambda command: None)
    monkeypatch.setattr("backend.app.core.tts_engine.subprocess.run", fake_run)

    output = tmp_path / "intro.mp3"
    warning = TextToSpeech().generate("Season 1", output)

    assert warning
    assert "Piper TTS 未安装" in warning
    # Should have generated a silent placeholder via ffmpeg
    assert output.read_bytes() == b"silent"
    assert calls[0][0] == "ffmpeg"


def test_edge_tts_failure_falls_back_to_silent_placeholder(monkeypatch, tmp_path):
    """When edge-tts fails (network error), falls back to silent placeholder."""
    calls: list[list[str]] = []

    def fake_command_available(command: str) -> bool:
        return command in ("edge-tts", "ffmpeg")

    def fake_run(command, **kwargs):
        calls.append(command)
        if command[0] == "edge-tts":
            raise subprocess.CalledProcessError(1, command, stderr="network unavailable\n")
        Path(command[-1]).write_bytes(b"silent")

    monkeypatch.setattr("backend.app.core.tts_engine.command_available", fake_command_available)
    monkeypatch.setattr("backend.app.core.tts_engine.require_command", lambda command: None)
    monkeypatch.setattr("backend.app.core.tts_engine.subprocess.run", fake_run)

    output = tmp_path / "intro.mp3"
    warning = TextToSpeech(provider="edge").generate("Season 1", output)

    assert warning
    assert "片头语音生成失败" in warning
    assert output.read_bytes() == b"silent"
    assert calls[0][0] == "edge-tts"
    assert calls[1][0] == "ffmpeg"


def test_piper_success(monkeypatch, tmp_path):
    """When Piper is available and model exists, generates speech successfully."""
    calls: list[list[str]] = []
    model_dir = tmp_path / "models"
    model_dir.mkdir()
    model_file = model_dir / "zh_CN-huayan-medium.onnx"
    model_file.write_bytes(b"fake-model")

    def fake_command_available(command: str) -> bool:
        return command in ("piper", "ffmpeg")

    def fake_run(command, **kwargs):
        calls.append(command if isinstance(command, list) else [command])
        # Piper writes to --output_file
        if command[0] == "piper":
            output_idx = command.index("--output_file") + 1
            Path(command[output_idx]).write_bytes(b"piper-audio")
        else:
            # ffmpeg normalize
            Path(command[-1]).write_bytes(b"normalized")

    monkeypatch.setattr("backend.app.core.tts_engine.command_available", fake_command_available)
    monkeypatch.setattr("backend.app.core.tts_engine.require_command", lambda command: None)
    monkeypatch.setattr("backend.app.core.tts_engine.subprocess.run", fake_run)
    monkeypatch.setattr("backend.app.core.tts_engine.PIPER_MODELS_DIR", model_dir)

    output = tmp_path / "intro.mp3"
    warning = TextToSpeech(voice="zh_CN-huayan-medium").generate("你好世界", output)

    assert warning is None
    assert output.read_bytes() == b"normalized"
    assert calls[0][0] == "piper"
    assert calls[1][0] == "ffmpeg"


def test_tts_can_be_disabled(tmp_path):
    output = tmp_path / "intro.mp3"

    warning = TextToSpeech(provider="disabled").generate("Season 1", output)

    assert warning == "片头语音已禁用。"
    assert not output.exists()
