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

# CI 里 stdout 是管道而不是终端，此时 Python 3.12 用系统 locale 编码：英文
# Windows runner 上是 cp1252，一 print 中文就 UnicodeEncodeError 直接把构建打挂
# （下载都还没开始）。显式切 UTF-8；errors="replace" 是兜底，遇到装不下的流最多
# 输出问号，绝不能让一行日志决定构建成败。
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, OSError, ValueError):
        pass

# 用带版本号的稳定分支，不用 master：master 每天变，构建不可复现。
RELEASE = "ffmpeg-n7.1-latest-win64-lgpl-shared-7.1.zip"
URL = f"https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/{RELEASE}"

WANTED_EXES = {"ffmpeg.exe", "ffprobe.exe"}
# 一个 dll 都不能跳过：shared 构建的 ffmpeg.exe 在 PE 导入表里链接了全部 lib
# （含 avdevice-61.dll）。Windows 加载器启动 exe 时会解析整张导入表，缺任何
# 一个都直接报"找不到 avdevice-61.dll，无法继续执行代码"——曾为省 7.7 MB
# 跳过 avdevice，结果安装后 ffmpeg.exe 根本起不来。
SKIP_DLLS = set()
# 许可证文本必须随二进制分发。
LICENSE_SUFFIXES = (".txt", "LICENSE", "COPYING")

HERE = pathlib.Path(__file__).resolve().parent
TARGET = HERE.parent / "binaries"


def download(url):
    print(f"下载 {url}")
    request = urllib.request.Request(url, headers={"User-Agent": "vid2audio-build"})
    with urllib.request.urlopen(request) as response:
        total = int(response.headers.get("content-length") or 0)
        # 进度条只在真终端里刷：CI 日志不认 \r，115 次刷新会摊成一行 115 段的噪音。
        live = total > 0 and sys.stdout is not None and sys.stdout.isatty()
        if total and not live:
            print(f"  {total / 1048576:.1f} MB")
        chunks, read = [], 0
        while chunk := response.read(1 << 20):
            chunks.append(chunk)
            read += len(chunk)
            if live:
                print(f"\r  {read / 1048576:.1f}/{total / 1048576:.1f} MB", end="")
        if live:
            print()
    return b"".join(chunks)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--force", action="store_true", help="已存在也重新下载")
    args = parser.parse_args()

    TARGET.mkdir(parents=True, exist_ok=True)
    if (
        not args.force
        and all((TARGET / name).is_file() for name in WANTED_EXES)
        and any(TARGET.glob("*.dll"))  # 旧下载可能缺 avdevice-61.dll，无 dll 就重下
    ):
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
