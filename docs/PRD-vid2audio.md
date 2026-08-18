# Vid2Audio 产品与实现说明

> 当前版本：0.2.11
>
> 目标平台：Docker/NAS 服务；Windows 与 macOS 桌面版（Tauri v2）
> Linux 桌面安装包尚未产出，见第 13 节。
> 本文只描述仓库当前实现和必须保持的产品约束。

## 1. 产品目标

Vid2Audio 将视频合集转换为适合儿童故事机、早教机和 U 盘播放的音频包。

核心能力：

- 浏览文件系统并选择目录或单个视频。
- 使用 `ffprobe` 识别视频、时长和音轨。
- 选择 FFmpeg stream index 对应的音轨并导出音频。
- 支持 MP3、M4A、AAC、OGG、FLAC、WAV、OPUS。
- 支持开头、结尾裁剪和 10 秒试听。
- 生成带前导零的安全文件名，保证故事机播放顺序。
- 「FAT 排序」重排目录项的物理写入顺序，修正按写入顺序播放的设备上的错序。
- 内置文件管理：复制、移动、重命名、删除，以及打包成 ZIP（浏览器下载或桌面版另存为）。
- 使用 Piper 生成离线合集片头；失败时可静音占位、跳过或终止。
- 保存合集、全局设置和提取任务，展示逐文件结果。
- 提取任务可暂停和继续；进程重启后中断的任务不会卡死。
- 同一套后端与界面同时以 Docker 服务、Windows 与 macOS 桌面程序发布。

## 2. 两种发布形态

| | 服务端（Docker/NAS） | 桌面版（Windows / macOS） |
| --- | --- | --- |
| 进程 | `vid2audio` 单进程 | `vid2audio-desktop`（Tauri 外壳 + 同一份 router） |
| 界面 | 浏览器访问 `http://host:8000` | 内嵌 WebView2 / WKWebView |
| 请求通道 | TCP（默认 `127.0.0.1:8000`） | `v2a://` 自定义 URI scheme，**不开端口** |
| FFmpeg | 镜像内置 Debian 包 | 随包分发 LGPL 构建（Windows 下载 BtbN 构建，macOS 从源码编译） |
| 数据库 | `/app/data/vid2audio.db` | Windows `%LOCALAPPDATA%\vid2audio\vid2audio.db` / macOS `~/Library/Application Support/vid2audio/vid2audio.db` |
| 前端代码 | 完全相同 | 完全相同（差异集中在 `src/desktop.ts`） |

两种形态共用 `vid2audio::build_router`，行为不会漂移。

## 3. 当前架构

```text
Vue 3 静态界面
       │ REST / 音频 Range
       ├── 服务端：TCP → axum::serve
       └── 桌面版：v2a:// → vid2audio-bridge → Router::oneshot
       ▼
Rust + Axum + Tokio
       ├── SQLite：设置、合集、任务
       ├── ffprobe：媒体分析
       ├── ffmpeg：试听、裁剪、转码
       └── Piper：可选离线 TTS
```

后端是单个 Rust 进程，不依赖 Redis、Celery、PostgreSQL 或独立 Worker。提取任务在 Tokio 后台任务中运行，全局并发数默认为 2，可在系统配置中调整（范围 1–32）。

主要代码：

- `backend/src/lib.rs`：模块导出、`build_router`、数据库与静态资源路径解析。服务端与桌面版共用。
- `backend/src/main.rs`：服务端进程入口和 TCP 绑定。
- `backend/src/api.rs`：REST API、静态资源、音频 Range 响应、文件管理与打包。
- `backend/src/db.rs`：SQLite schema、兼容迁移、启动自检和读写。
- `backend/src/scanner.rs`：递归扫描、过滤和合集分组。
- `backend/src/media.rs`：ffprobe 媒体与音轨解析。
- `backend/src/extractor.rs`：FFmpeg、TTS 和任务进度。
- `backend/src/platform.rs`：所有跨平台差异（路径默认值、`~` 展开、命令查找、盘符、隐藏文件、文件名规则）。
- `backend/src/sorter.rs`：集数解析、标题清理和文件名排序。
- `backend/src/models.rs`：Serde API 模型。
- `bridge/src/lib.rs`：`vid2audio-bridge`，WebView 请求与 Axum router 的转发层。刻意不引用 `tauri`，因此在没有 GTK/WebView2 的机器上也能跑单元测试。
- `src-tauri/src/lib.rs`：桌面外壳的运行时装配、协议注册和窗口创建。
- `frontend/src/desktop.ts`：浏览器与桌面版的全部行为差异。

