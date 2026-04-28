from __future__ import annotations

from pathlib import Path

from fastapi import APIRouter, HTTPException
from fastapi.responses import FileResponse

from backend.app.api.deps import get_collection_service, get_db, get_extract_service
from backend.app.core.extractor import Extractor
from backend.app.models.schemas import ExtractRequest
from backend.app.services.settings_service import load_settings

router = APIRouter(tags=["extract"])


@router.post("/extract")
def create_extract_job(request: ExtractRequest):
    return get_extract_service().create_job(request)


@router.get("/extract/jobs")
def list_jobs():
    return get_extract_service().list_jobs()


@router.get("/extract/jobs/{job_id}")
def get_job(job_id: str):
    job = get_extract_service().get_job_detail(job_id)
    if not job:
        raise HTTPException(status_code=404, detail="任务不存在")
    return job


@router.post("/extract/jobs/{job_id}/cancel")
def cancel_job(job_id: str):
    job = get_extract_service().cancel_job(job_id)
    if not job:
        raise HTTPException(status_code=404, detail="任务不存在")
    return job


@router.post("/extract/jobs/{job_id}/retry")
def retry_job(job_id: str):
    raise HTTPException(status_code=501, detail="MVP 暂未保存重试所需的完整请求快照")


@router.get("/extract/jobs/{job_id}/items/{item_id}/audio")
def play_extracted_audio(job_id: str, item_id: str):
    job = get_extract_service().get_job_detail(job_id)
    if not job:
        raise HTTPException(status_code=404, detail="任务不存在")
    item = next((entry for entry in job.items if entry.id == item_id), None)
    if not item:
        raise HTTPException(status_code=404, detail="任务文件不存在")
    if item.status != "completed" or not item.output_path:
        raise HTTPException(status_code=409, detail="音频文件尚未生成")
    output = Path(item.output_path)
    if not output.is_file():
        raise HTTPException(status_code=404, detail="音频文件不存在")
    return FileResponse(output, media_type=_audio_media_type(output), filename=output.name)


@router.get("/preview/{video_id}")
def preview(video_id: str, track: int = 0, duration: int = 10, start: float = 0):
    collection_service = get_collection_service()
    for collection in collection_service.list_collections():
        detail = collection_service.get_collection(collection.id)
        if not detail:
            continue
        video = next((item for item in detail.video_files if item.id == video_id), None)
        if video:
            settings = load_settings(get_db())
            output = Path("/tmp/vid2audio") / f"preview_{video_id}_{track}.mp3"
            Extractor(
                settings.output_directory,
                settings.ffmpeg_threads,
                settings.hardware_acceleration,
                settings.hardware_acceleration_device,
                settings.hardware_acceleration_fallback,
            ).preview(video.filepath, track, output, duration, start)
            return FileResponse(output, media_type="audio/mpeg", filename=output.name)
    raise HTTPException(status_code=404, detail="视频不存在")


def _audio_media_type(path: Path) -> str:
    suffix = path.suffix.lower()
    return {
        ".mp3": "audio/mpeg",
        ".m4a": "audio/mp4",
        ".aac": "audio/aac",
        ".ogg": "audio/ogg",
        ".opus": "audio/ogg",
        ".flac": "audio/flac",
        ".wav": "audio/wav",
    }.get(suffix, "application/octet-stream")
