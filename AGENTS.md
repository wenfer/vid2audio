# AGENTS.md

## Project

Vid2Audio is a NAS-friendly Docker application for turning video collections into audio packages for children's story players. It scans folders or individual video files, discovers audio tracks with `ffprobe`, extracts selected tracks with `ffmpeg`, generates ordered filenames with zero padding, and exposes a Rust/Axum-backed web UI.

## Repository Layout

- `backend/`: Rust crate for the Axum API and background extraction tasks.
  - `src/lib.rs`: module exports plus `build_router`, shared by the server binary and the desktop shell.
  - `src/main.rs`: server process entrypoint and TCP bind configuration.
  - `src/api.rs`: REST routes, static UI mount, and audio streaming.
  - `src/db.rs`: SQLite schema, migrations, and persistence.
  - `src/models.rs`: Serde request/response models.
  - `src/scanner.rs`: video discovery, grouping, and filtering.
  - `src/media.rs`: ffprobe parsing.
  - `src/extractor.rs`: FFmpeg extraction, preview, and TTS.
  - `src/platform.rs`: all cross-platform differences (paths, command lookup, filename rules).
  - `src/sorter.rs`: story-player-safe filename ordering.
  - `static/`: generated Vue output, not committed.
- `bridge/`: `vid2audio-bridge`, the WebView-to-Axum forwarding layer. Deliberately free of any `tauri` dependency so its tests run on machines without GTK/WebView2.
- `src-tauri/`: `vid2audio-desktop`, the Tauri v2 desktop shell.
  - `src/lib.rs`: runtime setup, custom URI scheme registration, window creation.
  - `capabilities/default.json`: the entire IPC allowlist — dialogs and reveal-in-folder, nothing else.
  - `scripts/fetch_ffmpeg.py`: downloads the bundled LGPL FFmpeg into `binaries/`.
  - `scripts/make_icons.py`: regenerates `icons/` using only the standard library.
- `frontend/`: Vue 3 + Vite + TypeScript source code.
  - `src/api/`: API client layer.
  - `src/components/`: reusable Vue components.
  - `src/composables/`: Vue composables (shared state/logic).
  - `src/desktop.ts`: every browser-vs-desktop difference (native dialogs, save-as, reveal).
  - `src/views/`: page-level view components.
  - `src/types/`: TypeScript type definitions.
  - `src/styles/`: global CSS (design tokens).
- Rust unit and API tests live beside their modules under `backend/src/`.
- `docker/`: Dockerfile and compose files.
  - `Dockerfile`: multi-stage Rust/Vue build with Debian FFmpeg bundled.
  - `docker-compose.yml`: default self-contained deployment.
- `.github/workflows/desktop-windows.yml`: the only release pipeline — Windows NSIS installer (`vid2audio-<version>-windows-x64-setup.exe`), on `v*.*.*` tags and manual dispatch.
- Docker images are not published by CI anymore: `.github/workflows/docker-ghcr.yml` was deleted. Do not re-add it with `on: []` — GitHub treats an empty trigger list as an invalid (but "active") workflow and creates a failing run on every push; delete the file instead.
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

Desktop build (Windows first; macOS and Linux planned):

```bash
cd src-tauri
python3 scripts/fetch_ffmpeg.py     # one-time: bundled LGPL FFmpeg into binaries/
npm --prefix ../frontend run build  # also run automatically by beforeBuildCommand
cargo tauri build                   # NSIS installer under target/release/bundle/
```

Requires the Tauri CLI (`cargo install tauri-cli --version '^2'`) plus the MSVC
toolchain and WebView2 runtime on Windows. Tauri cannot bundle a Windows
installer from Linux except through the NSIS cross-compilation path its own docs
call a last resort, so `.github/workflows/desktop-windows.yml` builds the
installer on a `windows-latest` runner instead — push a `v*.*.*` tag (must equal
`v` + the `"version"` in `src-tauri/tauri.conf.json`) or dispatch it manually,
then download the `vid2audio-windows-nsis` artifact. Released assets are renamed
to `vid2audio-<version>-windows-x64-setup.exe`.

`beforeBuildCommand` pins its `cwd` explicitly and must keep doing so. The CLI
runs the hook in `resolve_frontend_dir()`, which looks for a `package.json` and
falls back to **the parent of `src-tauri`** when it finds none — this repo has no
root `package.json`, so a relative `--prefix` in the script resolves differently
depending on where the build was invoked from. An explicit `cwd` is resolved
against the CLI's own working directory, which `build.rs` has already set to the
tauri dir, so `../frontend` is stable no matter where the user ran the command.

## Desktop Architecture

The desktop shell does **not** start a TCP server. It registers a `v2a://` custom
URI scheme and forwards each WebView request into the same Axum router the server
binary uses, via `Router::oneshot`. Consequences worth knowing before changing it:

- The frontend needs zero changes. Its requests are relative (`/api/v1/...`) and the
  page itself is served by the same router (`/` and `/static/*`), so `fetch`,
  `<audio src>`, and download links all resolve to the custom scheme.