## 4. 桌面版设计要点

桌面外壳**不启动 TCP 服务**。它注册 `v2a://` 自定义 URI scheme，把每个 WebView 请求通过 `Router::oneshot` 交给服务端同一份 Axum router。改动这一层前需要知道：

- 前端零改动。请求都是相对路径（`/api/v1/...`），页面本身也由这个 router 提供（`/` 与 `/static/*`），所以 `fetch`、`<audio src>`、下载链接自然落到同一 scheme。
- 不用 localhost 服务器是出于安全考虑：`/api/v1/files/delete` 一类接口能改任意路径，而一旦监听端口，任何网页的 JS 都能打进来。自定义 scheme 只有本进程的 WebView 能访问。
- 不把现有接口改写成 `#[tauri::command]`：返回 URL 的接口（音频流、试听、ZIP 打包）走 JSON-RPC IPC 无法工作。
- 代价是响应体要完整缓冲成 `Vec<u8>`，WebView 自定义协议接口没有流式形式。Range 请求仍然可用——状态码和响应头原样透传，`bridge/` 有对应测试。
- Windows 与 Android 把 scheme 映射为 `http://v2a.localhost/`，macOS 与 Linux 是 `v2a://localhost/`。由 `bridge::entry_url` 处理，两种形态都不能写死。
- tokio runtime 刻意用 `Box::leak` 泄漏。drop 它会静默取消正在跑的提取任务，而且 `Runtime::drop` 在异步上下文里会 panic。

工程上没有任何 `#[tauri::command]`。界面需要的一切都已经是 HTTP 路由，因此 `src-tauri/capabilities/default.json` 只放开原生对话框（`dialog:allow-open`、`allow-save`、`allow-confirm`）和 `opener:allow-reveal-item-in-dir`。**更小的 IPC 暴露面是这套设计的主要安全收益**，除非某个需求确实无法表达成路由，否则不要往里加权限。

界面差异集中在 `frontend/src/desktop.ts`，运行时靠 `'__TAURI_INTERNALS__' in window` 判定（绝不能看 User-Agent，WebView2 的 UA 就是 Edge 的）。Tauri 的 npm 包用动态 `import()` 引入，产物落在独立 chunk 里，Docker/浏览器部署永不请求。差异项：

- `window.prompt` 在 WebView2 里**没有实现**，静默返回 null。文本输入统一走 `usePrompt` + `PromptModal.vue`。
- `window.confirm` 可用，但渲染成页面来源的弹窗（`v2a.localhost 显示…`）。统一走 `confirmAction`，桌面版切到原生对话框。
- `<a download>` 在 WebView 里存不到用户指定的位置。桌面版改为「另存为」对话框加 `POST /files/archive-to`，由后端流式写盘。
- 路径输入框只在桌面版显示 📂 按钮；网页拿不到真实文件系统路径。
- 文件浏览器顶部显示盘符快捷入口，数据来自 `platform::filesystem_roots`。

随包 FFmpeg 由 `src-tauri/scripts/fetch_ffmpeg.py` 下载到 `binaries/`（不提交），外壳启动时用 `platform::set_bundled_bin_dir` 告知后端。查找顺序是随包目录 → exe 同级 → `PATH`，用户自己装的 ffmpeg 也能用。

## 5. 扫描规则

默认视频扩展名：

```text
.mp4 .mkv .avi .mov .wmv .flv .webm
.m4v .mpg .mpeg .ts .m2ts .vob
```

规则：

1. 目录递归扫描，单文件直接分析。
2. 隐藏文件不在文件浏览器中显示（Windows 上同时看隐藏属性，不只看点开头）。
3. 根据扩展名白名单、忽略后缀和最小文件大小过滤。
4. 每个包含视频的父目录形成一个合集。
5. `第一季` 至 `第五季` 目录会与父目录名称组合。
6. 集数按 `S01E02`、`第02集`、独立数字依次解析。
7. 无法执行 ffprobe 时仍保留视频记录，并返回明确警告。

