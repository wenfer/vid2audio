from __future__ import annotations

import json
import threading
import uuid
from datetime import datetime
from pathlib import Path

from backend.app.core.extractor import Extractor
from backend.app.core.media import resolve_hardware_acceleration
from backend.app.models.database import Database
from backend.app.models.schemas import (
    AppSettings,
    ExtractJob,
    ExtractJobDetail,
    ExtractJobItem,
    ExtractRequest,
    JobStatus,
)
from backend.app.services.collection_service import CollectionService


class ExtractService:
    def __init__(self, db: Database, settings: AppSettings, collections: CollectionService) -> None:
        self.db = db
        self.settings = settings
        self.collections = collections

    def create_job(self, request: ExtractRequest) -> ExtractJob:
        request = self._apply_request_defaults(request)
        collection = None
        warnings: list[str] = []
        if request.source_path:
            collection, warnings = self.collections.create_collection_from_source(request.source_path, request.job_name)
            if collection is None:
                job = self._insert_job(request, None, request.job_name or Path(request.source_path).name, request.source_path, 0)
                self._mark_failed(job.id, "没有找到符合过滤条件的视频文件。" + ("; ".join(warnings[:3]) if warnings else ""))
                return self.get_job(job.id) or job
            request.collection_id = collection.id
        elif request.collection_id:
            collection = self.collections.get_collection(request.collection_id)
        if collection is None:
            job = self._insert_job(request, None, request.job_name or "未命名任务", request.source_path or "", 0)
            self._mark_failed(job.id, "合集不存在或源路径无效")
            return self.get_job(job.id) or job
        if not request.intro_text:
            request.intro_text = self._intro_text(request.job_name or collection.name)

        videos = collection.video_files
        if request.selected_video_ids:
            selected = set(request.selected_video_ids)
            videos = [video for video in videos if video.id in selected]

        job = self._insert_job(request, collection.id, request.job_name or collection.name, collection.source_path, len(videos))
        self._insert_items(job.id, videos)
        thread = threading.Thread(target=self._run_sync, args=(job.id, request), daemon=True)
        thread.start()
        return self.get_job(job.id) or job

    def _apply_request_defaults(self, request: ExtractRequest) -> ExtractRequest:
        values = request.model_dump()
        values["generate_intro"] = bool(values["generate_intro"] and self.settings.tts_enabled)
        values["intro_voice"] = values["intro_voice"] or self.settings.tts_voice
        values["tts_provider"] = values["tts_provider"] or self.settings.tts_provider
        values["tts_rate"] = values["tts_rate"] or self.settings.tts_rate
        values["tts_failure_mode"] = values["tts_failure_mode"] or self.settings.tts_failure_mode
        values["filesystem_sorting"] = values["filesystem_sorting"] or self.settings.filesystem_sorting
        values["padding_digits"] = values["padding_digits"] or self.settings.padding_digits
        return ExtractRequest(**values)

    def _intro_text(self, fallback_name: str) -> str:
        template = self.settings.intro_text_template or "{collection_name}"
        name = Path(fallback_name).name if fallback_name else ""
        try:
            return template.format(collection_name=name or "合集")
        except (KeyError, ValueError):
            return name or "合集"

    def _insert_job(
        self,
        request: ExtractRequest,
        collection_id: str | None,
        name: str,
        source_path: str,
        total_count: int,
    ) -> ExtractJob:
        collection_id = collection_id or self._ensure_placeholder_collection(name, source_path)
        job = ExtractJob(
            id=str(uuid.uuid4()),
            collection_id=collection_id,
            name=name,
            source_path=source_path,
            selected_track_index=request.track_index,
            output_format=request.output_format,
            quality_setting=request.quality,
            trim_start_seconds=request.trim_start_seconds,
            trim_end_seconds=request.trim_end_seconds,
            total_count=total_count,
        )
        with self.db.connect() as conn:
            conn.execute(
                """
                INSERT INTO extract_jobs(
                    id, collection_id, name, source_path, status, progress, selected_track_index,
                    output_format, quality_setting, trim_start_seconds, trim_end_seconds, total_count, summary
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                (
                    job.id,
                    job.collection_id,
                    job.name,
                    job.source_path,
                    job.status.value,
                    job.progress,
                    job.selected_track_index,
                    job.output_format,
                    job.quality_setting,
                    job.trim_start_seconds,
                    job.trim_end_seconds,
                    job.total_count,
                    json.dumps({"warnings": []}, ensure_ascii=False),
                ),
            )
        return job

    def _ensure_placeholder_collection(self, name: str, source_path: str) -> str:
        collection_id = str(uuid.uuid4())
        with self.db.connect() as conn:
            conn.execute(
                """
                INSERT INTO collections(id, name, source_path, episode_count, status, settings)
                VALUES (?, ?, ?, 0, 'error', '{}')
                """,
                (collection_id, name or "无效源路径", source_path or ""),
            )
        return collection_id

    def _insert_items(self, job_id: str, videos) -> None:
        with self.db.connect() as conn:
            for video in videos:
                conn.execute(
                    """
                    INSERT INTO extract_job_items(id, job_id, video_file_id, source_path, title, status)
                    VALUES (?, ?, ?, ?, ?, 'pending')
                    """,
                    (str(uuid.uuid4()), job_id, video.id, video.filepath, video.episode_title),
                )

    def _run_sync(self, job_id: str, request: ExtractRequest) -> None:
        if not request.collection_id:
            self._mark_failed(job_id, "任务没有绑定合集")
            return
        collection = self.collections.get_collection(request.collection_id)
        if collection is None:
            self._mark_failed(job_id, "合集不存在")
            return
        self._update_job(job_id, status=JobStatus.processing, progress=1, started_at=True)
        total = len(collection.video_files)

        def on_start(video_id: str, index: int, count: int) -> None:
            nonlocal total
            total = count
            progress = int(((index - 1) / max(count, 1)) * 95) + 2
            video = next((item for item in collection.video_files if item.id == video_id), None)
            self._update_job(job_id, progress=progress, current_file=video.filename if video else video_id)
            self._update_item(job_id, video_id, status="processing")

        def on_done(video_id: str, output_path: Path) -> None:
            self._update_item(job_id, video_id, status="completed", output_path=str(output_path), completed_at=True)
            self._bump_counts(job_id)

        def on_failed(video_id: str, message: str) -> None:
            self._update_item(job_id, video_id, status="failed", error_message=message, completed_at=True)
            self._bump_counts(job_id)

        try:
            extractor = Extractor(
                self.settings.output_directory,
                self.settings.ffmpeg_threads,
                self.settings.hardware_acceleration,
                self.settings.hardware_acceleration_device,
                self.settings.hardware_acceleration_fallback,
            )
            output_dir, files, failures = extractor.extract_collection(collection, request, on_start, on_done, on_failed)
            detail = self.get_job_detail(job_id)
            success_count = detail.success_count if detail else 0
            failure_count = detail.failure_count if detail else len(failures)
            status = JobStatus.completed if failure_count == 0 else JobStatus.failed
            summary = {
                "output_files": files,
                "failures": failures,
                "warnings": extractor.warnings,
                "success_count": success_count,
                "failure_count": failure_count,
                "total_count": total,
                "hardware_acceleration": {
                    "requested": request.hardware_acceleration or self.settings.hardware_acceleration,
                    "resolved": resolve_hardware_acceleration(
                        request.hardware_acceleration or self.settings.hardware_acceleration
                    ),
                    "fallback_events": extractor.acceleration_events,
                    "note": "自动模式会选择当前 FFmpeg 推荐后端；不可用或失败时按配置回退 CPU。",
                },
            }
            self._update_job(
                job_id,
                status=status,
                progress=100,
                current_file=f"完成: 成功 {success_count}，失败 {failure_count}",
                output_path=str(output_dir),
                success_count=success_count,
                failure_count=failure_count,
                summary=json.dumps(summary, ensure_ascii=False),
                completed_at=True,
            )
            with self.db.connect() as conn:
                conn.execute(
                    """
                    UPDATE collections
                    SET status = ?, output_path = ?, updated_at = CURRENT_TIMESTAMP
                    WHERE id = ?
                    """,
                    ("completed" if failure_count == 0 else "error", str(output_dir), collection.id),
                )
        except Exception as exc:
            self._mark_failed(job_id, str(exc))

    def list_jobs(self) -> list[ExtractJob]:
        with self.db.connect() as conn:
            rows = conn.execute("SELECT * FROM extract_jobs ORDER BY created_at DESC").fetchall()
        return [self._job_from_row(row) for row in rows]

    def get_job(self, job_id: str) -> ExtractJob | None:
        with self.db.connect() as conn:
            row = conn.execute("SELECT * FROM extract_jobs WHERE id = ?", (job_id,)).fetchone()
        return self._job_from_row(row) if row else None

    def get_job_detail(self, job_id: str) -> ExtractJobDetail | None:
        job = self.get_job(job_id)
        if not job:
            return None
        with self.db.connect() as conn:
            rows = conn.execute(
                "SELECT * FROM extract_job_items WHERE job_id = ? ORDER BY created_at, title",
                (job_id,),
            ).fetchall()
        return ExtractJobDetail(**job.model_dump(), items=[self._item_from_row(row) for row in rows])

    def cancel_job(self, job_id: str) -> ExtractJob | None:
        self._update_job(job_id, status=JobStatus.cancelled, completed_at=True)
        return self.get_job(job_id)

    def delete_job(self, job_id: str) -> bool:
        with self.db.connect() as conn:
            cursor = conn.execute("DELETE FROM extract_jobs WHERE id = ?", (job_id,))
            return cursor.rowcount > 0

    def _mark_failed(self, job_id: str, message: str) -> None:
        with self.db.connect() as conn:
            conn.execute(
                """
                UPDATE extract_job_items
                SET status = 'failed', error_message = ?, completed_at = CURRENT_TIMESTAMP
                WHERE job_id = ? AND status IN ('pending', 'processing')
                """,
                (message, job_id),
            )
            row = conn.execute(
                """
                SELECT
                    COUNT(*) AS total_count,
                    SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) AS success_count,
                    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failure_count
                FROM extract_job_items WHERE job_id = ?
                """,
                (job_id,),
            ).fetchone()
        total_count = row["total_count"] or 0
        success_count = row["success_count"] or 0
        failure_count = row["failure_count"] or 0
        self._update_job(
            job_id,
            status=JobStatus.failed,
            progress=100,
            total_count=total_count,
            success_count=success_count,
            failure_count=failure_count,
            error_message=message,
            summary=json.dumps(
                {
                    "failures": [{"error": message}],
                    "success_count": success_count,
                    "failure_count": failure_count,
                    "total_count": total_count,
                },
                ensure_ascii=False,
            ),
            completed_at=True,
        )

    def _bump_counts(self, job_id: str) -> None:
        with self.db.connect() as conn:
            row = conn.execute(
                """
                SELECT
                    SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) AS success_count,
                    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) AS failure_count
                FROM extract_job_items WHERE job_id = ?
                """,
                (job_id,),
            ).fetchone()
            success_count = row["success_count"] or 0
            failure_count = row["failure_count"] or 0
            total = success_count + failure_count
            job = conn.execute("SELECT total_count FROM extract_jobs WHERE id = ?", (job_id,)).fetchone()
            total_count = job["total_count"] or total
            progress = min(99, int((total / max(total_count, 1)) * 95) + 2)
            conn.execute(
                "UPDATE extract_jobs SET success_count = ?, failure_count = ?, progress = ? WHERE id = ?",
                (success_count, failure_count, progress, job_id),
            )

    def _update_item(self, job_id: str, video_id: str, **fields: object) -> None:
        values = []
        assignments = []
        for key, value in fields.items():
            if key == "completed_at" and value is True:
                assignments.append("completed_at = CURRENT_TIMESTAMP")
            else:
                assignments.append(f"{key} = ?")
                values.append(value)
        values.extend([job_id, video_id])
        with self.db.connect() as conn:
            conn.execute(
                f"UPDATE extract_job_items SET {', '.join(assignments)} WHERE job_id = ? AND video_file_id = ?",
                values,
            )

    def _update_job(self, job_id: str, **fields: object) -> None:
        values = []
        assignments = []
        for key, value in fields.items():
            if key == "started_at" and value is True:
                assignments.append("started_at = CURRENT_TIMESTAMP")
            elif key == "completed_at" and value is True:
                assignments.append("completed_at = CURRENT_TIMESTAMP")
            elif key == "status" and isinstance(value, JobStatus):
                assignments.append("status = ?")
                values.append(value.value)
            else:
                assignments.append(f"{key} = ?")
                values.append(value)
        if not assignments:
            return
        values.append(job_id)
        with self.db.connect() as conn:
            conn.execute(f"UPDATE extract_jobs SET {', '.join(assignments)} WHERE id = ?", values)

    def _job_from_row(self, row) -> ExtractJob:
        return ExtractJob(
            id=row["id"],
            collection_id=row["collection_id"],
            name=row["name"] or "",
            source_path=row["source_path"] or "",
            status=row["status"],
            progress=row["progress"] or 0,
            current_file=row["current_file"],
            selected_track_index=row["selected_track_index"] or 0,
            output_format=row["output_format"] or "mp3",
            quality_setting=row["quality_setting"] or "standard",
            trim_start_seconds=row["trim_start_seconds"] or 0,
            trim_end_seconds=row["trim_end_seconds"] or 0,
            total_count=row["total_count"] or 0,
            success_count=row["success_count"] or 0,
            failure_count=row["failure_count"] or 0,
            error_message=row["error_message"],
            output_path=row["output_path"],
            summary=_json_or_empty(row["summary"]),
            created_at=_parse_dt(row["created_at"]),
            started_at=_parse_dt(row["started_at"]),
            completed_at=_parse_dt(row["completed_at"]),
        )

    def _item_from_row(self, row) -> ExtractJobItem:
        return ExtractJobItem(
            id=row["id"],
            job_id=row["job_id"],
            video_file_id=row["video_file_id"],
            source_path=row["source_path"],
            output_path=row["output_path"],
            title=row["title"] or "",
            status=row["status"] or "pending",
            error_message=row["error_message"],
            created_at=_parse_dt(row["created_at"]),
            completed_at=_parse_dt(row["completed_at"]),
        )


def _json_or_empty(value: str | None) -> dict:
    if not value:
        return {}
    try:
        return json.loads(value)
    except json.JSONDecodeError:
        return {}


def _parse_dt(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value)
    except ValueError:
        return None