- A localhost server was rejected on security grounds: `/api/v1/files/delete` and
  friends can modify arbitrary paths, and any web page's JavaScript can reach a
  listening port. A custom scheme is reachable only from this process's WebView.
- Rewriting the existing endpoints as `#[tauri::command]` was rejected because the
  URL-returning endpoints (audio streaming, preview, ZIP archive) cannot work over
  JSON-RPC IPC.
- The cost is that responses are fully buffered into `Vec<u8>`; the WebView custom
  protocol API has no streaming form. Range requests still work — status and headers
  pass through unchanged, which `bridge/` has tests for.
- Windows and Android map the scheme to `http://v2a.localhost/`, macOS and Linux to
  `v2a://localhost/`. `bridge::entry_url` handles both; do not hardcode either.
- The tokio runtime is deliberately leaked with `Box::leak`. Dropping it would
  silently cancel in-flight extraction jobs, and `Runtime::drop` panics inside an
  async context.

There are **no** `#[tauri::command]`s. Everything the UI needs already exists as an
HTTP route, so `capabilities/default.json` only opens the native dialogs
(`dialog:allow-open`, `allow-save`, `allow-confirm`) and `opener:allow-reveal-item-in-dir`.
Keep it that way unless something genuinely cannot be expressed as a route — a
smaller IPC allowlist is the main security benefit of this design.

Browser-vs-desktop differences live in `frontend/src/desktop.ts`, detected at runtime
via `'__TAURI_INTERNALS__' in window` (never the user agent — WebView2's UA is Edge's).
The Tauri npm packages are pulled in with dynamic `import()`, so they end up in
separate chunks that the Docker/browser deployment never fetches. Things that differ:

- `window.prompt` is **not implemented in WebView2** — it silently returns null. Text
  input goes through `usePrompt` + `PromptModal.vue` on both platforms instead.
- `window.confirm` works but renders as a page-origin dialog (`v2a.localhost 显示…`).
  Use `confirmAction`, which switches to the native dialog on the desktop.
- `<a download>` cannot save where the user wants in a WebView. The desktop path is
  a save-as dialog plus `POST /files/archive-to`, which writes the zip server-side
  and streams it to disk instead of buffering it.
- Path text fields get a 📂 button only on the desktop; a web page cannot resolve a
  real filesystem path.

## Development Notes

- Prefer small, focused changes that preserve the current lightweight stack: Axum, SQLite, static Vue UI, FFmpeg.
- Do not commit generated artifacts: `backend/target/`, `backend/static/`, `frontend/node_modules/`, `data/`, local databases, media output, or `src-tauri/binaries/` contents.
- `backend/` stays a standalone crate rather than a workspace member: `docker/Dockerfile` copies `backend/Cargo.toml` and `backend/Cargo.lock` on their own, and a workspace root would break that layer.
- Cross-platform behavior belongs in `backend/src/platform.rs`, not scattered `cfg!(windows)` checks. It covers path defaults, `~` expansion (Windows has no `HOME`), command lookup via `PATHEXT`, `CREATE_NO_WINDOW`, hidden-file attributes, drive-letter roots, extended-length path display, and the filename rules below.
- `platform::filesystem_roots` feeds the file browser's shortcut row. On Windows it is not a convenience: `Path::new("C:\\").parent()` is `None`, so without it a user cannot reach the drive their U-disk is on. It reads the `GetLogicalDrives` bitmask rather than probing `A:\`..`Z:\` — probing blocks for seconds on disconnected network drives.
- Every path handed to the frontend goes through `canonical`, which calls `platform::strip_extended_prefix`. Windows `canonicalize` returns `\\?\C:\...`; that prefix would show up in the UI and in ffmpeg argv. Paths near `MAX_PATH` keep the prefix, since that is the only thing making them usable.
- `platform::reject_windows_unsafe_name` runs on **every** platform, not just Windows. `:` is the reason: Windows `PathBuf::push` replaces the whole path when given a drive-relative prefix, so renaming a file to `C:evil` escapes the parent directory, and `a.mp3:hidden` writes an NTFS alternate data stream instead of renaming.
- The local machine may not have `ffmpeg` or `ffprobe`; code should fail with clear messages and continue where possible. The Docker image installs FFmpeg, and the desktop bundle ships its own via `platform::set_bundled_bin_dir`.
- File browsing is intentionally filesystem-based for NAS use. Keep filtering rules centralized around settings: video extension allowlist, ignored extensions, and minimum file size.
- Audio track `index` maps to the FFmpeg stream index, so extraction and preview should use `-map 0:{index}`.
- Story-player ordering is a core feature. Preserve zero-padded filenames and keep `sorter.rs` tests passing.
- `/files/fat-sort` rewrites a folder's directory entries in natural order by renaming them through a sibling `.vid2audio-fatsort.tmp` directory. It only changes entry order (visible on FAT/exFAT), never filenames or content, and rolls back on any failure — keep that invariant if you touch `reorder_directory_fat`.
- Deletes in `db.rs` remove child rows explicitly instead of relying on `ON DELETE CASCADE`. `CREATE TABLE IF NOT EXISTS` never upgrades an existing table, so databases created by older builds can lack the cascade and fail with `FOREIGN KEY constraint failed` (SQLite code 787). `clear_dangling_references` sweeps leftover orphans at startup; keep its statement order (detach references before deleting the rows they point at).
- Extraction jobs only live in process memory. `recover_interrupted_jobs` runs at startup and moves any job still marked `queued`/`processing` to `paused` (and its in-flight items back to `pending`), because after a restart nothing will advance them and the API refuses to delete a running job. Keep that sweep if you add job states.
- Pause is cooperative: `pause_job` only flips the status, and the worker exits at the next file boundary, so the file being extracted finishes first. `resume_job` replays the `ExtractRequest` persisted in `extract_jobs.request` and skips items already marked `completed`, which is what keeps the zero-padded numbering stable across a resume — position comes from the full ordered list, not from the remaining work.
- Job workers carry an epoch from `begin_run`. A worker that finds its epoch stale exits silently; that is what stops a resume issued during the pause window from running two extractions over the same files.
- Analysis results are just `collections` rows, which is why the workspace "分析历史" panel is a plain `GET /collections`. Modals holding analysis results or wizard input do not close on backdrop click — a misclick used to discard work with no way back.

