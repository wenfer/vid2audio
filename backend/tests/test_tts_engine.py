from pathlib import Path
import subprocess

from backend.app.core.tts_engine import TextToSpeech


def test_edge_tts_failure_falls_back_to_silent_placeholder(monkeypatch, tmp_path):
    calls: list[list[str]] = []

    def fake_command_available(command: str) -> bool:
        return command == "edge-tts"

    def fake_run(command, check, capture_output, text):
        calls.append(command)
        if command[0] == "edge-tts":
            raise subprocess.CalledProcessError(1, command, stderr="network unavailable\n")
        Path(command[-1]).write_bytes(b"silent")

    monkeypatch.setattr("backend.app.core.tts_engine.command_available", fake_command_available)
    monkeypatch.setattr("backend.app.core.tts_engine.require_command", lambda command: None)
    monkeypatch.setattr("backend.app.core.tts_engine.subprocess.run", fake_run)

    output = tmp_path / "intro.mp3"
    warning = TextToSpeech().generate("Season 1", output)

    assert warning
    assert "片头语音生成失败" in warning
    assert output.read_bytes() == b"silent"
    assert calls[0][0] == "edge-tts"
    assert calls[1][0] == "ffmpeg"


def test_tts_can_be_disabled(tmp_path):
    output = tmp_path / "intro.mp3"

    warning = TextToSpeech(provider="disabled").generate("Season 1", output)

    assert warning == "片头语音已禁用。"
    assert not output.exists()