音轨的 `index` 是 FFmpeg 全局 stream index。提取和试听必须使用：

```text
-map 0:{index}
```

## 6. 输出与排序

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

### FAT 排序

前导零文件名只能保证按名排序的设备正确播放。故事机、车机和老 U 盘播放器按 FAT 目录表里**目录项的物理写入顺序**播放，用户在文件管理器里增删文件后顺序仍可能错乱。

`POST /files/fat-sort` 通过同盘重命名重写目录项顺序：在目标目录**同级**建临时目录 `.vid2audio-fatsort.tmp`，把条目按自然数字顺序依次 `rename` 进去（新目录的目录项按创建顺序分配），删掉已清空的原目录，再把临时目录改名回原名。

- 只改变条目顺序，不改文件名、不改内容，不需要特权容器或块设备访问。
- 只处理所选目录的直接子项，子目录整体移动，内部顺序不变。
- 任何一步失败都按反序回滚并删除临时目录。
- 临时目录若因崩溃残留，下次调用会尝试恢复；原目录非空且临时目录也存在时报错要求人工处理，绝不自动删用户文件。
- 在 NTFS/ext4 上是无害的空操作（这些文件系统不按写入顺序遍历）。

## 7. 提取任务

任务状态：

```text
queued → processing → completed | failed | cancelled | paused
```

处理流程：

1. 创建任务及逐视频任务项，并把原始 `ExtractRequest` 存进 `extract_jobs.request`。
2. 根据集数和文件系统策略排序。
3. 可选生成 Piper 或静音片头。
4. 逐文件调用 FFmpeg，更新进度和当前文件。
5. 保存成功数、失败数、输出文件和错误摘要。

取消与暂停都是**协作式**的：只翻转状态，后台循环在下一个文件边界退出，正在转码的那个文件会先写完。运行中的任务不能直接删除，必须先取消或暂停。

继续任务会重放持久化的 `ExtractRequest`，跳过已标记 `completed` 的项。序号来自完整有序列表而不是剩余工作量，所以前导零编号跨暂停/继续保持稳定。

每次运行从 `begin_run` 领一个 epoch，发现自己 epoch 过期的 worker 静默退出。这是防止暂停窗口期内下发的「继续」导致两个 worker 同时处理同一批文件的机制。

进程重启后，内存里的任务全部消失，但库里的状态还停在 `queued`/`processing`——这类任务在界面上既不前进也删不掉。`recover_interrupted_jobs` 在启动时把它们改成 `paused`，正在处理的条目退回 `pending`，用户可以继续或删除。

## 8. TTS

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

桌面安装包不随包分发 Piper 和语音模型（模型体积远超程序本身）。默认设置仍是 `piper` 加失败策略 `silent`，所以没装 Piper 的机器上片头会退化成 1 秒静音而不是任务失败；需要真实片头就自行安装 Piper 并在设置里填路径。

## 9. API

基础路径：`/api/v1`

```text
GET    /files?path=...
POST   /files/copy
POST   /files/move
POST   /files/rename
POST   /files/delete
POST   /files/fat-sort
GET    /files/archive?path=...
POST   /files/archive-to

POST   /scan/start
GET    /collections
GET    /collections/{id}
POST   /collections/{id}/scan
DELETE /collections/{id}

POST   /extract
GET    /extract/jobs
GET    /extract/jobs/{id}
POST   /extract/jobs/{id}/cancel
POST   /extract/jobs/{id}/pause
POST   /extract/jobs/{id}/resume
DELETE /extract/jobs/{id}
GET    /extract/jobs/{job_id}/items/{item_id}/audio
GET    /preview/{video_id}?track=...&duration=...&start=...

GET    /settings
PUT    /settings
GET    /system/status
```

约定：