## API Surface

Base URL: `/api/v1`

- `GET /files?path=...`
- `POST /files/fat-sort`
- `POST /files/archive-to`
- `POST /scan/start`
- `GET /collections`
- `GET /collections/{id}`
- `POST /extract`
- `GET /extract/jobs`
- `GET /extract/jobs/{id}`
- `DELETE /extract/jobs/{id}`
- `POST /extract/jobs/{id}/cancel`
- `POST /extract/jobs/{id}/pause`
- `POST /extract/jobs/{id}/resume`
- `GET /preview/{video_id}?track=...&duration=...&start=...`
- `GET /settings`
- `PUT /settings`
- `GET /system/status`

## Releases (GitHub Actions)

The only CI pipeline is `.github/workflows/desktop-windows.yml` — it builds the
Windows NSIS installer on a `windows-latest` runner and publishes it as a GitHub
Release. Docker images are no longer published by CI (the old
`.github/workflows/docker-ghcr.yml` was deleted; do not bring it back with
`on: []` — an empty trigger list is parsed as an invalid active workflow that
fails on every push). Local `docker compose -f docker/docker-compose.yml up --build`
still works for development.

Release rules:

- Triggered by a `v*.*.*` tag, or by manual `workflow_dispatch` (which requires a
  `version` input and offers a `prerelease` flag).
- The version's single source of truth is the `"version"` in `src-tauri/tauri.conf.json`.
  The tag must be exactly `v{version}`, and a manually entered version must match
  too — the workflow fails otherwise, so a mislabeled package cannot be published.
- Every bash-style step sets `shell: bash` explicitly; the Windows runner's default
  shell is pwsh and would choke on `[[ ]]` / `$GITHUB_OUTPUT` scripts.
- `fetch_ffmpeg.py` runs with `working-directory: src-tauri` (it downloads into
  `cwd/binaries/`, and `tauri.conf.json` bundles `binaries/` relative to `src-tauri`).
- Installer naming: `vid2audio-<version>-windows-x64-setup.exe`, renamed from
  Tauri's default `Vid2Audio_<version>_x64-setup.exe`; the release asset and the
  `vid2audio-windows-nsis` artifact share the same name.
- Publishing a version whose tag already exists fails on purpose — no duplicate releases.

## Before Finishing a Change

Run at least:

```bash
cargo fmt --manifest-path backend/Cargo.toml -- --check
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
```

If `bridge/` or `src-tauri/` changed, also run:

```bash
cargo test --manifest-path bridge/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --locked -- -D warnings
```

On Linux without GTK/WebView2 development libraries, `src-tauri` cannot be compiled
for the host. Type-check it against Windows instead — the priority target, and the
one that needs no GTK. Use the **gnu** target, not msvc: the tree pulls in
`libsqlite3-sys`, and cross-compiling `sqlite3.c` needs the MSVC C headers, which a
Linux box does not have.

```bash
rustup target add x86_64-pc-windows-gnu
cargo clippy --manifest-path backend/Cargo.toml --locked --target x86_64-pc-windows-gnu -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-gnu -- -D warnings
```

The backend check matters as much as the shell's: `platform.rs` has real
`#[cfg(windows)]` bodies (the `GetLogicalDrives` FFI behind `filesystem_roots`) that
the host build never compiles.

If frontend files changed, rebuild:

```bash
cd frontend && npm run build
```

If UI files changed and the local server is running, refresh `http://127.0.0.1:8000` and verify the affected flow manually.
