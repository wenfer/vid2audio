from __future__ import annotations

import json
import sqlite3
from contextlib import contextmanager
from pathlib import Path
from typing import Iterator


SCHEMA = """
CREATE TABLE IF NOT EXISTS collections (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    source_path TEXT NOT NULL,
    output_path TEXT,
    episode_count INTEGER,
    status TEXT DEFAULT 'pending',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    settings TEXT DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS video_files (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    filename TEXT NOT NULL,
    filepath TEXT NOT NULL,
    file_size INTEGER,
    duration REAL,
    resolution TEXT,
    video_codec TEXT,
    episode_number INTEGER,
    episode_title TEXT,
    status TEXT DEFAULT 'pending',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (collection_id) REFERENCES collections(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS audio_tracks (
    id TEXT PRIMARY KEY,
    video_file_id TEXT NOT NULL,
    track_index INTEGER NOT NULL,
    codec TEXT,
    language TEXT,
    language_full TEXT,
    channels INTEGER,
    sample_rate INTEGER,
    bitrate INTEGER,
    title TEXT,
    is_default INTEGER DEFAULT 0,
    FOREIGN KEY (video_file_id) REFERENCES video_files(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS extract_jobs (
    id TEXT PRIMARY KEY,
    collection_id TEXT,
    name TEXT,
    source_path TEXT,
    status TEXT DEFAULT 'queued',
    progress INTEGER DEFAULT 0,
    current_file TEXT,
    selected_track_index INTEGER,
    output_format TEXT,
    quality_setting TEXT,
    trim_start_seconds REAL DEFAULT 0,
    trim_end_seconds REAL DEFAULT 0,
    total_count INTEGER DEFAULT 0,
    success_count INTEGER DEFAULT 0,
    failure_count INTEGER DEFAULT 0,
    error_message TEXT,
    output_path TEXT,
    summary TEXT DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    FOREIGN KEY (collection_id) REFERENCES collections(id)
);

CREATE TABLE IF NOT EXISTS extract_job_items (
    id TEXT PRIMARY KEY,
    job_id TEXT NOT NULL,
    video_file_id TEXT,
    source_path TEXT NOT NULL,
    output_path TEXT,
    title TEXT,
    status TEXT DEFAULT 'pending',
    error_message TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    completed_at TIMESTAMP,
    FOREIGN KEY (job_id) REFERENCES extract_jobs(id) ON DELETE CASCADE,
    FOREIGN KEY (video_file_id) REFERENCES video_files(id)
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
"""


class Database:
    def __init__(self, path: str | Path) -> None:
        self.path = Path(path)
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.initialize()

    @contextmanager
    def connect(self) -> Iterator[sqlite3.Connection]:
        conn = sqlite3.connect(self.path)
        conn.row_factory = sqlite3.Row
        conn.execute("PRAGMA foreign_keys = ON")
        try:
            yield conn
            conn.commit()
        finally:
            conn.close()

    def initialize(self) -> None:
        with self.connect() as conn:
            conn.executescript(SCHEMA)
            self._migrate(conn)

    def _migrate(self, conn: sqlite3.Connection) -> None:
        existing = {row["name"] for row in conn.execute("PRAGMA table_info(extract_jobs)").fetchall()}
        columns = {
            "name": "TEXT",
            "source_path": "TEXT",
            "trim_start_seconds": "REAL DEFAULT 0",
            "trim_end_seconds": "REAL DEFAULT 0",
            "total_count": "INTEGER DEFAULT 0",
            "success_count": "INTEGER DEFAULT 0",
            "failure_count": "INTEGER DEFAULT 0",
            "summary": "TEXT DEFAULT '{}'",
        }
        for column, definition in columns.items():
            if column not in existing:
                conn.execute(f"ALTER TABLE extract_jobs ADD COLUMN {column} {definition}")

    def get_settings(self) -> dict[str, object]:
        with self.connect() as conn:
            rows = conn.execute("SELECT key, value FROM settings").fetchall()
        values: dict[str, object] = {}
        for row in rows:
            try:
                values[row["key"]] = json.loads(row["value"])
            except json.JSONDecodeError:
                values[row["key"]] = row["value"]
        return values

    def update_settings(self, values: dict[str, object]) -> None:
        with self.connect() as conn:
            for key, value in values.items():
                conn.execute(
                    """
                    INSERT INTO settings(key, value, updated_at)
                    VALUES (?, ?, CURRENT_TIMESTAMP)
                    ON CONFLICT(key) DO UPDATE SET
                        value = excluded.value,
                        updated_at = CURRENT_TIMESTAMP
                    """,
                    (key, json.dumps(value, ensure_ascii=False)),
                )
