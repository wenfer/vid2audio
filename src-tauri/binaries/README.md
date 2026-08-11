随包分发的 ffmpeg/ffprobe 放在这个目录。

内容不入库（体积约 60 MB），构建前先跑：

    python3 scripts/fetch_ffmpeg.py

脚本会从 BtbN/FFmpeg-Builds 拉 **LGPL shared** 构建，取出 `ffmpeg.exe`、
`ffprobe.exe`、它们依赖的 DLL，以及许可证文本。

为什么是 LGPL 版：本项目只做音频，用不到 libx264/libx265——而那两个才是把
ffmpeg 拖进 GPL 的原因。LGPL 版在功能上没有任何损失。

分发时请保留本目录下的许可证文件：LGPL 要求随二进制提供许可证文本和获取源码
的途径。BtbN 的构建脚本和源码在 https://github.com/BtbN/FFmpeg-Builds。

用户机器上如果已经装了 ffmpeg，本目录为空也能跑——`platform::find_command`
会退回去查 PATH。
