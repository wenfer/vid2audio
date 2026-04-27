from __future__ import annotations

import os
from pathlib import Path

from backend.app.models.database import Database
from backend.app.models.schemas import AppSettings


def default_data_dir() -> Path:
    return Path(os.getenv("VID2AUDIO_DATA_DIR", "/app/data"))


def load_settings(db: Database) -> AppSettings:
    values = db.get_settings()
    defaults = AppSettings()
    if os.getenv("VID2AUDIO_INPUT"):
        values["scan_directories"] = [os.environ["VID2AUDIO_INPUT"]]
    if os.getenv("VID2AUDIO_OUTPUT"):
        values["output_directory"] = os.environ["VID2AUDIO_OUTPUT"]
    return defaults.model_copy(update=values)
