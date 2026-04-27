from backend.app.core.media import ffmpeg_acceleration_args, resolve_hardware_acceleration


def test_auto_acceleration_falls_back_to_safe_without_ffmpeg():
    assert resolve_hardware_acceleration("auto") == "safe"
    assert ffmpeg_acceleration_args("auto") == []


def test_explicit_acceleration_builds_ffmpeg_args():
    assert ffmpeg_acceleration_args("qsv") == ["-hwaccel", "qsv"]
    assert ffmpeg_acceleration_args("vaapi", "/dev/dri/renderD128") == [
        "-hwaccel",
        "vaapi",
        "-hwaccel_device",
        "/dev/dri/renderD128",
    ]
