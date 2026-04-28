from pathlib import Path
import uuid

from backend.app.models.database import Database
from backend.app.models.schemas import AppSettings
from backend.app.services.collection_service import CollectionService


def test_rescan_keeps_collection_when_jobs_reference_it(tmp_path: Path):
    source = tmp_path / "Season 1"
    source.mkdir()
    video = source / "S01E01.mp4"
    video.write_bytes(b"not a real video")

    db = Database(tmp_path / "vid2audio.db")
    service = CollectionService(
        db,
        AppSettings(
            video_extensions=[".mp4"],
            min_file_size_mb=0,
            scan_directories=[str(source)],
        ),
    )

    first = service.scan([str(source)])
    collection = first.collections[0]
    video_id = collection.video_files[0].id
    job_id = str(uuid.uuid4())
    item_id = str(uuid.uuid4())

    with db.connect() as conn:
        conn.execute(
            """
            INSERT INTO extract_jobs(id, collection_id, name, source_path, status)
            VALUES (?, ?, 'old job', ?, 'completed')
            """,
            (job_id, collection.id, collection.source_path),
        )
        conn.execute(
            """
            INSERT INTO extract_job_items(id, job_id, video_file_id, source_path, title)
            VALUES (?, ?, ?, ?, 'old item')
            """,
            (item_id, job_id, video_id, str(video)),
        )

    second = service.scan([str(source)])

    assert second.collections[0].id == collection.id
    with db.connect() as conn:
        job = conn.execute("SELECT collection_id FROM extract_jobs WHERE id = ?", (job_id,)).fetchone()
        item = conn.execute("SELECT video_file_id FROM extract_job_items WHERE id = ?", (item_id,)).fetchone()
        videos = conn.execute("SELECT COUNT(*) AS count FROM video_files WHERE collection_id = ?", (collection.id,)).fetchone()

    assert job["collection_id"] == collection.id
    assert item["video_file_id"] is None
    assert videos["count"] == 1


def test_delete_collection_detaches_historical_jobs(tmp_path: Path):
    source = tmp_path / "Season 2"
    source.mkdir()
    video = source / "S02E01.mp4"
    video.write_bytes(b"not a real video")

    db = Database(tmp_path / "vid2audio.db")
    service = CollectionService(
        db,
        AppSettings(
            video_extensions=[".mp4"],
            min_file_size_mb=0,
            scan_directories=[str(source)],
        ),
    )

    collection = service.scan([str(source)]).collections[0]
    video_id = collection.video_files[0].id
    job_id = str(uuid.uuid4())
    item_id = str(uuid.uuid4())

    with db.connect() as conn:
        conn.execute(
            "INSERT INTO extract_jobs(id, collection_id, name, source_path, status) VALUES (?, ?, 'job', ?, 'completed')",
            (job_id, collection.id, collection.source_path),
        )
        conn.execute(
            "INSERT INTO extract_job_items(id, job_id, video_file_id, source_path, title) VALUES (?, ?, ?, ?, 'item')",
            (item_id, job_id, video_id, str(video)),
        )

    assert service.delete_collection(collection.id) is True

    with db.connect() as conn:
        job = conn.execute("SELECT collection_id FROM extract_jobs WHERE id = ?", (job_id,)).fetchone()
        item = conn.execute("SELECT video_file_id FROM extract_job_items WHERE id = ?", (item_id,)).fetchone()

    assert job["collection_id"] is None
    assert item["video_file_id"] is None
