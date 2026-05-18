from __future__ import annotations

from fastapi import APIRouter

from backend.app.api.deps import get_collection_service
from backend.app.models.schemas import ScanRequest

router = APIRouter(prefix="/scan", tags=["scan"])


@router.post("/start")
def start_scan(request: ScanRequest):
    return get_collection_service().scan(request.source_paths or request.directories)
