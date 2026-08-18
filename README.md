# Vid2Audio

把儿童视频合集批量转成**故事机 / 车机 / U 盘**直接能播的音频包。

扫描视频目录，自动识别合集与集数，用 FFmpeg 提取所选音轨，输出带前导零序号的安全文件名，并按 FAT 目录项顺序重排——开箱即用，不用在播放器上一集一集翻。

同一套 Rust/Axum 后端与 Vue 3 界面，提供三种运行形态：**浏览器访问的 NAS/Docker 服务**、**Windows 桌面版**、**macOS 桌面版**。

## 特性

- **合集识别**：递归扫描、`S01E02` / `第02集` / 独立数字集数解析、季节目录合并
- **多音轨**：ffprobe 解析全部音轨，按需选择；10 秒试听、开头 / 结尾裁剪
- **格式**：MP3 / M4A / AAC / OGG / FLAC / WAV / OPUS
- **故事机排序**：前导零文件名（`000_合集名.mp3`）+ 「FAT 排序」重排目录项物理写入顺序，修正按写入顺序播放的设备错序
- **文件管理器**：浏览、复制、移动、重命名、删除、ZIP 打包下载
- **离线 TTS 片头**：Piper 离线生成合集片头；未装 Piper 时自动降级为 1 秒静音占位
- **任务管理**：逐文件进度、成功/失败简报、暂停 / 继续、重启后断点恢复
- **安全设计**：桌面版不开 TCP 端口，WebView 请求走自定义 `v2a://` 协议直通后端，别的程序与网页碰不到本机文件接口

## 运行形态

| | 服务端（Docker / NAS） | 桌面版（Windows / macOS） |
| --- | --- | --- |
| 界面 | 浏览器访问 `http://host:8000` | 内嵌 WebView |
| 请求通道 | TCP（默认 `8000`） | `v2a://` 自定义协议，**不开端口** |
| FFmpeg | 镜像内置 Debian 包 | 随包分发的 LGPL 构建（Windows 下载、macOS 源码编译） |
| 数据库 | `/app/data/vid2audio.db` | Windows `%LOCALAPPDATA%\vid2audio` / macOS `~/Library/Application Support/vid2audio` |
| 前端代码 | 完全相同 | 完全相同（差异集中在 `frontend/src/desktop.ts`） |

## 下载

