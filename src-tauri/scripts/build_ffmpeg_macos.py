#!/usr/bin/env python3
"""在 macOS 上从源码编译 LGPL ffmpeg/ffprobe 到 src-tauri/binaries/。

背景：BtbN 已停发 macOS 构建（Windows/Linux 仍有），evermeet/osxexperts 等
预编译包又都是 GPL 构建。本项目只做音频、用不到 libx264 之类 GPL 组件，
文档里明确坚持 LGPL 发行，所以 macOS 版只能自己编。

产物是两个静态二进制 ffmpeg / ffprobe：
  - 静态版没有 dylib 安装路径（install_name）问题，直接打进 .app 的
    Contents/Resources 就能跑，不依赖系统库里的某个 ffmpeg 版本。
  - 本项目把 ffmpeg 当子进程调用，Rust 侧不链接它；ffmpeg 内部静态链接的
    lame / vorbis / opus 都是 LGPL/BSD，随包带上许可证文本，满足 LGPL
    对再链接义务的常见做法。

内置编码器覆盖后端用到的全部输出格式：
  - libmp3lame（MP3，LGPL）——story player 的核心格式
  - aac / flac / pcm_s16le（ffmpeg 内置）
  - libvorbis（ogg，BSD）+ libopus（opus，BSD）

用法：
    python3 scripts/build_ffmpeg_macos.py --arch aarch64   # 或 x86_64
    python3 scripts/build_ffmpeg_macos.py --arch aarch64 --force

仅 CI 用；本地开发机器直接用系统/Homebrew ffmpeg 即可。
"""

import argparse
import os
import pathlib
import shutil
import subprocess
import sys
import tarfile
import time
import urllib.request

# CI 里 stdout 是管道：默认 locale 编码下中文可能直接 UnicodeEncodeError 打挂构建，
# 和 fetch_ffmpeg.py 的处理一致，显式切 UTF-8。
for _stream in (sys.stdout, sys.stderr):
    try:
        _stream.reconfigure(encoding="utf-8", errors="replace")
    except (AttributeError, OSError, ValueError):
        pass

# 版本号固定，构建可复现。ffmpeg 用 7.1.x 稳定分支，与 Windows 打包的 n7.1 对齐。
FFMPEG_VERSION = "7.1.5"
LAME_VERSION = "3.100"
OGG_VERSION = "1.3.5"
VORBIS_VERSION = "1.3.7"
OPUS_VERSION = "1.5.2"

URLS = {
    "ffmpeg": f"https://ffmpeg.org/releases/ffmpeg-{FFMPEG_VERSION}.tar.xz",
    "lame": (
        "https://downloads.sourceforge.net/project/lame/lame/"
        f"{LAME_VERSION}/lame-{LAME_VERSION}.tar.gz"
    ),
    "ogg": f"https://downloads.xiph.org/releases/ogg/libogg-{OGG_VERSION}.tar.gz",
    "vorbis": (
        f"https://downloads.xiph.org/releases/vorbis/libvorbis-{VORBIS_VERSION}.tar.gz"
    ),
    "opus": f"https://downloads.xiph.org/releases/opus/opus-{OPUS_VERSION}.tar.gz",
}

HERE = pathlib.Path(__file__).resolve().parent
TARGET = HERE.parent / "binaries"

# ffmpeg 自身的许可证文本（LGPL 随源码发行，COPYING.LGPLv2.1 / COPYING.LGPLv3）
FFMPEG_LICENSES = ("COPYING.LGPLv2.1", "COPYING.LGPLv3")
# 第三方库源码里随包的许可证文件（lame 是 LGPL 2.1+，ogg/vorbis/opus 是 BSD）。
EXTRA_LICENSES = {
    "lame": ("COPYING", "LICENSE", "README"),
    "ogg": ("COPYING",),
    "vorbis": ("COPYING",),
    "opus": ("COPYING",),
}


def download(url: str, dest: pathlib.Path) -> None:
    print(f"下载 {url}")
    last_error = None
    for attempt in range(1, 4):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": "vid2audio-build"})
            with urllib.request.urlopen(request) as response:
                total = int(response.headers.get("content-length") or 0)
                live = total > 0 and sys.stdout.isatty()
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
            dest.write_bytes(b"".join(chunks))
            return
        except (urllib.error.URLError, OSError, EOFError) as error:
            last_error = error
            print(f"  第 {attempt} 次失败（{error}），2 秒后重试…", file=sys.stderr)
            time.sleep(2 * attempt)
    raise RuntimeError(f"下载失败：{url}（{last_error}）")


