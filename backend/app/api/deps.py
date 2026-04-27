from __future__ import annotations

import os
from functools import lru_cache
from pathlib import Path

from backend.app.models.database import Database
from backend.app.services.collection_service import CollectionService
from backend.app.services.extract_service import ExtractService
from backend.app.services.settings_service import load_settings


@lru_cache
def get_db() -> Database:
    db_path = Path(os.getenv("VID2AUDIO_DB", "data/vid2audio.db"))
    return Database(db_path)


def get_collection_service() -> CollectionService:
    db = get_db()
    return CollectionService(db, load_settings(db))


def get_extract_service() -> ExtractService:
    db = get_db()
    settings = load_settings(db)
    collections = CollectionService(db, settings)
    return ExtractService(db, settings, collections)