安装包发布在 [GitHub Releases](https://github.com/wenfer/vid2audio/releases)：

- `vid2audio-<版本>-windows-x64-setup.exe`（Windows 10/11）
- `vid2audio-<版本>-macos-aarch64.dmg`（Apple Silicon）与 `-macos-x86_64.dmg`（Intel），最低 macOS 11

桌面版设置页提供「检查更新 / 立即更新」，自动检查并下载安装新版本。未配置 Apple 证书的构建使用 ad-hoc 签名，macOS 首次打开需在 Finder 中右键 → 打开。

## 快速开始

### Docker / NAS

```bash
# 先按需调整 docker/docker-compose.yml 中的挂载路径
docker compose -f docker/docker-compose.yml up --build
```

打开 `http://localhost:8000`。镜像基于 Debian，内置 `ffmpeg` / `ffprobe`，无需宿主机安装。容器内路径：

- 输入视频：`/app/input`（建议只读挂载）
- 音频输出：`/app/output`
- 数据与数据库：`/app/data`

### 本地开发

需要本机安装 `ffmpeg` 与 `ffprobe`。

```bash
cd frontend
npm ci
npm run build          # 产物输出到 backend/static/

cd ../backend
VID2AUDIO_DB=../data/vid2audio.db \
VID2AUDIO_INPUT=/path/to/videos \
VID2AUDIO_OUTPUT=/path/to/output \
cargo run
```

打开 http://127.0.0.1:8000。

默认同时运行 2 个提取任务，可在「系统配置」调整（1–32），或用 `VID2AUDIO_EXTRACTION_CONCURRENCY` 设置初始值。

## 典型使用流程

1. **扫描**：在文件浏览器中选择视频文件夹或单个文件，发起扫描。
2. **分析**：查看合集、集数、音轨列表（分析弹窗可直接复用为提取表单）。
3. **试听与裁剪**：选择音轨试听 10 秒，设置开头 / 结尾偏移。
4. **提取**：选择输出格式与排序策略，创建任务并跟踪进度。
5. **打包**：把输出目录打包下载（桌面版走系统「另存为」）。
6. **排错**：若故事机仍按写入顺序播放，对该目录执行「FAT 排序」。

## 排序与 TTS

系统配置中可选择排序策略：

- **NTFS/FAT 兼容排序**（默认）：面向故事机 / U 盘 / NAS，配合前导零文件名保证播放顺序
- **自然数字排序**：贴近桌面文件管理器的数字顺序
- **按名称排序**：大小写折叠后按名称排序

片头支持三种模式：

- **Piper 离线 TTS**（推荐）：完全离线，适合 NAS。需自备 Piper 二进制与中文语音模型（默认 `zh_CN-huayan-medium`）。Docker 中挂载 `/usr/local/bin/piper:/app/bin/piper:ro` 与模型目录到 `/app/data/piper-voices:ro`
- **静音占位**：生成 1 秒静音片头（Piper 失败时的默认降级策略）
- **禁用片头**：不生成片头

## 桌面版架构

桌面版复用同一份 `build_router`，**不监听任何 TCP 端口**：注册 `v2a://` 自定义 URI scheme，把每个 WebView 请求通过 `Router::oneshot` 直通 Axum router。

- 前端零改动，请求与页面都走同一 scheme，`fetch` / `<audio>` / 下载链接无需特判
- 不开端口 = 其他程序与网页无法触达 `/api/v1/files/delete` 这类改文件的接口
- IPC 白名单最小化：`capabilities/default.json` 只放开原生对话框与「在文件管理器中显示」
- 浏览器与桌面差异集中在 `frontend/src/desktop.ts`（系统文件夹对话框、另存为、原生确认框等）

## 构建与发布

CI（`.github/workflows/desktop-release.yml`）在各平台原生 runner 上构建：

- Windows：NSIS 安装包
- macOS：arm64 / x86_64 两个 DMG + updater 归档（macOS 的 LGPL FFmpeg 在 CI 从源码编译，按脚本内容缓存，版本不变不会重复编译）

推送 `v*.*.*` 标签（必须等于 `src-tauri/tauri.conf.json` 的版本号）即自动构建并发布 GitHub Release；也可以在 Actions 页面手动 dispatch。

本地打包（可选）：

```bash
cargo install tauri-cli --version '^2'
cd src-tauri
python3 scripts/fetch_ffmpeg.py           # Windows：下载 LGPL FFmpeg 到 binaries/
python3 scripts/build_ffmpeg_macos.py     # macOS：源码编译 LGPL FFmpeg
cargo tauri build                         # Windows NSIS / macOS .app + dmg
```

Docker 镜像由本地 `docker compose up --build` 构建，CI 不再发布镜像。

## API

基础路径：`/api/v1`

```text
GET    /files?path=...                    文件浏览
POST   /files/copy|move|rename|delete     文件管理
POST   /files/fat-sort                    FAT 目录项排序
GET    /files/archive                     打包下载（浏览器）
POST   /files/archive-to                  打包写入指定路径（桌面版另存为）

POST   /scan/start                        发起扫描
GET    /collections                       合集列表
GET    /collections/{id}                  合集详情
POST   /collections/{id}/scan             重新扫描合集
DELETE /collections/{id}                  删除合集

POST   /extract                           创建提取任务
GET    /extract/jobs                      任务列表
GET    /extract/jobs/{id}                 任务详情
DELETE /extract/jobs/{id}                 删除任务
POST   /extract/jobs/{id}/cancel|pause|resume
GET    /extract/jobs/{job_id}/items/{item_id}/audio   单文件音频
GET    /preview/{video_id}?track=...&duration=...&start=...   试听

GET    /settings                          系统配置
PUT    /settings
GET    /system/status                     运行状态
```

音频接口支持 HTTP Range，可拖动播放进度。

## 目录结构

```text
backend/     Rust + Axum 后端（路由、SQLite、扫描、提取、排序、FFmpeg）
frontend/    Vue 3 + Vite + TypeScript 界面
src-tauri/   Tauri v2 桌面外壳（Windows / macOS）与打包脚本
bridge/      WebView 自定义协议 ↔ Axum router 转发层（不依赖 tauri）
docker/      Dockerfile 与 docker-compose
docs/        产品与实现说明（PRD）
```

## 文档

- [docs/PRD-vid2audio.md](docs/PRD-vid2audio.md)：产品约束、架构与实现细节
- [AGENTS.md](AGENTS.md)：仓库约定与开发命令

## 许可证

暂未指定开源许可证，保留所有权利。