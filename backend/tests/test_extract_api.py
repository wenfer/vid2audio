from types import SimpleNamespace

from backend.app.api import extract


def test_play_extracted_audio_serves_completed_output(monkeypatch, tmp_path):
    audio = tmp_path / "episode.mp3"
    audio.write_bytes(b"audio")
    item = SimpleNamespace(id="item-1", status="completed", output_path=str(audio))
    job = SimpleNamespace(items=[item])
    service = SimpleNamespace(get_job_detail=lambda job_id: job)

    monkeypatch.setattr(extract, "get_extract_service", lambda: service)

    response = extract.play_extracted_audio("job-1", "item-1")

    assert response.path == audio
    assert response.media_type == "audio/mpeg"