def fetch(url: str, build_root: pathlib.Path, dest_dir: pathlib.Path) -> pathlib.Path:
    """下载（已有则跳过）并解压源码包，返回顶层源码目录。

    每个压缩包解到独立的子目录：多个包共享 dest_dir 时，第二次解压会把
    前一个源码目录一并数进来，导致顶层目录数量判断出错。
    """
    archive_name = pathlib.PurePosixPath(url.rstrip("/")).name
    archive = build_root / archive_name
    if not archive.is_file():
        download(url, archive)
    extract_dir = dest_dir / archive_name
    extract_dir.mkdir(parents=True, exist_ok=True)
    with tarfile.open(archive) as tf:
        tf.extractall(extract_dir)
    top = [p for p in extract_dir.iterdir() if p.is_dir()]
    if len(top) != 1:
        raise RuntimeError(f"{archive_name} 解出 {len(top)} 个顶层目录")
    return top[0]


def run(cmd, cwd, env=None, **kwargs) -> None:
    print(f"$ {' '.join(cmd)}")
    subprocess.run(cmd, cwd=cwd, env=env, check=True, **kwargs)


def strip_unsupported_ld_flag(root: pathlib.Path) -> None:
    """旧版 autotools 项目在 Darwin 的 CFLAGS 里塞 `-force_cpusubtype_ALL`，
    新版 clang/ld 已删除该选项，链接时报 `unknown options`（libvorbis 等
    2010 年代的包都中招）。它只影响链接不影响功能，把生成文件里出现它的地方
    全部删掉（configure 生成的 Makefile / config.status / libtool 等）。
    """
    target = "-force_cpusubtype_ALL"
    touched = 0
    for file in root.rglob("*"):
        if not file.is_file():
            continue
        try:
            text = file.read_text(errors="replace")
        except OSError:
            continue
        if target not in text:
            continue
        file.write_text(text.replace(target, ""))
        touched += 1
    if touched:
        print(f"  已从 {touched} 个文件移除 {target}（新版链接器不支持）")


def autotools_build(src: pathlib.Path, prefix: pathlib.Path, *configure_args: str) -> None:
    """configure + make + make install 一条龙（静态库）。"""
    env = os.environ.copy()
    env.setdefault("CFLAGS", f"-I{prefix}/include")
    env.setdefault("CPPFLAGS", f"-I{prefix}/include")
    env.setdefault("LDFLAGS", f"-L{prefix}/lib")
    env["PKG_CONFIG_PATH"] = str(prefix / "lib" / "pkgconfig")
    run(
        ["./configure", f"--prefix={prefix}", "--disable-shared", "--enable-static", *configure_args],
        src,
        env,
    )
    strip_unsupported_ld_flag(src)
    run(["make", f"-j{jobs}"], src, env)
    run(["make", "install"], src, env)


