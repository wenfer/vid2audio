# Vid2Audio 产品与实现说明

> 当前版本：0.2.0
>
> 目标平台：Docker、Linux AMD64/ARM64、NAS 设备
> 本文只描述仓库当前实现和必须保持的产品约束。

## 1. 产品目标

Vid2Audio 将视频合集转换为适合儿童故事机、早教机和 U 盘播放的音频包。

核心能力：

- 浏览 NAS 文件系统并选择目录或单个视频。
- 使用 `ffprobe` 识别视频、时长和音轨。
- 选择 FFmpeg stream index 对应的音轨并导出音频。
- 支持 MP3、M4A、AAC、OGG、FLAC、WAV、OPUS。
- 支持开头、结尾裁剪和 10 秒试听。
- 生成带前导零的安全文件名，保证故事机播放顺序。
- 使用 Piper 生成离线合集片头；失败时可静音占位、跳过或终止。
- 保存合集、全局设置和提取任务，展示逐文件结果。

## 2. 当前架构

```text
Vue 3 静态界面
       │ REST / 音频 Range
       ▼
Rust + Axum + Tokio
       ├── SQLite：设置、合集、任务
       ├── ffprobe：媒体分析
       ├── ffmpeg：试听、裁剪、转码
       └── Piper：可选离线 TTS
```

后端是单个 Rust 进程，不依赖 Redis、Celery、PostgreSQL 或独立 Worker。提取任务在 Tokio 后台任务中运行，全局最多并行两个任务。

主要代码：

- `backend/src/api.rs`：REST API、静态资源、音频 Range 响应。
- `backend/src/db.rs`：SQLite schema、兼容迁移和读写。
- `backend/src/scanner.rs`：递归扫描、过滤和合集分组。
- `backend/src/media.rs`：ffprobe 媒体与音轨解析。
- `backend/src/extractor.rs`：FFmpeg、TTS 和任务进度。
- `backend/src/sorter.rs`：集数解析、标题清理和文件名排序。
- `backend/src/models.rs`：Serde API 模型。

## 3. 扫描规则

默认视频扩展名：

```text
.mp4 .mkv .avi .mov .wmv .flv .webm
.m4v .mpg .mpeg .ts .m2ts .vob
```

规则：

1. 目录递归扫描，单文件直接分析。
2. 隐藏文件不在文件浏览器中显示。
3. 根据扩展名白名单、忽略后缀和最小文件大小过滤。
4. 每个包含视频的父目录形成一个合集。
5. `第一季` 至 `第五季` 目录会与父目录名称组合。
6. 集数按 `S01E02`、`第02集`、独立数字依次解析。
7. 无法执行 ffprobe 时仍保留视频记录，并返回明确警告。

音轨的 `index` 是 FFmpeg 全局 stream index。提取和试听必须使用：

```text
-map 0:{index}
```

## 4. 输出与排序

输出目录：

```text
{output_directory}/{安全合集名}/
```

输出示例：

```text
000_萌鸡小队第一季.mp3
001_植树节.mp3
002_找妈妈.mp3
...
```

约束：

- 合集片头固定使用 `000_`。
- 少于 1000 集时使用三位序号，1000 集起使用四位。
- 可显式配置序号位数。
- 标题去除集数、合集名和常见编码/清晰度标签。
- 移除路径分隔符和非法文件名字符，防止输出目录逃逸。
- 支持 NTFS/FAT 字符排序、自然数字排序和名称排序。

## 5. 提取任务

任务状态：

```text
queued → processing → completed | failed | cancelled
```

处理流程：

1. 创建任务及逐视频任务项。
2. 根据集数和文件系统策略排序。
3. 可选生成 Piper 或静音片头。
4. 逐文件调用 FFmpeg，更新进度和当前文件。
5. 保存成功数、失败数、输出文件和错误摘要。

取消任务会立即保存 `cancelled` 状态；正在运行的单次 FFmpeg 调用完成后，后台循环停止处理后续文件。运行中的任务不能直接删除，必须先取消。

## 6. TTS

支持的 provider：

- `piper`：离线 Piper CLI 和 ONNX 模型。
- `silent`：生成一秒静音片头。
- `disabled`：不生成片头。

默认模型名称为 `zh_CN-huayan-medium`，容器内模型目录为：

```text
/app/data/piper-voices
```

Piper 失败策略：

- `silent`：生成静音占位。
- `skip`：跳过片头。
- `fail`：终止任务。

## 7. API

基础路径：`/api/v1`

```text
GET    /files?path=...
POST   /scan/start
GET    /collections
GET    /collections/{id}
POST   /collections/{id}/scan
DELETE /collections/{id}

POST   /extract
GET    /extract/jobs
GET    /extract/jobs/{id}
POST   /extract/jobs/{id}/cancel
DELETE /extract/jobs/{id}
GET    /extract/jobs/{job_id}/items/{item_id}/audio
GET    /preview/{video_id}?track=...&duration=...&start=...

GET    /settings
PUT    /settings
GET    /system/status
```

音频响应支持 HTTP Range，保证浏览器能够拖动播放进度。

## 8. 数据与配置

SQLite 默认路径：`/app/data/vid2audio.db`。

数据表：

- `collections`
- `video_files`
- `audio_tracks`
- `extract_jobs`
- `extract_job_items`
- `settings`

数据库启用 foreign keys、WAL、15 秒 busy timeout 和 normal synchronous。Rust 后端沿用原有 SQLite 表名和字段，现有数据库无需转换。

环境变量：

```text
VID2AUDIO_DB
VID2AUDIO_INPUT
VID2AUDIO_OUTPUT
VID2AUDIO_STATIC
VID2AUDIO_BIND
RUST_LOG
```

## 9. Docker 与发布

默认 Dockerfile 使用三个阶段：

1. Node 构建 Vue 静态资源。
2. Rust 构建并 strip release 二进制。
3. `debian:bookworm-slim` 安装 FFmpeg，复制二进制和静态资源。

默认 GHCR 镜像内置 `ffmpeg` 和 `ffprobe`，不需要挂载宿主机二进制：

```text
ghcr.io/wenfer/vid2audio:v0.2.0
ghcr.io/wenfer/vid2audio:latest
```

版本遵循 SemVer：Cargo 与前端为 `X.Y.Z`，Git 标签与镜像标签为 `vX.Y.Z`。发布工作流校验两者一致，并同时更新 `latest`。两个标签指向同一份 `linux/amd64`、`linux/arm64` manifest，不生成提交哈希或 `-ffmpeg` 别名。

## 10. 必须保持的质量约束

- FFmpeg/ffprobe 缺失时返回清晰错误，服务本身继续运行。
- 不得把输入目录写入容器；视频挂载保持只读。
- 不得允许合集名或任务名逃逸输出根目录。
- 重新扫描和删除合集不得破坏运行中的任务。
- SQLite schema 变更必须向后兼容。
- 排序、扫描、设置 API 和音频 Range 行为必须有 Rust 测试。

验证命令：

```bash
cargo fmt --manifest-path backend/Cargo.toml -- --check
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
cargo build --manifest-path backend/Cargo.toml --locked --release
```

前端变更后还需执行：

```bash
cd frontend && npm run build
```

## 11. 当前不包含的功能

以下能力不属于当前实现，不能在部署文档中当作已支持功能描述：

- Redis/Celery 或分布式任务队列。
- PostgreSQL。
- 云端 Edge-TTS、Coqui、百度或阿里 TTS。
- 自动定时扫描、增量扫描或文件系统监听。
- 多用户、权限管理和媒体服务器集成。
- 单个视频按章节自动拆分。
