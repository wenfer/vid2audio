# Vid2Audio

儿童故事机音频资源生产系统。它扫描视频目录，识别合集和集数，调用 FFmpeg 提取音轨，并生成适合 FAT32/NTFS 简单排序规则的前导零文件名。

## 当前实现

- FastAPI 后端与静态 Web UI
- 目录扫描、合集识别、标题清理
- 支持按文件夹或单个视频文件创建音频提取任务
- ffprobe 音轨解析
- 音轨选择、10 秒试听、导出音频播放、开头/结尾偏移
- MP3/M4A/OGG/FLAC/WAV/OPUS 提取接口
- `000_合集名.mp3` TTS 提示音占位/生成
- NTFS/FAT 兼容排序、自然排序、前导零适配与排序验证
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

本机需要安装 `ffmpeg` 和 `ffprobe` 才能解析和提取真实视频。Docker 默认通过挂载宿主机二进制文件使用 FFmpeg。

## 硬件加速策略

音频提取通常不需要解码视频画面，硬件视频解码对多数任务收益有限。Vid2Audio 默认使用“自动选择”，检测到合适后端时会尝试启用；不可用或失败时回退 CPU，优先保证 NAS 和 Docker 环境稳定运行。

也可以在 Web UI 的“全局配置”中手动选择：

- Intel NAS: `qsv` 或 `vaapi`
- NVIDIA GPU: `cuda`
- Rockchip ARM NAS: `rkmpp`
- macOS 本机调试: `videotoolbox`

如果启用后 FFmpeg 失败，任务会自动用 CPU 重试，并在任务简报中记录回退原因。

## Docker 运行

默认镜像**不包含 FFmpeg**，通过挂载宿主机的 `ffmpeg`/`ffprobe` 二进制文件来使用。这样镜像体积更小（约 80MB vs 200MB+），且可以直接使用宿主机已编译好的硬件加速版本。

先调整 [docker/docker-compose.yml](docker/docker-compose.yml) 中的路径：

```yaml
volumes:
  - /your/videos:/app/input:ro
  - /your/output:/app/output
  # 映射宿主机 ffmpeg（根据实际路径调整）
  - /usr/bin/ffmpeg:/app/bin/ffmpeg:ro
  - /usr/bin/ffprobe:/app/bin/ffprobe:ro
```

然后启动：

```bash
docker compose -f docker/docker-compose.yml up --build
```

### 如果宿主机没有 FFmpeg

使用内置 FFmpeg 的 override 文件（镜像会大 80-120MB）：

```bash
docker compose \
  -f docker/docker-compose.yml \
  -f docker/docker-compose.ffmpeg-bundled.yml \
  up --build
```

### 查找宿主机 FFmpeg 路径

```bash
which ffmpeg ffprobe
# 常见位置: /usr/bin/ffmpeg, /usr/local/bin/ffmpeg
```

### 硬件加速 override

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

Rockchip 需要宿主机暴露 MPP/RGA 设备，并且宿主机 FFmpeg 具备 `rkmpp` 解码器。不同 NAS 系统设备节点不完全一致，如果容器启动时报某个 `/dev/...` 不存在，可以在 override 文件中注释掉缺失设备。

启动后可在容器内确认 FFmpeg 支持项：

```bash
docker compose exec vid2audio ffmpeg -hide_banner -hwaccels
```

硬件加速通过环境变量控制：

- `VID2AUDIO_HWACCEL=auto|safe|qsv|vaapi|cuda|rkmpp|videotoolbox`
- `VID2AUDIO_HWACCEL_DEVICE=/dev/dri/renderD128`
- `VID2AUDIO_HWACCEL_FALLBACK=true`

## 排序和 TTS

系统配置中可以选择文件系统排序策略：

- `NTFS/FAT 兼容排序`: 面向故事机、U 盘、NAS 文件遍历的默认策略，配合前导零文件名保证播放顺序。
- `自然数字排序`: 更贴近桌面文件管理器的人类数字顺序。
- `按名称排序`: 只按文件名大小写折叠后排序。

TTS 片头支持多通道配置：

- `Piper 离线 TTS`（推荐）: 完全离线的神经网络 TTS，无需联网，适合 NAS/Docker 环境。需要安装 Piper 二进制和中文语音模型。
- `Edge 在线 TTS`: 使用 `edge-tts`，需要容器能访问 Microsoft 在线服务（不稳定，可能被限流）。
- `静音占位`: 不访问云端，生成 1 秒静音片头。
- `禁用片头`: 不生成片头文件。

### 安装 Piper TTS（推荐）

```bash
# 在宿主机安装 Piper
pip install piper-tts

# 下载中文语音模型
python3 -m piper.download_voices zh_CN-huayan-medium

# 找到 piper 二进制和模型路径
which piper
# 模型通常在 ~/.local/share/piper-voices/ 或 site-packages 内
```

Docker 中使用 Piper：

```yaml
volumes:
  - /usr/local/bin/piper:/app/bin/piper:ro
  - /path/to/piper-voices:/app/data/piper-voices:ro
```

如果 TTS 失败，可以选择静音占位、跳过片头或终止任务。

## GHCR 镜像

推送到 `main` 分支或 `v*.*.*` tag 时，GitHub Actions 会构建并发布多架构镜像：

- `linux/amd64`
- `linux/arm64`

镜像标签：

```text
ghcr.io/wenfer/vid2audio:<commit-id>
ghcr.io/wenfer/vid2audio:latest
```

如果 Actions 在推送阶段报 `permission_denied: write_package`，优先检查仓库设置：

1. 进入 GitHub 仓库 `Settings -> Actions -> General -> Workflow permissions`，选择 `Read and write permissions`。
2. 如果仓库属于组织，确认组织没有把 Actions 的 package 写入权限禁用。
3. 如果 `GITHUB_TOKEN` 仍然无法写入 GHCR，创建一个 classic PAT，勾选 `write:packages` 和 `read:packages`；私有仓库还需要 `repo`。然后在仓库 `Settings -> Secrets and variables -> Actions` 中添加：
   - `GHCR_TOKEN`: PAT 内容
   - `GHCR_USERNAME`: PAT 所属 GitHub 用户名，可选；不设置时默认使用 Actions 触发者
4. 如果同名 package 已经存在，进入 package 设置确认此仓库拥有访问权限，或者删除旧 package 后重新发布。

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
