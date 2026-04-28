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
    if os.getenv("VID2AUDIO_INPUT") and not values.get("scan_directories"):
        values["scan_directories"] = [os.environ["VID2AUDIO_INPUT"]]
    if os.getenv("VID2AUDIO_OUTPUT") and not values.get("output_directory"):
        values["output_directory"] = os.environ["VID2AUDIO_OUTPUT"]
    if os.getenv("VID2AUDIO_HWACCEL") and not values.get("hardware_acceleration"):
        values["hardware_acceleration"] = os.environ["VID2AUDIO_HWACCEL"]
    if os.getenv("VID2AUDIO_HWACCEL_DEVICE") and not values.get("hardware_acceleration_device"):
        values["hardware_acceleration_device"] = os.environ["VID2AUDIO_HWACCEL_DEVICE"]
    if os.getenv("VID2AUDIO_HWACCEL_FALLBACK") and "hardware_acceleration_fallback" not in values:
        values["hardware_acceleration_fallback"] = os.environ["VID2AUDIO_HWACCEL_FALLBACK"].lower() in {
            "1",
            "true",
            "yes",
            "on",
        }
    return defaults.model_copy(update=values)
