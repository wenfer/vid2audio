from __future__ import annotations

from fastapi import APIRouter

from backend.app.api.deps import get_db
from backend.app.models.schemas import AppSettings
from backend.app.services.settings_service import load_settings

router = APIRouter(prefix="/settings", tags=["settings"])


@router.get("")
def get_settings():
    return load_settings(get_db())


@router.put("")
def update_settings(values: dict):
    db = get_db()
    current = load_settings(db).model_dump()
    current.update(values)
    settings = AppSettings(**current)
    db.update_settings(settings.model_dump())
    return settings
