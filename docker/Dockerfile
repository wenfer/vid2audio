# syntax=docker/dockerfile:1

FROM node:20-bookworm-slim AS frontend
WORKDIR /workspace
COPY frontend/package.json frontend/package-lock.json frontend/
RUN --mount=type=cache,target=/root/.npm npm ci --prefix frontend
COPY frontend frontend
RUN npm run --prefix frontend build

FROM rust:1-bookworm AS backend
WORKDIR /workspace/backend
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/src ./src
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/workspace/backend/target \
    cargo build --locked --release && cp target/release/vid2audio /tmp/vid2audio

FROM debian:bookworm-slim
WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg \
    && rm -rf /var/lib/apt/lists/* /usr/share/doc /usr/share/man /usr/share/info
RUN mkdir -p /app/input /app/output /app/data /app/bin /app/static
COPY --from=backend /tmp/vid2audio /usr/local/bin/vid2audio
COPY --from=frontend /workspace/backend/static /app/static

ENV VID2AUDIO_DB=/app/data/vid2audio.db \
    VID2AUDIO_INPUT=/app/input \
    VID2AUDIO_OUTPUT=/app/output \
    VID2AUDIO_STATIC=/app/static \
    VID2AUDIO_BIND=0.0.0.0:8000 \
    RUST_LOG=vid2audio=info,tower_http=info \
    PATH="/app/bin:${PATH}"

EXPOSE 8000
CMD ["vid2audio"]
