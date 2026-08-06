# Vid2Audio

儿童故事机音频资源生产系统。它扫描视频目录，识别合集和集数，调用 FFmpeg 提取音轨，并生成适合 FAT32/NTFS 简单排序规则的前导零文件名。

## 当前实现

- Rust/Axum 后端与 Vue 3 静态 Web UI
- 目录扫描、合集识别、标题清理
- 支持按文件夹或单个视频文件创建音频提取任务
- ffprobe 音轨解析
- 音轨选择、10 秒试听、导出音频播放、开头/结尾偏移
- MP3/M4A/OGG/FLAC/WAV/OPUS 提取接口
- `000_合集名.mp3` TTS 提示音占位/生成
- NTFS/FAT 兼容排序、自然排序、前导零适配与排序验证
- 任务进度、逐文件结果、成功/失败数量和失败原因简报
- 全局最小文件大小、视频后缀白名单和过滤后缀配置
- SQLite 持久化
- Docker 与 docker-compose 部署文件

## 本地运行

```bash
cd frontend
npm ci
npm run build

cd ../backend
VID2AUDIO_DB=../data/vid2audio.db \
VID2AUDIO_INPUT=/path/to/videos \
VID2AUDIO_OUTPUT=/path/to/output \
cargo run
```

打开 http://127.0.0.1:8000。

本机直接运行需要安装 `ffmpeg` 和 `ffprobe`。默认 Docker/GHCR 镜像已经内置两者。

常用检查命令：

```bash
cd backend
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings

cd ../frontend
npm run build
```

## Docker 运行

默认镜像基于 `debian:bookworm-slim`，包含 Rust 服务以及 Debian 提供的 `ffmpeg`/`ffprobe`，AMD64 和 ARM64 平台拉取后即可使用，不需要宿主机安装或挂载 FFmpeg。

先调整 [docker/docker-compose.yml](docker/docker-compose.yml) 中的路径：

```yaml
volumes:
  - /your/videos:/app/input:ro
  - /your/output:/app/output
  - /your/data:/app/data
```

然后启动：

```bash
docker compose -f docker/docker-compose.yml up --build
```

直接拉取 GHCR 默认镜像：

```bash
docker run -d --name vid2audio -p 8000:8000 \
  -v /your/videos:/app/input:ro \
  -v /your/output:/app/output \
  -v /your/data:/app/data \
  ghcr.io/wenfer/vid2audio:latest
```

`latest` 是唯一发布标签，内含 `linux/amd64` 和 `linux/arm64` 两个架构变体。Docker 会按宿主机架构自动拉取对应镜像。

## 排序和 TTS

系统配置中可以选择文件系统排序策略：

- `NTFS/FAT 兼容排序`: 面向故事机、U 盘、NAS 文件遍历的默认策略，配合前导零文件名保证播放顺序。
- `自然数字排序`: 更贴近桌面文件管理器的人类数字顺序。
- `按名称排序`: 只按文件名大小写折叠后排序。

TTS 片头支持多通道配置：

- `Piper 离线 TTS`（推荐）: 完全离线的神经网络 TTS，无需联网，适合 NAS/Docker 环境。需要安装 Piper 二进制和中文语音模型。
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

发布 `vX.Y.Z` Git 标签时，GitHub Actions 会校验它与 Cargo 版本一致，并行构建两个原生架构：

- `linux/amd64`
- `linux/arm64`

每个版本发布两个可读标签，它们指向同一份多架构 manifest，不会重复构建镜像：

```text
ghcr.io/wenfer/vid2audio:v0.2.1
ghcr.io/wenfer/vid2audio:latest
```

版本规范统一为：Cargo 与前端使用 `X.Y.Z`，Git 和镜像使用 `vX.Y.Z`。版本标签用于固定生产部署，`latest` 指向最新发布；不再生成提交哈希或 `-ffmpeg` 标签。Docker 会根据 x86_64 或 ARM64 宿主机自动选择正确变体。

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
