FROM python:3.11-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY backend/requirements.txt /app/backend/requirements.txt
RUN pip install --no-cache-dir -r /app/backend/requirements.txt

COPY backend /app/backend
COPY docs /app/docs

RUN mkdir -p /app/input /app/output /app/config /app/data

ENV VID2AUDIO_DB=/app/data/vid2audio.db
ENV VID2AUDIO_INPUT=/app/input
ENV VID2AUDIO_OUTPUT=/app/output
ENV PYTHONUNBUFFERED=1

EXPOSE 8000

CMD ["uvicorn", "backend.app.main:app", "--host", "0.0.0.0", "--port", "8000"]
