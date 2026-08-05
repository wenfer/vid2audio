# AGENTS.md

## Project

Vid2Audio is a NAS-friendly Docker application for turning video collections into audio packages for children's story players. It scans folders or individual video files, discovers audio tracks with `ffprobe`, extracts selected tracks with `ffmpeg`, generates ordered filenames with zero padding, and exposes a Rust/Axum-backed web UI.

## Repository Layout

- `backend/`: Rust crate for the Axum API and background extraction tasks.
  - `src/main.rs`: process entrypoint and server configuration.
  - `src/api.rs`: REST routes, static UI mount, and audio streaming.
  - `src/db.rs`: SQLite schema, migrations, and persistence.
  - `src/models.rs`: Serde request/response models.
  - `src/scanner.rs`: video discovery, grouping, and filtering.
  - `src/media.rs`: ffprobe parsing.
  - `src/extractor.rs`: FFmpeg extraction, preview, and TTS.
  - `src/sorter.rs`: story-player-safe filename ordering.
  - `static/`: generated Vue output, not committed.
- `frontend/`: Vue 3 + Vite + TypeScript source code.
  - `src/api/`: API client layer.
  - `src/components/`: reusable Vue components.
  - `src/composables/`: Vue composables (shared state/logic).
  - `src/views/`: page-level view components.
  - `src/types/`: TypeScript type definitions.
  - `src/styles/`: global CSS (design tokens).
- Rust unit and API tests live beside their modules under `backend/src/`.
- `docker/`: Dockerfile and compose files.
  - `Dockerfile`: multi-stage Rust/Vue build with Debian FFmpeg bundled.
  - `docker-compose.yml`: default self-contained deployment.
- `.github/workflows/docker-ghcr.yml`: multi-arch GHCR image publishing.
- `docs/PRD-vid2audio.md`: product requirements and design reference.

## Common Commands

Build and test the backend:

```bash
cargo build --manifest-path backend/Cargo.toml
cargo test --manifest-path backend/Cargo.toml --locked
```

Frontend development:

```bash
cd frontend
npm install --registry https://registry.npmmirror.com
npm run dev    # Dev server with HMR at http://localhost:5173
npm run build  # Build to backend/static/
```

Compile check:

```bash
cargo check --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
```

Run locally:

```bash
VID2AUDIO_DB=data/vid2audio.db \
VID2AUDIO_INPUT=/path/to/videos \
VID2AUDIO_OUTPUT=/path/to/output \
cargo run --manifest-path backend/Cargo.toml
```

Run with Docker:

```bash
docker compose -f docker/docker-compose.yml up --build
```

## Development Notes

- Prefer small, focused changes that preserve the current lightweight stack: Axum, SQLite, static Vue UI, FFmpeg.
- Do not commit generated artifacts: `backend/target/`, `backend/static/`, `frontend/node_modules/`, `data/`, local databases, or media output.
- The local machine may not have `ffmpeg` or `ffprobe`; code should fail with clear messages and continue where possible. The Docker image installs FFmpeg.
- File browsing is intentionally filesystem-based for NAS use. Keep filtering rules centralized around settings: video extension allowlist, ignored extensions, and minimum file size.
- Audio track `index` maps to the FFmpeg stream index, so extraction and preview should use `-map 0:{index}`.
- Story-player ordering is a core feature. Preserve zero-padded filenames and keep `sorter.rs` tests passing.

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

## Docker and Publishing

GitHub Actions builds and publishes multi-arch images to:

```text
ghcr.io/wenfer/vid2audio:vX.Y.Z
ghcr.io/wenfer/vid2audio:latest
```

Supported platforms:

- `linux/amd64`
- `linux/arm64`

Workflow triggers:

- Tags matching `v*.*.*`; the tag must match `backend/Cargo.toml`
- Pull requests to `main` build only, without pushing
- Manual `workflow_dispatch`

GHCR authentication:

- The workflow uses `GITHUB_TOKEN` by default and requests `packages: write`.
- If GHCR rejects pushes with `permission_denied: write_package`, set repository Actions workflow permissions to read/write.
- For org or package permission issues, add `GHCR_TOKEN` with `write:packages` and `read:packages`; optionally add `GHCR_USERNAME` when the PAT owner differs from the repository owner.
- The default Dockerfile bundles Debian FFmpeg and ffprobe.
- The runtime image contains the stripped Rust binary, Vue output, and Debian FFmpeg. Versioned and `latest` tags point to the same amd64/arm64 manifest.

## Before Finishing a Change

Run at least:

```bash
cargo fmt --manifest-path backend/Cargo.toml -- --check
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
```

If frontend files changed, rebuild:

```bash
cd frontend && npm run build
```

If UI files changed and the local server is running, refresh `http://127.0.0.1:8000` and verify the affected flow manually.
