随包分发的 ffmpeg/ffprobe 放在这个目录。

内容不入库（体积约 60 MB），构建前先跑：

- **Windows**：`python3 scripts/fetch_ffmpeg.py`
- **macOS**：`python3 scripts/build_ffmpeg_macos.py --arch aarch64|x86_64`

Windows 脚本从 BtbN/FFmpeg-Builds 拉 **LGPL shared** 构建，取出 `ffmpeg.exe`、
`ffprobe.exe`、它们依赖的 DLL，以及许可证文本。BtbN 已停发 macOS 构建，
所以 macOS 脚本改为在 CI 里从源码编译 **LGPL 静态** 的 ffmpeg/ffprobe（含
lame/vorbis/opus），输出无后缀的 `ffmpeg`/`ffprobe`，许可证文本在 `licenses/` 下。

为什么是 LGPL 版：本项目只做音频，用不到 libx264/libx265——而那两个才是把
ffmpeg 拖进 GPL 的原因。LGPL 版在功能上没有任何损失。

分发时请保留许可证文件：LGPL 要求随二进制提供许可证文本和获取源码的途径。
Windows 的源码在 https://github.com/BtbN/FFmpeg-Builds，macOS 的编译参数与
版本固定在 `scripts/build_ffmpeg_macos.py` 里（ffmpeg/lame/ogg/vorbis/opus
的官方源码 tarball）。

用户机器上如果已经装了 ffmpeg，本目录为空也能跑——`platform::find_command`
会退回去查 PATH。
