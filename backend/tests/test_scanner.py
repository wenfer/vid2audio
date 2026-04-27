from pathlib import Path

from backend.app.core.scanner import Scanner


def test_scanner_groups_by_parent_and_warns_without_ffprobe(tmp_path: Path):
    folder = tmp_path / "萌鸡小队第一季"
    folder.mkdir()
    (folder / "萌鸡小队.S01E01.植树节.1080p.mp4").write_bytes(b"not a real video")
    (folder / "萌鸡小队.S01E02.找妈妈.1080p.mp4").write_bytes(b"not a real video")

    collections, warnings = Scanner([".mp4"]).scan([str(tmp_path)])

    assert len(collections) == 1
    assert collections[0].name == "萌鸡小队第一季"
    assert collections[0].episode_count == 2
    assert [video.episode_title for video in collections[0].video_files] == ["植树节", "找妈妈"]
    assert warnings


def test_scanner_filters_small_files_and_ignored_suffixes(tmp_path: Path):
    folder = tmp_path / "素材"
    folder.mkdir()
    (folder / "ok.S01E01.mp4").write_bytes(b"0" * 2048)
    (folder / "tiny.S01E02.mp4").write_bytes(b"1")
    (folder / "partial.part").write_bytes(b"0" * 4096)

    collections, warnings = Scanner([".mp4"], min_file_size_mb=0.001, ignored_extensions=[".part"]).scan([str(folder)])

    assert len(collections) == 1
    assert collections[0].episode_count == 1
    assert collections[0].video_files[0].filename == "ok.S01E01.mp4"
    assert any("已过滤小文件" in warning for warning in warnings)
