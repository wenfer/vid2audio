import shutil
import sys
import time
import urllib.request
import zipfile
from pathlib import Path

# yt-dlp/ffmpeg-binaries 的 LGPL FFmpeg win64 构建（zip 内为 ffmpeg.exe、ffprobe.exe
# 及一堆 avcodec/avformat/avutil 等 dll；文件可能在子目录中，解压后统一提升到根目录）。
FFMPEG_URL = "https://github.com/yt-dlp/ffmpeg-binaries/releases/download/0.3.0/ffmpeg-win64-v5.1.2.zip"


def download_file(url: str, dest_path: Path, progress_cb=None):
    """下载文件并显示进度"""
    req = urllib.request.Request(url, headers={'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36'})
    with urllib.request.urlopen(req) as response, open(dest_path, 'wb') as out_file:
        total_length = response.headers.get('content-length')
        if total_length:
            total_length = int(total_length)
            downloaded = 0
            last_update = 0
            while True:
                data = response.read(8192)
                if not data:
                    break
                out_file.write(data)
                downloaded += len(data)
                if progress_cb:
                    progress_cb(downloaded, total_length)
                now = time.time()
                if now - last_update > 0.5:
                    last_update = now
                    print(f"[{downloaded}/{total_length}]", end='', flush=True)
        else:
            out_file.write(response.read())
    print()


def main():
    print("=== Vid2Audio Windows FFmpeg 下载器 ===")
    # 脚本约定在 src-tauri 目录下运行（cwd），binaries/ 即 src-tauri/binaries/，
    # 与 tauri.conf.json 里 `"binaries/": "./"` 的 resources 引用一致。
    ffmpeg_dir = Path("binaries")
    ffmpeg_dir.mkdir(exist_ok=True, parents=True)

    ffmpeg_exe = ffmpeg_dir / "ffmpeg.exe"
    if ffmpeg_exe.exists():
        print("✅ ffmpeg.exe 已存在，无需下载")
        return

    print("正在下载 FFmpeg 64-bit 版本...")
    zip_path = ffmpeg_dir / "ffmpeg.zip"

    try:
        def progress(downloaded, total):
            print(f"\r下载进度: {downloaded}/{total} ({downloaded/total*100:.1f}%)", end='', flush=True)

        download_file(FFMPEG_URL, zip_path, progress)
        print("\n✅ 下载完成")

        print("正在解压...")
        with zipfile.ZipFile(zip_path) as z:
            z.extractall(ffmpeg_dir)
        zip_path.unlink()

        # zip 里的文件可能在子目录中，把散落各处的文件全部提升到 binaries/ 根目录，
        # 否则 tauri build 打包 resources 时找不到 ffmpeg.exe。
        for extracted in ffmpeg_dir.rglob("*"):
            if extracted.is_file() and extracted.parent != ffmpeg_dir:
                target = ffmpeg_dir / extracted.name
                if not target.exists():
                    extracted.rename(target)

        # 清理解压留下的空子目录，避免把多余目录一起打进安装包。
        for sub in list(ffmpeg_dir.iterdir()):
            if sub.is_dir():
                shutil.rmtree(sub, ignore_errors=True)

        if not ffmpeg_exe.exists():
            print("❌ 解压后未找到 ffmpeg.exe", file=sys.stderr)
            sys.exit(1)

        print("✅ FFmpeg 安装成功")
        return

    except Exception as e:
        print(f"\n❌ 下载失败: {e}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