def build_all(arch: str, jobs: int, force: bool) -> None:
    if not force and (TARGET / "ffmpeg").is_file() and (TARGET / "ffprobe").is_file():
        print(f"{TARGET} 下已有 ffmpeg/ffprobe，跳过（--force 可强制重编）")
        return

    TARGET.mkdir(parents=True, exist_ok=True)
    build_root = HERE.parent / "ffbuild" / arch
    prefix = build_root / "prefix"
    src_dir = build_root / "src"
    # 保留已下载的压缩包（断点续跑不用重下），只清源码目录和安装前缀。
    for stale in (src_dir, prefix):
        if stale.exists():
            shutil.rmtree(stale)
    build_root.mkdir(parents=True, exist_ok=True)

    ffmpeg_src = fetch(URLS["ffmpeg"], build_root, src_dir)
    ogg_src = fetch(URLS["ogg"], build_root, src_dir)
    vorbis_src = fetch(URLS["vorbis"], build_root, src_dir)
    opus_src = fetch(URLS["opus"], build_root, src_dir)
    lame_src = fetch(URLS["lame"], build_root, src_dir)

    # 依赖顺序：ogg 先于 vorbis，三者都先于 ffmpeg。
    print("\n=== 编译 libogg（BSD） ===")
    autotools_build(ogg_src, prefix)
    print("\n=== 编译 libvorbis（BSD，依赖 ogg） ===")
    autotools_build(vorbis_src, prefix, f"--with-ogg={prefix}", "--disable-oggtest")
    print("\n=== 编译 libopus（BSD） ===")
    autotools_build(opus_src, prefix, "--disable-doc", "--disable-extra-programs")
    # lame 的 configure 会自动探测 nasm；没有就退回纯 C，编码功能一致。
    print("\n=== 编译 libmp3lame（LGPL） ===")
    autotools_build(lame_src, prefix, "--disable-frontend", "--disable-dependency-tracking")

    print("\n=== 编译 ffmpeg（LGPL，静态） ===")
    env = os.environ.copy()
    env.setdefault("CFLAGS", f"-I{prefix}/include")
    env["PKG_CONFIG_PATH"] = str(prefix / "lib" / "pkgconfig")
    run(
        [
            "./configure",
            "--cc=clang",
            f"--arch={arch}",
            "--enable-static",
            "--disable-shared",
            "--disable-doc",
            "--disable-debug",
            "--disable-ffplay",
            "--disable-network",
            # 关掉外部库自动探测：只编明确启用的三个（lame/vorbis/opus），
            # 系统里装了什么 x264 之类 GPL 组件也不会被无意卷进来。
            "--disable-autodetect",
            "--enable-libmp3lame",
            "--enable-libvorbis",
            "--enable-libopus",
            f"--extra-cflags=-I{prefix}/include",
            f"--extra-ldflags=-L{prefix}/lib",
            "--pkg-config-flags=--static",
            f"--prefix={prefix}",
        ],
        ffmpeg_src,
        env,
    )
    run(["make", f"-j{jobs}"], ffmpeg_src, env)
    run(["make", "install"], ffmpeg_src, env)

    # 拷贝产物并瘦身
    for name in ("ffmpeg", "ffprobe"):
        built = prefix / "bin" / name
        target = TARGET / name
        shutil.copy2(built, target)
        target.chmod(0o755)
        run(["strip", str(target)], build_root)

    # 许可证文本随包分发（LGPL 要求随二进制提供许可证与获取源码的途径）。
    licenses = TARGET / "licenses"
    licenses.mkdir(exist_ok=True)
    for name in FFMPEG_LICENSES:
        shutil.copy2(ffmpeg_src / name, licenses / name)
    for lib, names in EXTRA_LICENSES.items():
        src_files = {
            "lame": lame_src,
            "ogg": ogg_src,
            "vorbis": vorbis_src,
            "opus": opus_src,
        }
        for name in names:
            src_file = src_files[lib] / name
            if src_file.is_file():
                shutil.copy2(src_file, licenses / f"{lib}-{name}")

    # 冒烟测试：确认 MP3 编码链路完整（libmp3lame 生效）
    smoke = build_root / "smoke.mp3"
    run(
        [
            str(TARGET / "ffmpeg"),
            "-hide_banner",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=1",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "128k",
            str(smoke),
        ],
        build_root,
    )
    if not smoke.is_file():
        raise RuntimeError("MP3 冒烟测试失败：未生成 smoke.mp3")
    run([str(TARGET / "ffmpeg"), "-version"], build_root)

    size = sum(f.stat().st_size for f in TARGET.rglob("*") if f.is_file())
    print(f"\n完成：{TARGET} 共 {size / 1048576:.1f} MB")


def main() -> int:
    parser = argparse.ArgumentParser(description="编译 macOS LGPL ffmpeg/ffprobe")
    parser.add_argument(
        "--arch",
        choices=("aarch64", "x86_64"),
        default="aarch64" if os.uname().machine == "arm64" else "x86_64",
    )
    parser.add_argument("--force", action="store_true", help="已存在也重新编译")
    args = parser.parse_args()

    global jobs
    jobs = os.cpu_count() or 2
    try:
        build_all(args.arch, jobs, args.force)
    except (subprocess.CalledProcessError, RuntimeError) as error:
        print(f"构建失败：{error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())