from __future__ import annotations

from pathlib import Path

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import FileResponse
from fastapi.staticfiles import StaticFiles

from backend.app.api import collections, extract, files, scan, settings, system


app = FastAPI(title="Vid2Audio", version="0.1.0")

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

api_prefix = "/api/v1"
app.include_router(collections.router, prefix=api_prefix)
app.include_router(scan.router, prefix=api_prefix)
app.include_router(extract.router, prefix=api_prefix)
app.include_router(files.router, prefix=api_prefix)
app.include_router(settings.router, prefix=api_prefix)
app.include_router(system.router, prefix=api_prefix)

static_dir = Path(__file__).parent / "static"
app.mount("/static", StaticFiles(directory=static_dir), name="static")


@app.get("/")
def index():
    return FileResponse(static_dir / "index.html")
