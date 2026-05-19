# ---- Frontend build stage ----
FROM node:20-slim AS frontend

WORKDIR /frontend

COPY frontend/package.json frontend/package-lock.json* /frontend/
RUN npm install --registry https://registry.npmmirror.com

COPY frontend/ /frontend/
RUN npm run build


# ---- Python build stage ----
FROM python:3.11-slim AS builder

WORKDIR /build

COPY backend/requirements.txt /build/requirements.txt
RUN pip install --no-cache-dir --no-compile --prefix=/install -r /build/requirements.txt

COPY pyproject.toml /build/pyproject.toml
COPY backend /build/backend

# Copy frontend build output into static directory
COPY --from=frontend /backend/app/static /build/backend/app/static

RUN pip install --no-cache-dir --no-compile --no-deps --prefix=/install /build

# Strip unnecessary metadata
RUN find /install -type d -name '__pycache__' -exec rm -rf {} + 2>/dev/null; \
    find /install -type d -name 'tests' -exec rm -rf {} + 2>/dev/null; \
    find /install -type d -name '*.dist-info' -exec sh -c 'rm -f "$1"/RECORD "$1"/SOURCES.txt "$1"/direct_url.json' _ {} \; 2>/dev/null; \
    true


# ---- Runtime stage ----
FROM python:3.11-slim

RUN rm -rf /var/lib/apt/lists/* /usr/share/doc /usr/share/man /usr/share/info

COPY --from=builder /install /usr/local

WORKDIR /app

RUN mkdir -p /app/input /app/output /app/data /app/bin

ENV VID2AUDIO_DB=/app/data/vid2audio.db \
    VID2AUDIO_INPUT=/app/input \
    VID2AUDIO_OUTPUT=/app/output \
    PYTHONUNBUFFERED=1 \
    PYTHONDONTWRITEBYTECODE=1 \
    PATH="/app/bin:${PATH}"

# Verify the package is importable and static assets are bundled
RUN python -c "import backend.app.main; from pathlib import Path; import backend.app; p = Path(backend.app.__file__).parent / 'static'; assert (p / 'index.html').exists(); assert (p / 'assets').is_dir()"

EXPOSE 8000

CMD ["python", "-m", "uvicorn", "backend.app.main:app", "--host", "0.0.0.0", "--port", "8000"]
