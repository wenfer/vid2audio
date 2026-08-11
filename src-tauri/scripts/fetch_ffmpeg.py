#!/usr/bin/env python3
"""下载随包分发的 ffmpeg/ffprobe（Windows）。

只取 **LGPL** 构建：本项目只做音频，用不到 libx264/libx265，
而那两个才是把 ffmpeg 拖进 GPL 的原因。LGPL 版功能上没有任何损失
（MP3 编码器 libmp3lame 本身就是 LGPL，MP3 专利也已于 2017 年全部到期）。

取 shared 版而非 static：两个 exe 共享同一套 DLL，总体积约为 static 的一半。

用法：
    python3 scripts/fetch_ffmpeg.py            # 下到 binaries/
    python3 scripts/fetch_ffmpeg.py --force    # 已存在也重新下

分发时请连带保留 binaries/ 下的许可证文件——LGPL 要求随二进制提供许可证文本
和获取源码的途径。
"""

import argparse
import io
import pathlib
import shutil
import sys
import urllib.request
import zipfile

# 用带版本号的稳定分支，不用 master：master 每天变，构建不可复现。
RELEASE = "ffmpeg-n7.1-latest-win64-lgpl-shared-7.1.zip"
URL = f"https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/{RELEASE}"

WANTED_EXES = {"ffmpeg.exe", "ffprobe.exe"}
# avdevice 是摄像头/麦克风采集，本项目只读文件，7.7 MB 白搭。
# 其余都要留着：avfilter 提供 loudnorm（片头响度归一化）和 lavfi 虚拟输入，
# 少了它生成静音片头和响度处理都会失败。
SKIP_DLLS = {"avdevice"}
# 许可证文本必须随二进制分发。
LICENSE_SUFFIXES = (".txt", "LICENSE", "COPYING")

HERE = pathlib.Path(__file__).resolve().parent
TARGET = HERE.parent / "binaries"


def download(url):
    print(f"下载 {url}")
    request = urllib.request.Request(url, headers={"User-Agent": "vid2audio-build"})
    with urllib.request.urlopen(request) as response:
        total = int(response.headers.get("content-length") or 0)
        chunks, read = [], 0
        while chunk := response.read(1 << 20):
            chunks.append(chunk)
            read += len(chunk)
            if total:
                print(f"\r  {read / 1048576:.1f}/{total / 1048576:.1f} MB", end="")
        print()
    return b"".join(chunks)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--force", action="store_true", help="已存在也重新下载")
    args = parser.parse_args()

    TARGET.mkdir(parents=True, exist_ok=True)
    if not args.force and all((TARGET / name).is_file() for name in WANTED_EXES):
        print(f"{TARGET} 下已有 ffmpeg/ffprobe，跳过（--force 可强制重下）")
        return 0

    archive = zipfile.ZipFile(io.BytesIO(download(URL)))
    extracted = 0
    for entry in archive.infolist():
        if entry.is_dir():
            continue
        name = pathlib.PurePosixPath(entry.filename).name
        parts = pathlib.PurePosixPath(entry.filename).parts
        # 结构是 <顶层目录>/bin/*.exe|*.dll 和 <顶层目录>/LICENSE.txt。
        in_bin = "bin" in parts
        is_wanted_exe = in_bin and name in WANTED_EXES
        is_dll = (
            in_bin
            and name.lower().endswith(".dll")
            and not any(name.lower().startswith(skip) for skip in SKIP_DLLS)
        )
        is_license = name.endswith(LICENSE_SUFFIXES) or name in {"LICENSE", "COPYING"}
        if not (is_wanted_exe or is_dll or is_license):
            continue
        with archive.open(entry) as source, open(TARGET / name, "wb") as sink:
            shutil.copyfileobj(source, sink)
        extracted += 1
        print(f"  {name}")

    missing = [name for name in WANTED_EXES if not (TARGET / name).is_file()]
    if missing:
        print(f"错误：压缩包里没找到 {', '.join(missing)}", file=sys.stderr)
        return 1
    size = sum(f.stat().st_size for f in TARGET.iterdir() if f.is_file())
    print(f"完成：{extracted} 个文件，共 {size / 1048576:.1f} MB → {TARGET}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
