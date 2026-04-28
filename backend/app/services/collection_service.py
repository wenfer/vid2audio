from __future__ import annotations

import json
import uuid
from datetime import datetime

from backend.app.core.scanner import Scanner
from backend.app.models.database import Database
from backend.app.models.schemas import AppSettings, AudioTrack, Collection, ScanResult, VideoFile


class CollectionService:
    def __init__(self, db: Database, settings: AppSettings) -> None:
        self.db = db
        self.settings = settings

    def scan(self, directories: list[str] | None = None) -> ScanResult:
        scanner = Scanner(
            self.settings.video_extensions,
            self.settings.min_file_size_mb,
            self.settings.ignored_extensions,
            self.settings.filesystem_sorting,
        )
        collections, warnings = scanner.scan(directories or self.settings.scan_directories)
        with self.db.connect() as conn:
            for collection in collections:
                self._upsert_collection(conn, collection)
                for video in collection.video_files:
                    conn.execute(
                        """
                        INSERT INTO video_files(
                            id, collection_id, filename, filepath, file_size, duration, resolution,
                            video_codec, episode_number, episode_title, status
                        )
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                        """,
                        (
                            video.id,
                            collection.id,
                            video.filename,
                            video.filepath,
                            video.file_size,
                            video.duration,
                            video.resolution,
                            video.video_codec,
                            video.episode_number,
                            video.episode_title,
                            video.status,
                        ),
                    )
                    for track in video.audio_tracks:
                        conn.execute(
                            """
                            INSERT INTO audio_tracks(
                                id, video_file_id, track_index, codec, language, language_full,
                                channels, sample_rate, bitrate, title, is_default
                            )
                            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                            """,
                            (
                                track.id or str(uuid.uuid4()),
                                video.id,
                                track.index,
                                track.codec,
                                track.language,
                                track.language_full,
                                track.channels,
                                track.sample_rate,
                                track.bitrate,
                                track.title,
                                int(track.default),
                            ),
                        )
        return ScanResult(
            scan_id=str(uuid.uuid4()),
            collections=collections,
            files_found=sum(item.episode_count for item in collections),
            warnings=warnings,
        )

    def _upsert_collection(self, conn, collection: Collection) -> None:
        existing = conn.execute(
            "SELECT id, output_path FROM collections WHERE source_path = ?",
            (collection.source_path,),
        ).fetchone()
        settings = json.dumps(collection.settings, ensure_ascii=False)
        if existing:
            collection.id = existing["id"]
            collection.output_path = collection.output_path or existing["output_path"]
            for video in collection.video_files:
                video.collection_id = collection.id
            conn.execute(
                """
                UPDATE extract_job_items
                SET video_file_id = NULL
                WHERE video_file_id IN (
                    SELECT id FROM video_files WHERE collection_id = ?
                )
                """,
                (collection.id,),
            )
            conn.execute("DELETE FROM video_files WHERE collection_id = ?", (collection.id,))
            conn.execute(
                """
                UPDATE collections
                SET name = ?, output_path = ?, episode_count = ?, status = ?,
                    settings = ?, updated_at = CURRENT_TIMESTAMP
                WHERE id = ?
                """,
                (
                    collection.name,
                    collection.output_path,
                    collection.episode_count,
                    collection.status.value,
                    settings,
                    collection.id,
                ),
            )
            return

        for video in collection.video_files:
            video.collection_id = collection.id
        conn.execute(
            """
            INSERT INTO collections(id, name, source_path, output_path, episode_count, status, settings)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            """,
            (
                collection.id,
                collection.name,
                collection.source_path,
                collection.output_path,
                collection.episode_count,
                collection.status.value,
                settings,
            ),
        )

    def create_collection_from_source(self, source_path: str, job_name: str | None = None) -> tuple[Collection | None, list[str]]:
        result = self.scan([source_path])
        if not result.collections:
            return None, result.warnings
        collection = result.collections[0]
        if job_name:
            with self.db.connect() as conn:
                conn.execute(
                    "UPDATE collections SET name = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?",
                    (job_name, collection.id),
                )
            collection.name = job_name
        detail = self.get_collection(collection.id)
        return detail or collection, result.warnings

    def list_collections(self) -> list[Collection]:
        with self.db.connect() as conn:
            rows = conn.execute("SELECT * FROM collections ORDER BY updated_at DESC").fetchall()
        return [self._collection_from_row(row, include_videos=False) for row in rows]

    def get_collection(self, collection_id: str) -> Collection | None:
        with self.db.connect() as conn:
            row = conn.execute("SELECT * FROM collections WHERE id = ?", (collection_id,)).fetchone()
            if not row:
                return None
            video_rows = conn.execute(
                "SELECT * FROM video_files WHERE collection_id = ? ORDER BY episode_number, filename",
                (collection_id,),
            ).fetchall()
            track_rows = conn.execute(
                """
                SELECT audio_tracks.* FROM audio_tracks
                JOIN video_files ON video_files.id = audio_tracks.video_file_id
                WHERE video_files.collection_id = ?
                ORDER BY video_files.episode_number, audio_tracks.track_index
                """,
                (collection_id,),
            ).fetchall()
        tracks_by_video: dict[str, list[AudioTrack]] = {}
        for track in track_rows:
            tracks_by_video.setdefault(track["video_file_id"], []).append(
                AudioTrack(
                    id=track["id"],
                    video_file_id=track["video_file_id"],
                    index=track["track_index"],
                    codec=track["codec"] or "",
                    language=track["language"] or "und",
                    language_full=track["language_full"] or "未知语言",
                    channels=track["channels"],
                    sample_rate=track["sample_rate"],
                    bitrate=track["bitrate"],
                    title=track["title"] or "",
                    default=bool(track["is_default"]),
                )
            )
        collection = self._collection_from_row(row, include_videos=False)
        collection.video_files = [
            VideoFile(
                id=video["id"],
                collection_id=video["collection_id"],
                filename=video["filename"],
                filepath=video["filepath"],
                file_size=video["file_size"] or 0,
                duration=video["duration"],
                resolution=video["resolution"] or "",
                video_codec=video["video_codec"] or "",
                episode_number=video["episode_number"],
                episode_title=video["episode_title"],
                status=video["status"] or "pending",
                audio_tracks=tracks_by_video.get(video["id"], []),
            )
            for video in video_rows
        ]
        return collection

    def delete_collection(self, collection_id: str) -> bool:
        with self.db.connect() as conn:
            cursor = conn.execute("DELETE FROM collections WHERE id = ?", (collection_id,))
            return cursor.rowcount > 0

    def _collection_from_row(self, row, include_videos: bool) -> Collection:
        settings = json.loads(row["settings"] or "{}")
        return Collection(
            id=row["id"],
            name=row["name"],
            source_path=row["source_path"],
            output_path=row["output_path"],
            episode_count=row["episode_count"] or 0,
            status=row["status"],
            video_files=[] if not include_videos else [],
            created_at=_parse_dt(row["created_at"]),
            updated_at=_parse_dt(row["updated_at"]),
            settings=settings,
        )


def _parse_dt(value: str | None) -> datetime | None:
    if not value:
        return None
    try:
        return datetime.fromisoformat(value)
    except ValueError:
        return None
