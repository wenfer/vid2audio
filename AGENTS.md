# AGENTS.md

## Project

Vid2Audio is a NAS-friendly Docker application for turning video collections into audio packages for children's story players. It scans folders or individual video files, discovers audio tracks with `ffprobe`, extracts selected tracks with `ffmpeg`, generates ordered filenames with zero padding, and exposes a simple FastAPI-backed web UI.

## Repository Layout

- `backend/app/main.py`: FastAPI application entrypoint and static UI mount.
- `backend/app/api/`: REST API routers.
  - `files.py`: file browser API for folders and selectable video files.
  - `scan.py`: scan/analyze selected files or folders.
  - `extract.py`: extraction jobs and preview audio.
  - `collections.py`: analyzed collections.
  - `settings.py`: persisted global settings.
  - `system.py`: FFmpeg and hardware acceleration status.
- `backend/app/core/`: media and filename logic.
  - `scanner.py`: video discovery, grouping, filtering.
  - `media.py`: ffprobe parsing and hardware acceleration detection.
  - `extractor.py`: ffmpeg extraction, preview, trim offsets, fallback behavior.
  - `sorter.py`: title cleanup and story-player-safe filename ordering.
  - `tts_engine.py`: Edge-TTS intro generation with silent fallback.
- `backend/app/models/`: Pydantic schemas and SQLite schema/migrations.
- `backend/app/services/`: persistence-backed business services.
- `backend/app/static/`: plain HTML/CSS/JS web UI.
- `backend/tests/`: pytest coverage for sorting, scanning, and acceleration helpers.
- `docker/`: Dockerfile and compose files.
  - `Dockerfile`: multi-stage build, no FFmpeg bundled (user mounts host binaries).
  - `Dockerfile.ffmpeg-bundled`: alternative Dockerfile that installs FFmpeg via apt.
  - `docker-compose.yml`: portable default compose with host ffmpeg mount.
  - `docker-compose.ffmpeg-bundled.yml`: override to use the bundled-FFmpeg image.
  - `docker-compose.intel-vaapi.yml`: Intel iGPU / VAAPI / QSV override.
  - `docker-compose.nvidia.yml`: NVIDIA Container Toolkit override.
  - `docker-compose.rockchip.yml`: Rockchip MPP/RGA device override for ARM NAS systems.
- `.github/workflows/docker-ghcr.yml`: multi-arch GHCR image publishing.
- `docs/PRD-vid2audio.md`: product requirements and design reference.

## Common Commands

Create local environment:

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install -r requirements.txt
```

Run tests:

```bash
.venv/bin/python -m pytest backend/tests
```

Compile check:

```bash
.venv/bin/python -m compileall backend
```

Run locally:

```bash
VID2AUDIO_DB=data/vid2audio.db \
VID2AUDIO_INPUT=/path/to/videos \
VID2AUDIO_OUTPUT=/path/to/output \
.venv/bin/uvicorn backend.app.main:app --host 127.0.0.1 --port 8000
```

Run with Docker:

```bash
docker compose -f docker/docker-compose.yml up --build
```

Run with Intel iGPU / VAAPI / QSV devices exposed:

```bash
docker compose -f docker/docker-compose.yml -f docker/docker-compose.intel-vaapi.yml up --build
```

Run with NVIDIA GPU devices exposed:

```bash
docker compose -f docker/docker-compose.yml -f docker/docker-compose.nvidia.yml up --build
```

Run with Rockchip MPP/RGA devices exposed:

```bash
docker compose -f docker/docker-compose.yml -f docker/docker-compose.rockchip.yml up --build
```

## Development Notes

- Prefer small, focused changes that preserve the current lightweight stack: FastAPI, SQLite, plain static UI, FFmpeg.
- Do not commit generated artifacts: `.venv/`, `data/`, `__pycache__/`, `.pytest_cache/`, local databases, or media output.
- The local machine may not have `ffmpeg` or `ffprobe`; code should fail with clear messages and continue where possible. The Docker image installs FFmpeg.
- File browsing is intentionally filesystem-based for NAS use. Keep filtering rules centralized around settings: video extension allowlist, ignored extensions, and minimum file size.
- Audio track `index` maps to the FFmpeg stream index, so extraction and preview should use `-map 0:{index}`.
- Hardware acceleration must be conservative:
  - Default to `auto`.
  - Resolve `auto` through `ffmpeg -hwaccels`.
  - Detect Rockchip `rkmpp` through `ffmpeg -decoders`, because it is codec-based rather than a generic `-hwaccel` flag.
  - Prefer normal operation over speed.
  - Always keep CPU fallback unless a future setting explicitly disables it.
  - Record fallback events in job summaries.
  - Keep the base compose portable; put hardware device mounts in override files.
- Story-player ordering is a core feature. Preserve zero-padded filenames and keep tests around `sorter.py` passing.

## API Surface

Base URL: `/api/v1`

- `GET /files?path=...`
- `POST /scan/start`
- `GET /collections`
- `GET /collections/{id}`
- `POST /extract`
- `GET /extract/jobs`
- `GET /extract/jobs/{id}`
- `GET /preview/{video_id}?track=...&duration=...&start=...`
- `GET /settings`
- `PUT /settings`
- `GET /system/status`
- `GET /system/hardware-acceleration`

## Docker and Publishing

GitHub Actions builds and publishes multi-arch images to:

```text
ghcr.io/wenfer/vid2audio:<commit-id>
ghcr.io/wenfer/vid2audio:latest
```

Supported platforms:

- `linux/amd64`
- `linux/arm64`

Workflow triggers:

- Push to `main`
- Tags matching `v*.*.*`
- Pull requests to `main` build only, without pushing
- Manual `workflow_dispatch`

GHCR authentication:

- The workflow uses `GITHUB_TOKEN` by default and requests `packages: write`.
- If GHCR rejects pushes with `permission_denied: write_package`, set repository Actions workflow permissions to read/write.
- For org or package permission issues, add `GHCR_TOKEN` with `write:packages` and `read:packages`; optionally add `GHCR_USERNAME` when the PAT owner differs from the repository owner.
- The container installs the project with `pip install /app`, so `backend` must be importable from site-packages without relying on `/app`, `PYTHONPATH`, or the current working directory.
- The default Dockerfile does NOT bundle FFmpeg; it expects the user to mount host `ffmpeg`/`ffprobe` binaries via volumes. `Dockerfile.ffmpeg-bundled` is provided for users without host FFmpeg.
- The Dockerfile intentionally imports `backend.app.main` from `/tmp` during image build and verifies `backend.app/static/index.html` is packaged. If this fails, fix packaging before publishing.

## Before Finishing a Change

Run at least:

```bash
.venv/bin/python -m compileall backend
.venv/bin/python -m pytest backend/tests
```

If UI files changed and the local server is running, refresh `http://127.0.0.1:8000` and verify the affected flow manually.
