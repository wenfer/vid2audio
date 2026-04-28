FROM python:3.11-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY backend/requirements.txt /app/backend/requirements.txt
RUN pip install --no-cache-dir -r /app/backend/requirements.txt

COPY pyproject.toml /app/pyproject.toml
COPY backend /app/backend
COPY docs /app/docs
RUN pip install --no-cache-dir /app

RUN mkdir -p /app/input /app/output /app/config /app/data

ENV VID2AUDIO_DB=/app/data/vid2audio.db
ENV VID2AUDIO_INPUT=/app/input
ENV VID2AUDIO_OUTPUT=/app/output
ENV PYTHONUNBUFFERED=1

RUN cd /tmp \
    && python -c "import backend.app.main; from pathlib import Path; import backend.app; assert (Path(backend.app.__file__).parent / 'static' / 'index.html').exists()"

EXPOSE 8000

CMD ["python", "-m", "uvicorn", "backend.app.main:app", "--host", "0.0.0.0", "--port", "8000"]
