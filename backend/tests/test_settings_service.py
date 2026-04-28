from backend.app.models.database import Database
from backend.app.services.settings_service import load_settings


def test_saved_settings_override_environment_defaults(monkeypatch, tmp_path):
    db = Database(tmp_path / "vid2audio.db")
    db.update_settings(
        {
            "scan_directories": ["/custom/input"],
            "output_directory": "/custom/output",
            "hardware_acceleration": "safe",
        }
    )
    monkeypatch.setenv("VID2AUDIO_INPUT", "/env/input")
    monkeypatch.setenv("VID2AUDIO_OUTPUT", "/env/output")
    monkeypatch.setenv("VID2AUDIO_HWACCEL", "cuda")

    settings = load_settings(db)

    assert settings.scan_directories == ["/custom/input"]
    assert settings.output_directory == "/custom/output"
    assert settings.hardware_acceleration == "safe"
