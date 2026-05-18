from __future__ import annotations

from fastapi import APIRouter

from backend.app import __version__
from backend.app.api.deps import get_db
from backend.app.core.media import command_available, detect_hardware_acceleration
from backend.app.models.schemas import SystemStatus
from backend.app.services.settings_service import load_settings

router = APIRouter(prefix="/system", tags=["system"])


@router.get("/status", response_model=SystemStatus)
def status():
    db = get_db()
    settings = load_settings(db)
    return SystemStatus(
        version=__version__,
        ffmpeg_available=command_available("ffmpeg"),
        ffprobe_available=command_available("ffprobe"),
        database_path=str(db.path),
        input_directories=settings.scan_directories,
        output_directory=settings.output_directory,
        hardware_acceleration=detect_hardware_acceleration(),
    )


@router.get("/hardware-acceleration")
def hardware_acceleration():
    """Return hardware acceleration detection results with backend details."""
    return detect_hardware_acceleration()


@router.post("/hardware-acceleration/detect")
def redetect_hardware_acceleration():
    """Force re-detection of hardware acceleration capabilities."""
    return detect_hardware_acceleration()