- 音频响应支持 HTTP Range，保证浏览器能够拖动播放进度。
- `GET /files` 同时返回 `roots`（主目录与各盘符），供界面显示快捷入口。
- `GET /files/archive` 把 ZIP 作为响应体返回，用于浏览器下载；`POST /files/archive-to` 直接写到指定路径，用于桌面版另存为。两者共用同一套写入逻辑，逐文件流式拷贝，不把整个压缩包攒在内存里。
- 返回给前端的每个路径都过 `canonical`，其中调用 `platform::strip_extended_prefix` 去掉 Windows 的 `\\?\` 前缀。
- 默认只允许同源跨域。`VID2AUDIO_CORS_ORIGINS` 可以逗号分隔地追加来源（前端 dev server 用）。

## 10. 数据与配置

SQLite 默认路径：容器 `/app/data/vid2audio.db`，Windows 桌面版 `%LOCALAPPDATA%\vid2audio\vid2audio.db`，macOS 桌面版 `~/Library/Application Support/vid2audio/vid2audio.db`。

数据表：

- `collections`
- `video_files`
- `audio_tracks`
- `extract_jobs`
- `extract_job_items`
- `settings`

数据库启用 foreign keys、WAL、15 秒 busy timeout 和 normal synchronous。Rust 后端沿用原有 SQLite 表名和字段，现有数据库无需转换。`extract_jobs.request` 是为「继续任务」新增的列，走同一套「缺列就 ALTER TABLE」的兼容迁移。

`CREATE TABLE IF NOT EXISTS` 不会升级已存在的表，所以老库可能缺少外键级联，删除时会报 `FOREIGN KEY constraint failed`。启动时 `clear_dangling_references` 清理父行已删、子行残留的悬空引用；`db.rs` 里的删除逻辑也显式删子行，不依赖 `ON DELETE CASCADE`。

平台相关的默认路径：

| | 容器 | Windows 桌面版 | macOS 桌面版 |
| --- | --- | --- | --- |
| 数据 | `/app/data` | `%LOCALAPPDATA%\vid2audio` | `~/Library/Application Support/vid2audio` |
| 扫描 | `/app/input` | `%USERPROFILE%\Videos` | `~/Videos` |
| 输出 | `/app/output` | `%USERPROFILE%\Music\Vid2Audio` | `~/Music/Vid2Audio` |

环境变量：

```text
VID2AUDIO_DB
VID2AUDIO_INPUT
VID2AUDIO_OUTPUT
VID2AUDIO_STATIC
VID2AUDIO_BIND
VID2AUDIO_EXTRACTION_CONCURRENCY
VID2AUDIO_CORS_ORIGINS
VID2AUDIO_BIN_DIR
RUST_LOG
```

## 11. Docker 与发布

默认 Dockerfile 使用三个阶段：

1. Node 构建 Vue 静态资源。
2. Rust 构建并 strip release 二进制。
3. `debian:bookworm-slim` 安装 FFmpeg，复制二进制和静态资源。

镜像内置 `ffmpeg` 和 `ffprobe`，不需要挂载宿主机二进制。**CI 不再发布镜像**（`docker-ghcr.yml` 已删除，不要用空 `on: []` 加回来——GitHub 会把空触发列表解析成非法的活动工作流）；Docker 镜像用本地构建：

```bash
docker compose -f docker/docker-compose.yml up --build
```

版本遵循 SemVer：Cargo 与前端为 `X.Y.Z`，Git 标签为 `vX.Y.Z`。桌面发布工作流校验两者一致。`bridge/`、`src-tauri/` 与 `frontend/` 的版本号跟随 `backend/Cargo.toml`。

`backend/` 保持独立 crate 而不是 workspace 成员：`docker/Dockerfile` 单独复制 `backend/Cargo.toml` 和 `backend/Cargo.lock` 做依赖缓存层，加个 workspace 根会破坏这一层。

桌面版打包：

```bash
cargo install tauri-cli --version '^2'
cd src-tauri
python3 scripts/fetch_ffmpeg.py           # Windows：下载 BtbN 的 LGPL FFmpeg 到 binaries/
python3 scripts/build_ffmpeg_macos.py     # macOS：从源码编译 LGPL 静态 ffmpeg/ffprobe 到 binaries/
cargo tauri build                         # Windows NSIS / macOS .app + dmg
```

`beforeBuildCommand` 会自动执行前端构建。Windows 安装模式是 `currentUser`，不需要管理员权限。

没有对应平台机器时走 CI：`.github/workflows/desktop-release.yml` 在各自的原生 runner 上构建 Windows NSIS 安装包和 macOS 两个架构的 DMG，`v*.*.*` tag 或手动 dispatch 触发，产物作为 artifact 上传后统一发布为 GitHub Release（含各平台 `.sig` 合并的 updater `latest.json`）。macOS 的 FFmpeg 源码编译产物按脚本内容缓存，版本与脚本不变时不重复编译。Tauri 在 Linux 上只支持被官方称为「最后手段」的 NSIS 交叉编译（MSI 需要 WiX，只能在 Windows 运行），所以不在开发机上做这件事。

## 12. 必须保持的质量约束

- FFmpeg/ffprobe 缺失时返回清晰错误，服务本身继续运行。
- 不得把输入目录写入容器；视频挂载保持只读。
- 不得允许合集名或任务名逃逸输出根目录。
- 重新扫描和删除合集不得破坏运行中的任务。
- SQLite schema 变更必须向后兼容。
- 排序、扫描、设置 API 和音频 Range 行为必须有 Rust 测试。
- 跨平台差异集中在 `platform.rs`，不要散落 `cfg!(windows)` 判断。
- `platform::reject_windows_unsafe_name` 在**所有**平台执行。`:` 是原因：Windows 的 `PathBuf::push` 遇到盘符相对前缀会替换整个路径，把文件重命名成 `C:evil` 就逃出了父目录，而 `a.mp3:hidden` 写的是 NTFS 备用数据流。
- `reorder_directory_fat` 只改目录项顺序，失败必须完整回滚。
- 桌面版不新增 `#[tauri::command]`，IPC 白名单保持最小。
- 承载分析结果或向导输入的弹窗不响应背景点击——误点会丢掉没法找回的工作。

