# Vid2Audio

儿童故事机音频资源生产系统。它扫描视频目录，识别合集和集数，调用 FFmpeg 提取音轨，并生成适合 FAT32/NTFS 简单排序规则的前导零文件名。

## 当前实现

- FastAPI 后端与静态 Web UI
- 目录扫描、合集识别、标题清理
- 支持按文件夹或单个视频文件创建音频提取任务
- ffprobe 音轨解析
- 音轨选择、10 秒试听、开头/结尾偏移
- MP3/M4A/OGG/FLAC/WAV/OPUS 提取接口
- `000_合集名.mp3` TTS 提示音占位/生成
- 前导零排序适配与排序验证
- 任务进度、逐文件结果、成功/失败数量和失败原因简报
- 全局最小文件大小、视频后缀白名单和过滤后缀配置
- 硬件加速能力检测，默认自动选择 QSV/VAAPI/CUDA/Rockchip MPP/VideoToolbox，失败自动回退 CPU
- SQLite 持久化
- Docker 与 docker-compose 部署文件

## 本地运行

```bash
python3 -m venv .venv
. .venv/bin/activate
pip install -r requirements.txt
VID2AUDIO_DB=data/vid2audio.db VID2AUDIO_INPUT=/path/to/videos VID2AUDIO_OUTPUT=/path/to/output \
  uvicorn backend.app.main:app --reload
```

打开 http://127.0.0.1:8000。

本机需要安装 `ffmpeg` 和 `ffprobe` 才能解析和提取真实视频。Docker 镜像会自动安装 FFmpeg。

## 硬件加速策略

音频提取通常不需要解码视频画面，硬件视频解码对多数任务收益有限。Vid2Audio 默认使用“自动选择”，检测到合适后端时会尝试启用；不可用或失败时回退 CPU，优先保证 NAS 和 Docker 环境稳定运行。

也可以在 Web UI 的“全局配置”中手动选择：

- Intel NAS: `qsv` 或 `vaapi`
- NVIDIA GPU: `cuda`
- Rockchip ARM NAS: `rkmpp`
- macOS 本机调试: `videotoolbox`

如果启用后 FFmpeg 失败，任务会自动用 CPU 重试，并在任务简报中记录回退原因。

## Docker 运行

先调整 [docker/docker-compose.yml](/Users/qiuyuan/vscode/vid2audio/docker/docker-compose.yml) 中的输入和输出目录挂载：

```bash
docker compose -f docker/docker-compose.yml up --build
```

默认 compose 不挂载任何硬件设备，保证不支持硬件加速的 NAS 也能正常启动。确认主机支持后，可以叠加 override：

Intel iGPU / VAAPI / QSV：

```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.intel-vaapi.yml \
  up --build
```

NVIDIA GPU：

```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.nvidia.yml \
  up --build
```

Rockchip ARM / RK356x / RK3588：

```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.rockchip.yml \
  up --build
```

Rockchip 需要宿主机暴露 MPP/RGA 设备，并且镜像中的 FFmpeg 具备 `rkmpp` 解码器。不同 NAS 系统设备节点不完全一致，如果容器启动时报某个 `/dev/...` 不存在，可以在 `docker/docker-compose.rockchip.yml` 中注释掉缺失设备。

启动后可在容器内确认 FFmpeg 支持项：

```bash
docker compose -f docker/docker-compose.yml exec vid2audio ffmpeg -hide_banner -hwaccels
```

硬件加速通过环境变量控制：

- `VID2AUDIO_HWACCEL=auto|safe|qsv|vaapi|cuda|rkmpp|videotoolbox`
- `VID2AUDIO_HWACCEL_DEVICE=/dev/dri/renderD128`
- `VID2AUDIO_HWACCEL_FALLBACK=true`

## GHCR 镜像

推送到 `main` 分支或 `v*.*.*` tag 时，GitHub Actions 会构建并发布多架构镜像：

- `linux/amd64`
- `linux/arm64`

镜像地址：

```text
ghcr.io/wenfer/vid2audio
```

## API

Base URL: `/api/v1`

- `POST /scan/start`
- `GET /collections`
- `GET /collections/{id}`
- `POST /extract`
- `GET /extract/jobs`
- `GET /settings`
- `PUT /settings`
- `GET /system/status`