验证命令：

```bash
cargo fmt --manifest-path backend/Cargo.toml -- --check
cargo test --manifest-path backend/Cargo.toml --locked
cargo clippy --manifest-path backend/Cargo.toml --locked --all-targets -- -D warnings
cargo build --manifest-path backend/Cargo.toml --locked --release
```

改动 `bridge/` 或 `src-tauri/` 时追加：

```bash
cargo test --manifest-path bridge/Cargo.toml --locked
cargo clippy --manifest-path src-tauri/Cargo.toml --locked -- -D warnings
```

没有 GTK/WebView2 开发库的 Linux 机器编译不了 `src-tauri` 的本机目标，改为对 Windows 目标做类型检查。必须用 **gnu** 目标而不是 msvc：仓库依赖 `libsqlite3-sys`，交叉编译 `sqlite3.c` 需要 Linux 上没有的 MSVC C 头文件。

```bash
rustup target add x86_64-pc-windows-gnu
cargo clippy --manifest-path backend/Cargo.toml --locked --target x86_64-pc-windows-gnu -- -D warnings
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --target x86_64-pc-windows-gnu -- -D warnings
```

backend 的这一项和外壳同样重要：`platform.rs` 里有真实的 `#[cfg(windows)]` 实现（`filesystem_roots` 背后的 `GetLogicalDrives` FFI），本机构建从不编译它们。

前端变更后还需执行：

```bash
cd frontend && npm run build
```

## 13. 当前不包含的功能

以下能力不属于当前实现，不能在部署文档中当作已支持功能描述：

- Redis/Celery 或分布式任务队列。
- PostgreSQL。
- 云端 Edge-TTS、Coqui、百度或阿里 TTS。
- 自动定时扫描、增量扫描或文件系统监听。
- 多用户、权限管理和媒体服务器集成。
- 单个视频按章节自动拆分。
- 块设备级 `fatsort`（需要特权容器）；FAT 排序只做同盘重命名。
- FAT 排序递归处理子目录，只处理所选目录的直接子项。
- Linux 桌面安装包。外壳代码已经按平台分支写好（`bridge::entry_url` 覆盖两种 scheme 形态），但尚未在 Linux 上构建和验证过。
- 桌面版的进度推送。任务进度仍靠前端轮询 `GET /extract/jobs`，没有走 Tauri Channel。
- 桌面版尚未收紧 `tauri.conf.json` 的 `security.csp`（当前为 `null`）。
- Windows 安装包尚未在真机上完成手工验证：音频试听拖动、另存为打包、文件夹选择、盘符入口、重命名/移动对话框都只经过代码审查和交叉类型检查。macOS 版已在真机（Apple Silicon）上验证启动与数据落位。
