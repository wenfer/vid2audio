# Vid2Audio - 儿童故事机音频资源生产系统

> **产品需求文档 (PRD)**  
> **版本**: v1.0  
> **日期**: 2026-04-27  
> **用途**: 指导开发实现，供多会话/多Agent协作参考  
> **目标平台**: Docker容器，运行于NAS系统

---

## 1. 产品概述

### 1.1 产品定位

Vid2Audio是一款运行在NAS上的Docker应用，面向有儿童的家庭用户。产品自动扫描用户存储的视频资源（动画片、教育视频等），从中提取音频轨道，生成适合儿童故事机/早教机播放的音频资源包。

### 1.2 核心价值

- **零配置自动化**: 扫描指定目录，自动识别视频合集
- **智能音频提取**: 支持多音轨选择、试听、格式转换
- **故事机适配**: 生成合集语音提示，适配文件系统排序规则
- **即导即用**: 输出目录可直接复制到故事机SD卡/U盘使用

### 1.3 目标用户

- 有NAS设备的技术型家长
- 希望为孩子制作音频资源的父母
- 使用儿童故事机/早教机的家庭

### 1.4 使用场景示例

**场景1 - 萌鸡小队第一季提取**
```
输入目录: /videos/萌鸡小队/第一季/
├── 萌鸡小队.S01E01.植树节.mp4
├── 萌鸡小队.S01E02.找妈妈.mp4
├── ... (共52集)

输出目录: /output/萌鸡小队第一季/
├── 000_萌鸡小队第一季.mp3    ← TTS生成的合集提示音
├── 001_植树节.mp3
├── 002_找妈妈.mp3
├── ...
```

**场景2 - 多语言音轨选择**
```
输入: 某动画片包含中文、英文两条音轨
操作: 用户在Web界面选择"中文"音轨提取
输出: 仅包含中文音频的文件
```

---

## 2. 系统架构设计

### 2.1 总体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                         Docker Container                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │   Web UI     │  │   API Server │  │   Background Worker  │  │
│  │  (Frontend)  │◄─┤  (Backend)   │◄─┤   (Celery/RQ)        │  │
│  │  React/Vue   │  │  FastAPI     │  │   FFmpeg Processing  │  │
│  └──────────────┘  └──────────────┘  └──────────────────────┘  │
│         ▲                 ▲                    ▲                │
│         │                 │                    │                │
│    WebSocket         REST API            File System            │
│         │                 │                    │                │
│         └─────────────────┴────────────────────┘                │
│                              │                                  │
│                    ┌─────────┴─────────┐                        │
│                    │   SQLite/PostgreSQL│                        │
│                    │   (Metadata DB)    │                        │
│                    └───────────────────┘                        │
└─────────────────────────────────────────────────────────────────┘
                              │
         ┌────────────────────┼────────────────────┐
         ▼                    ▼                    ▼
   /input/videos      /output/audio         /config
   (视频源目录)       (音频输出目录)        (配置文件)
```

### 2.2 技术栈建议

| 层级 | 技术选型 | 说明 |
|------|---------|------|
| **前端** | React 18 + Ant Design / Vue 3 + Element Plus | 现代化UI框架，组件丰富 |
| **后端** | Python + FastAPI | 高性能异步框架，自动API文档 |
| **任务队列** | Celery + Redis / RQ + Redis | 异步处理视频转码任务 |
| **数据库** | SQLite (默认) / PostgreSQL (高级) | 轻量级默认配置，支持扩展 |
| **媒体处理** | FFmpeg + ffprobe | 视频解析、音频提取、格式转换 |
| **TTS引擎** | Edge-TTS (免费) / Coqui TTS | 生成合集提示音 |
| **容器化** | Docker + Docker Compose | 一键部署，NAS友好 |
| **进程管理** | Supervisor / PM2 | 容器内进程守护 |

### 2.3 目录挂载设计

```yaml
# docker-compose.yml 建议配置
volumes:
  - /nas/videos:/app/input:ro        # 视频源目录（只读）
  - /nas/audio-output:/app/output    # 音频输出目录
  - ./config:/app/config             # 配置文件
  - ./data:/app/data                 # 数据库持久化
```

---

## 3. 核心功能模块

### 3.1 模块总览

```
┌────────────────────────────────────────────────────────────┐
│                      功能模块架构                            │
├──────────────┬──────────────┬──────────────┬───────────────┤
│  扫描模块     │  提取模块     │  TTS模块      │  排序适配模块  │
│  Scanner     │  Extractor   │  TTS Engine  │  Sorter       │
├──────────────┼──────────────┼──────────────┼───────────────┤
│ • 目录监控    │ • 音轨分析    │ • 文本转语音  │ • NTFS排序    │
│ • 视频识别    │ • 音轨试听    │ • 语音合成    │ • 序号补零    │
│ • 合集分组    │ • 格式转换    │ • 音量标准化  │ • 乱序修复    │
│ • 元数据提取  │ • 质量设置    │ • 合并插入    │ • 输出验证    │
└──────────────┴──────────────┴──────────────┴───────────────┘
```

### 3.2 扫描模块 (Scanner)

#### 3.2.1 功能描述

自动扫描指定目录中的视频文件，识别合集结构，提取元数据。

#### 3.2.2 支持的视频格式

```python
VIDEO_EXTENSIONS = {
    '.mp4', '.mkv', '.avi', '.mov', '.wmv', '.flv', '.webm',
    '.m4v', '.mpg', '.mpeg', '.ts', '.m2ts', '.vob'
}
```

#### 3.2.3 合集识别规则

合集是用户视角的"一组相关视频"，识别优先级如下：

| 优先级 | 识别方式 | 示例 |
|--------|---------|------|
| 1 | 父目录名称 | `/videos/萌鸡小队第一季/` 下的所有视频 |
| 2 | 文件名前缀 | `萌鸡小队S01E01.mp4`, `萌鸡小队S01E02.mp4` |
| 3 |  Season/Episode标记 | `S01E01`, `S1E1`, `第01集` 等模式 |
| 4 |  连续数字编号 | `01.mp4`, `02.mp4` (在同一目录下) |

#### 3.2.4 扫描结果数据结构

```python
class Collection:
    id: str                    # UUID
    name: str                  # 合集名称，如"萌鸡小队第一季"
    source_path: str           # 源目录路径
    video_files: List[VideoFile]
    created_at: datetime
    status: ScanStatus         # scanned / processing / completed / error

class VideoFile:
    id: str
    collection_id: str
    filename: str              # 原始文件名
    filepath: str              # 完整路径
    file_size: int             # 字节
    duration: float            # 秒
    video_codec: str
    resolution: str            # 如 "1920x1080"
    audio_tracks: List[AudioTrack]
    episode_number: int        # 提取的集数 (用于排序)
    episode_title: str         # 提取的集标题

class AudioTrack:
    index: int                 # 音轨索引 (0-based, 对应ffmpeg stream index)
    codec: str                 # aac, ac3, dts等
    language: str              # 语言代码 zh, en, ja
    language_full: str         # 中文, English
    channels: int              # 声道数 1/2/6
    sample_rate: int           # 采样率 44100/48000
    bitrate: int               # 比特率 bps
    title: str                 # 音轨标题 (mkv常见)
    default: bool              # 是否默认音轨
```

#### 3.2.5 ffprobe音轨解析命令

```bash
ffprobe -v quiet -print_format json -show_streams \
  -select_streams a \
  "video_file.mkv"
```

需要解析的关键字段：
- `streams[].index`: 流索引
- `streams[].codec_name`: 编码格式
- `streams[].tags.language`: 语言
- `streams[].channels`: 声道数
- `streams[].sample_rate`: 采样率
- `streams[].bit_rate`: 比特率
- `streams[].tags.title`: 音轨标题
- `streams[].disposition.default`: 是否默认

### 3.3 提取模块 (Extractor)

#### 3.3.1 功能描述

从视频文件中提取选定的音轨，转换为指定格式和质量的音频文件。

#### 3.3.2 支持的输出格式

| 格式 | 扩展名 | 故事机兼容性 | 说明 |
|------|--------|------------|------|
| MP3 | .mp3 | ⭐⭐⭐ 极好 | 最通用，推荐默认 |
| AAC | .m4a | ⭐⭐⭐ 极好 | 苹果设备友好 |
| OGG | .ogg | ⭐⭐ 一般 | 开源格式 |
| FLAC | .flac | ⭐⭐ 一般 | 无损，文件大 |
| WAV | .wav | ⭐⭐⭐ 极好 | 无损，最大 |
| OPUS | .opus | ⭐ 较少 | 高压缩率 |

#### 3.3.3 音频质量设置

**MP3/AAC质量等级：**

| 等级 | 码率 | 用途 |
|------|------|------|
| 经济 | 64 kbps | 节省空间，语音内容可接受 |
| 标准 | 128 kbps | 平衡质量与大小 |
| 优质 | 192 kbps | 音乐内容丰富推荐 |
| 无损 | 320 kbps | 最佳质量 |

**采样率选项：**
- 22050 Hz (经济)
- 44100 Hz (CD标准，推荐)
- 48000 Hz (视频标准)

#### 3.3.4 FFmpeg提取命令模板

```bash
# 基础提取 (指定音轨)
ffmpeg -i "input.mp4" -map 0:a:TRACK_INDEX -c:a libmp3lame \
  -b:a 128k -ar 44100 -ac 2 "output.mp3"

# 多音轨分别提取
ffmpeg -i "input.mkv" \
  -map 0:a:0 -c:a libmp3lame -b:a 128k "output_track0.mp3" \
  -map 0:a:1 -c:a libmp3lame -b:a 128k "output_track1.mp3"

# 保留原始音频流 (不重新编码，更快)
ffmpeg -i "input.mp4" -map 0:a:TRACK_INDEX -c:a copy "output.m4a"
```

#### 3.3.5 批量提取流程

```python
async def extract_collection(collection_id: str, settings: ExtractSettings):
    """
    批量提取合集的所有视频
    
    流程:
    1. 创建输出目录: /output/{collection_name}/
    2. 按episode_number排序遍历视频
    3. 对每个视频:
       a. 调用ffmpeg提取指定音轨
       b. 生成目标文件名 (含序号前缀)
       c. 写入输出目录
    4. 生成合集提示音 (调用TTS模块)
    5. 验证排序正确性
    6. 返回完成报告
    """
```

### 3.4 TTS模块 (TextToSpeech)

#### 3.4.1 功能描述

将合集名称转换为语音提示文件，插入到输出目录的最前面。当儿童切换故事机文件夹时，首先听到合集名称的语音播报。

#### 3.4.2 TTS服务选择

| 方案 | 优点 | 缺点 | 推荐度 |
|------|------|------|--------|
| Edge-TTS | 免费、中文质量高、无需API Key | 需要网络 | ⭐⭐⭐ 首选 |
| Coqui TTS | 本地运行、离线 | 中文模型质量一般 | ⭐⭐ |
| 百度/阿里TTS | 质量极高 | 需要API Key和付费 | ⭐⭐ |

**默认推荐 Edge-TTS**，配置简单且效果优秀。

#### 3.4.3 Edge-TTS使用示例

```bash
# 中文女声
edge-tts --text "萌鸡小队第一季" --voice zh-CN-XiaoxiaoNeural \
  --write-media "000_萌鸡小队第一季.mp3"

# 中文男声
edge-tts --text "萌鸡小队第一季" --voice zh-CN-YunxiNeural \
  --write-media "000_萌鸡小队第一季.mp3"

# 调整语速 (默认+0%)
edge-tts --text "萌鸡小队第一季" --voice zh-CN-XiaoxiaoNeural \
  --rate="+10%" --write-media "output.mp3"
```

**推荐语音角色：**
- 儿童内容: `zh-CN-XiaoxiaoNeural` (女声，温暖亲切)
- 备选: `zh-CN-YunxiNeural` (男声，清晰)

#### 3.4.4 TTS输出处理

生成的语音文件需要：
1. **音量标准化**: 使用ffmpeg调整至与其他音频相近的音量
2. **格式统一**: 确保与输出格式一致 (如MP3)
3. **质量匹配**: 使用相同的码率和采样率
4. **静音修剪**: 去除首尾多余静音

```bash
# 音量标准化 + 格式统一
ffmpeg -i "tts_output.mp3" -af "loudnorm=I=-16:TP=-1.5:LRA=11" \
  -c:a libmp3lame -b:a 128k -ar 44100 -ac 2 "final_intro.mp3"
```

### 3.5 排序适配模块 (Sorter) - 核心重点

#### 3.5.1 问题描述

**这是产品的关键差异化功能。**

儿童故事机通常基于简单文件系统遍历播放，而NTFS/FAT32在按文件名排序时有特殊规则：

**NTFS排序规则：**
1. **按字符ASCII值排序**，不是按数字大小
2. 这导致 `10.mp3` 排在 `2.mp3` 前面（因为 '1' < '2'）
3. 故事机播放顺序：1, 10, 11, 12, ... 2, 20, 21... (完全乱序)

#### 3.5.2 解决方案：前导零填充

通过前导零确保所有文件名数字部分位数相同：

```
❌ 不使用前导零 (乱序)
├── 1_植树节.mp3
├── 10_找妈妈.mp3    ← 实际第2集，但被排到了第10位
├── 11_去海边.mp3
├── 2_过生日.mp3     ← 实际第3集，被排到了第12位

✅ 使用前导零 (正确顺序)
├── 000_萌鸡小队第一季.mp3   ← 合集提示音固定为000
├── 001_植树节.mp3
├── 002_过生日.mp3
├── ...
├── 010_找妈妈.mp3
├── 011_去海边.mp3
├── ...
├── 052_大结局.mp3
```

#### 3.5.3 前导零位数计算

```python
def calculate_padding(total_episodes: int) -> int:
    """
    根据总集数计算前导零位数
    
    1-9集    -> 1位 (0-9)
    10-99集  -> 2位 (00-99)  
    100-999集 -> 3位 (000-999)
    1000+集   -> 4位 (0000-9999)
    
    注意: 需要额外+1位给提示音文件 (000)
    """
    import math
    if total_episodes < 10:
        return 2  # 00-09，给提示音预留000
    elif total_episodes < 100:
        return 2  # 00-99
    elif total_episodes < 1000:
        return 3  # 000-999
    else:
        return 4  # 0000-9999

# 示例
def generate_filename(episode_num: int, title: str, padding: int) -> str:
    prefix = str(episode_num).zfill(padding)
    return f"{prefix}_{title}.mp3"
```

#### 3.5.4 合集提示音的序号规则

合集提示音固定使用全零前缀，确保它在任何排序规则下都是第一个：

```python
INTRO_FILENAME = "0".zfill(padding) + f"_{collection_name}.mp3"

# 示例:
# 52集 -> padding=2 -> 但提示音用 "000_萌鸡小队第一季.mp3"
# 实际实现: 提示音总是用 max(padding, 3) 位，确保优先级
```

**建议规则：提示音始终使用3位零 (`000`)，无论实际集数多少。**

#### 3.5.5 排序验证

提取完成后，验证输出目录的文件顺序：

```python
def verify_sorting(output_dir: str) -> List[str]:
    """
    模拟NTFS/FAT32排序，验证文件顺序是否正确
    """
    files = [f for f in os.listdir(output_dir) if f.endswith('.mp3')]
    # Python默认字符串排序 ≈ NTFS排序
    sorted_files = sorted(files)
    
    expected_order = [f"{str(i).zfill(padding)}_..." for i in range(len(files))]
    
    if sorted_files != expected_order:
        raise SortingError(f"排序验证失败: {sorted_files}")
    
    return sorted_files
```

### 3.6 文件命名规范

#### 3.6.1 命名规则

```
{序号}_{标题}.{扩展名}

序号: 前导零填充的数字
标题: 从文件名解析的集标题，或用户自定义
扩展名: 用户选择的输出格式
```

#### 3.6.2 标题清理规则

原始视频文件名通常包含多余信息，需要清理：

```python
def clean_title(filename: str) -> str:
    """
    清理文件名，提取有意义的标题
    
    输入: "萌鸡小队.S01E02.找妈妈.1080p.WEB-DL.x264.mp4"
    输出: "找妈妈"
    
    规则:
    1. 移除文件扩展名
    2. 移除Season/Episode标记 (S01E02, 第02集等)
    3. 移除技术标签 (1080p, WEB-DL, x264, H264, BluRay等)
    4. 移除合集名称前缀 (如果标题以合集名开头)
    5. 清理特殊字符
    6. 限制长度 (建议最多30个中文字符)
    """
```

**常见需要移除的技术标签：**
```python
TECH_TAGS = [
    '1080p', '720p', '480p', '2160p', '4K', '8K',
    'WEB-DL', 'WEBRip', 'BluRay', 'BDRip', 'HDRip',
    'x264', 'x265', 'H264', 'H265', 'HEVC', 'AVC',
    'AAC', 'DTS', 'DD5.1', 'AC3',
    'CHS', 'CHT', 'GB', 'BIG5', '简体', '繁体',
]
```

#### 3.6.3 特殊字符处理

```python
# 需要替换/移除的字符，确保跨文件系统兼容
INVALID_CHARS = {
    '\\': '', '/': '', ':': '：', '*': '', '?': '？',
    '"': ''', '<': '', '>': '', '|': '',
}

# 限制长度
MAX_TITLE_LENGTH = 50  # 字符
```

---

## 4. 数据模型设计

### 4.1 数据库Schema (SQLite/PostgreSQL)

```sql
-- 合集表
CREATE TABLE collections (
    id TEXT PRIMARY KEY,           -- UUID v4
    name TEXT NOT NULL,            -- 合集名称
    source_path TEXT NOT NULL,     -- 源目录绝对路径
    output_path TEXT,              -- 输出目录绝对路径
    episode_count INTEGER,         -- 总集数
    status TEXT DEFAULT 'pending', -- pending/scanned/processing/completed/error
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    settings JSON                  -- 提取设置快照
);

-- 视频文件表
CREATE TABLE video_files (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    filename TEXT NOT NULL,        -- 原始文件名
    filepath TEXT NOT NULL,        -- 完整路径
    file_size BIGINT,
    duration REAL,
    resolution TEXT,
    video_codec TEXT,
    episode_number INTEGER,        -- 解析的集数
    episode_title TEXT,            -- 解析的标题
    status TEXT DEFAULT 'pending', -- pending/processing/completed/error
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (collection_id) REFERENCES collections(id)
);

-- 音轨表
CREATE TABLE audio_tracks (
    id TEXT PRIMARY KEY,
    video_file_id TEXT NOT NULL,
    track_index INTEGER NOT NULL,  -- ffmpeg中的stream index
    codec TEXT,
    language TEXT,                 -- 语言代码
    language_full TEXT,            -- 语言全称
    channels INTEGER,
    sample_rate INTEGER,
    bitrate INTEGER,
    title TEXT,                    -- 音轨标题
    is_default BOOLEAN DEFAULT 0,
    FOREIGN KEY (video_file_id) REFERENCES video_files(id)
);

-- 提取任务表
CREATE TABLE extract_jobs (
    id TEXT PRIMARY KEY,
    collection_id TEXT NOT NULL,
    status TEXT DEFAULT 'queued',  -- queued/processing/completed/failed/cancelled
    progress INTEGER DEFAULT 0,    -- 0-100
    current_file TEXT,             -- 当前处理的文件
    selected_track_index INTEGER,  -- 选择的音轨
    output_format TEXT,            -- 输出格式
    quality_setting TEXT,          -- 质量设置
    error_message TEXT,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    FOREIGN KEY (collection_id) REFERENCES collections(id)
);

-- 系统设置表
CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
```

### 4.2 设置项设计

```python
@dataclass
class AppSettings:
    # 扫描设置
    scan_directories: List[str] = field(default_factory=list)
    auto_scan_interval: int = 0  # 分钟，0=不自动扫描
    video_extensions: List[str] = field(default_factory=lambda: ['.mp4', '.mkv'])
    
    # 提取默认设置
    default_output_format: str = 'mp3'      # mp3/aac/flac/wav
    default_quality: str = 'standard'       # economy/standard/premium/lossless
    default_sample_rate: int = 44100        # 22050/44100/48000
    default_language: str = 'zh'            # 默认选择语言
    
    # TTS设置
    tts_enabled: bool = True
    tts_voice: str = 'zh-CN-XiaoxiaoNeural'
    tts_rate: str = '+0%'
    tts_volume_normalize: bool = True
    
    # 输出设置
    output_directory: str = '/app/output'
    filename_template: str = '{index}_{title}'
    padding_digits: str = 'auto'            # auto/2/3/4
    
    # 高级设置
    preserve_original_audio: bool = False   # 是否直接复制不重新编码
    max_concurrent_jobs: int = 2
    ffmpeg_threads: int = 4
```

---

## 5. 用户界面设计

### 5.1 界面结构

```
┌─────────────────────────────────────────────────────────────┐
│  Vid2Audio                                    [设置] [关于]  │
├────────────┬──────────────────────────────────────────────┤
│            │                                              │
│  📁 扫描    │     合集: 萌鸡小队第一季 (52集)              │
│  ─────────  │  ┌─────────────────────────────────────┐    │
│            │  │ [提取音频] [试听合集名] [打开目录]    │    │
│  📂 萌鸡小队 │  └─────────────────────────────────────┘    │
│    第一季   │                                              │
│  ✓ 已完成   │  提取设置:                                   │
│            │  • 音轨: 中文 (AAC, 立体声)                  │
│  📂 小猪佩奇 │  • 格式: MP3                                │
│    第三季   │  • 质量: 标准 (128kbps)                      │
│  ○ 待处理   │  • 采样率: 44100Hz                          │
│            │                                              │
│  📂 超级飞侠 │  文件列表 (52个文件):                        │
│    第一季   │  ┌─────────────────────────────────────┐    │
│  ○ 待处理   │  │ ☑ 001_植树节.mp3      ✓ 完成        │    │
│            │  │ ☑ 002_找妈妈.mp3      ✓ 完成        │    │
│            │  │ ☑ 003_去海边.mp3      ✓ 完成        │    │
│            │  │ ...                                 │    │
│            │  │ ☑ 052_大结局.mp3      ✓ 完成        │    │
│            │  └─────────────────────────────────────┘    │
│            │                                              │
│            │  输出预览:                                   │
│            │  /output/萌鸡小队第一季/                     │
│            │  ├── 000_萌鸡小队第一季.mp3                  │
│            │  ├── 001_植树节.mp3                          │
│            │  ├── ...                                     │
│            │  └── 052_大结局.mp3                          │
│            │                                              │
└────────────┴──────────────────────────────────────────────┘
```

### 5.2 关键界面说明

#### 5.2.1 扫描页面

- **扫描按钮**: 手动触发目录扫描
- **合集列表**: 左侧导航栏显示所有识别到的合集
- **合集状态**: 待处理/扫描完成/处理中/已完成/错误
- **合集信息**: 名称、集数、总时长、文件大小

#### 5.2.2 合集详情页

- **基本信息**: 合集名称、路径、文件数量
- **音轨选择**: 下拉框选择要提取的音轨 (显示语言、编码、声道信息)
- **试听按钮**: 播放选中的音轨样本 (提取前10秒)
- **提取设置**: 
  - 输出格式 (单选)
  - 音频质量 (单选或滑块)
  - 采样率 (下拉)
- **文件列表**: 显示所有视频文件，可勾选/全选
- **输出预览**: 显示将要生成的文件名列表
- **提取按钮**: 开始批量提取任务

#### 5.2.3 任务队列页面

- **进行中的任务**: 显示进度条、当前文件、预计剩余时间
- **历史任务**: 已完成/失败的任务记录
- **任务操作**: 暂停/恢复/取消/重试

#### 5.2.4 设置页面

- **目录设置**: 输入目录、输出目录
- **默认提取设置**: 默认格式、质量、采样率
- **TTS设置**: 启用/禁用、语音选择、语速
- **扫描设置**: 自动扫描间隔、文件扩展名过滤
- **高级设置**: FFmpeg线程数、并发任务数

### 5.3 交互流程

```
用户流程1 - 首次使用:
1. 打开Web界面
2. 进入设置页面，配置输入/输出目录
3. 返回首页，点击"扫描"
4. 等待扫描完成，查看识别的合集
5. 点击某个合集，选择音轨和提取设置
6. 点击"提取音频"
7. 等待任务完成
8. 将输出目录复制到故事机

用户流程2 - 增量更新:
1. 用户往输入目录添加新视频
2. (自动扫描模式下)系统自动识别新合集
3. 用户选择新合集提取
4. 或手动点击"扫描"发现新内容

用户流程3 - 试听音轨:
1. 进入合集详情
2. 点击"试听"按钮
3. 前端播放10秒音轨样本
4. 确认音质后选择提取
```

---

## 6. API接口设计

### 6.1 REST API 规范

Base URL: `/api/v1`

#### 6.1.1 合集管理

```
GET    /collections              # 获取合集列表
GET    /collections/{id}         # 获取合集详情
POST   /collections/{id}/scan    # 重新扫描合集
DELETE /collections/{id}         # 删除合集记录 (不删除文件)
```

#### 6.1.2 扫描

```
POST   /scan/start               # 开始扫描
       Body: { "directories": ["/path/to/videos"] }
       
GET    /scan/status/{scan_id}    # 获取扫描进度
```

#### 6.1.3 提取任务

```
POST   /extract                  # 创建提取任务
       Body: {
         "collection_id": "uuid",
         "track_index": 0,
         "output_format": "mp3",
         "quality": "standard",
         "sample_rate": 44100,
         "generate_intro": true,
         "intro_voice": "zh-CN-XiaoxiaoNeural"
       }
       
GET    /extract/jobs             # 获取任务列表
GET    /extract/jobs/{id}        # 获取任务详情/进度
POST   /extract/jobs/{id}/cancel # 取消任务
POST   /extract/jobs/{id}/retry  # 重试失败任务
```

#### 6.1.4 音轨试听

```
GET    /preview/{video_id}?track={track_index}&duration=10
       # 返回音频流 (提取前10秒用于试听)
```

#### 6.1.5 设置

```
GET    /settings                 # 获取所有设置
PUT    /settings                 # 更新设置
       Body: { "key": "value", ... }
```

#### 6.1.6 系统

```
GET    /system/status            # 系统状态 (版本、FFmpeg版本、磁盘空间)
GET    /system/logs              # 获取日志
```

### 6.2 WebSocket 事件

用于实时推送任务进度：

```javascript
// 连接
const ws = new WebSocket('ws://nas-ip:port/ws');

// 事件类型
{
  "type": "job_progress",
  "job_id": "uuid",
  "collection_id": "uuid",
  "progress": 45,          // 百分比
  "current_file": "xxx.mp4",
  "status": "processing",  // processing/completed/failed
  "message": "正在处理第23/52集..."
}

{
  "type": "scan_progress",
  "scan_id": "uuid",
  "current_dir": "/videos/萌鸡小队",
  "files_found": 150,
  "collections_found": 3
}
```

---

## 7. 工作流程设计

### 7.1 完整提取流程

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   开始扫描   │────▶│  解析视频   │────▶│  识别合集   │
└─────────────┘     └─────────────┘     └──────┬──────┘
                                               │
                                               ▼
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  验证&完成   │◄────│  生成TTS    │◄────│  提取音频   │
└─────────────┘     └─────────────┘     └──────▲──────┘
                                               │
                                        用户选择音轨/
                                        格式/质量
```

### 7.2 扫描流程详细步骤

```
1. 用户指定扫描目录 (或配置中的默认目录)
2. 递归遍历目录，匹配视频扩展名
3. 对每个视频文件:
   a. 调用ffprobe获取元数据 (时长、分辨率、音轨)
   b. 解析文件名，尝试提取集数和标题
   c. 根据父目录或文件名前缀分组到合集
4. 保存合集和视频信息到数据库
5. 返回扫描结果到前端
```

### 7.3 提取流程详细步骤

```
1. 用户选择合集、音轨、输出格式、质量
2. 创建提取任务 (状态: queued)
3. 任务调度器将任务分配给Worker
4. Worker按以下步骤处理:
   a. 创建输出目录: /output/{collection_name}/
   b. 按episode_number排序视频列表
   c. 对每个视频 (序号从1开始):
      i.   更新进度: "正在处理第N/M集: {title}"
      ii.  调用ffmpeg提取指定音轨
      iii. 生成目标文件名: {序号}_{标题}.{格式}
      iv.  保存到输出目录
   d. 生成合集提示音 (TTS)
      i.   调用Edge-TTS生成语音
      ii.  音量标准化
      iii. 保存为 000_{合集名}.{格式}
   e. 验证排序正确性
   f. 更新任务状态: completed
5. 通知前端任务完成
```

### 7.4 错误处理流程

| 错误场景 | 处理方式 |
|---------|---------|
| 视频文件损坏 | 跳过该文件，记录错误，继续处理其他 |
| 音轨提取失败 | 重试1次，仍失败则跳过 |
| TTS生成失败 | 生成一个静默音频文件作为占位，或跳过 |
| 磁盘空间不足 | 暂停任务，通知用户清理空间 |
| FFmpeg未找到 | 启动时检查，给出明确错误提示 |

---

## 8. 技术实现指南

### 8.1 Docker镜像设计

```dockerfile
FROM python:3.11-slim

# 安装系统依赖
RUN apt-get update && apt-get install -y \
    ffmpeg \
    ffprobe \
    libffi-dev \
    && rm -rf /var/lib/apt/lists/*

# 安装Edge-TTS
RUN pip install edge-tts

# 安装Python依赖
COPY requirements.txt .
RUN pip install -r requirements.txt

# 复制应用代码
COPY . /app
WORKDIR /app

# 创建目录
RUN mkdir -p /app/input /app/output /app/config /app/data

# 暴露端口
EXPOSE 8000

# 启动命令
CMD ["python", "main.py"]
```

### 8.2 docker-compose.yml 示例

```yaml
version: '3.8'

services:
  vid2audio:
    image: vid2audio:latest
    container_name: vid2audio
    restart: unless-stopped
    ports:
      - "8000:8000"
    volumes:
      - /nas/media/videos:/app/input:ro
      - /nas/media/audio-output:/app/output
      - ./config:/app/config
      - ./data:/app/data
    environment:
      - TZ=Asia/Shanghai
      - PYTHONUNBUFFERED=1
    # 如果需要GPU加速 (可选)
    # deploy:
    #   resources:
    #     reservations:
    #       devices:
    #         - driver: nvidia
    #           count: 1
    #           capabilities: [gpu]
```

### 8.3 目录结构设计

```
/project-root
├── docker/                      # Docker相关文件
│   ├── Dockerfile
│   ├── docker-compose.yml
│   └── entrypoint.sh
├── backend/                     # 后端代码
│   ├── app/
│   │   ├── __init__.py
│   │   ├── main.py              # FastAPI入口
│   │   ├── api/                 # API路由
│   │   │   ├── collections.py
│   │   │   ├── scan.py
│   │   │   ├── extract.py
│   │   │   └── settings.py
│   │   ├── core/                # 核心模块
│   │   │   ├── scanner.py
│   │   │   ├── extractor.py
│   │   │   ├── tts_engine.py
│   │   │   └── sorter.py
│   │   ├── models/              # 数据模型
│   │   │   ├── database.py
│   │   │   └── schemas.py
│   │   └── services/            # 业务服务
│   │       ├── collection_service.py
│   │       ├── extract_service.py
│   │       └── file_service.py
│   ├── workers/                 # 后台任务
│   │   └── extract_worker.py
│   └── requirements.txt
├── frontend/                    # 前端代码
│   ├── src/
│   │   ├── components/          # UI组件
│   │   ├── pages/               # 页面
│   │   ├── api/                 # API客户端
│   │   └── stores/              # 状态管理
│   ├── package.json
│   └── vite.config.ts
├── docs/                        # 文档
│   └── PRD-vid2audio.md
└── README.md
```

### 8.4 关键依赖 (requirements.txt)

```
# Web框架
fastapi==0.110.0
uvicorn[standard]==0.27.0
python-multipart==0.0.9

# 数据库
sqlalchemy==2.0.27
alembic==1.13.1
aiosqlite==0.19.0  # 异步SQLite

# 任务队列
celery==5.3.6
redis==5.0.1

# 工具
pydantic==2.6.1
pydantic-settings==2.1.0
python-magic==0.4.27  # 文件类型检测
aiofiles==23.2.1      # 异步文件操作

# 日志
loguru==0.7.2

# 测试
pytest==8.0.0
pytest-asyncio==0.23.5
httpx==0.27.0
```

### 8.5 配置文件设计 (config.yml)

```yaml
app:
  name: "Vid2Audio"
  version: "1.0.0"
  debug: false
  
server:
  host: "0.0.0.0"
  port: 8000
  
database:
  url: "sqlite:///app/data/vid2audio.db"
  # 或 PostgreSQL: "postgresql://user:pass@localhost/vid2audio"
  
directories:
  input: "/app/input"
  output: "/app/output"
  temp: "/tmp/vid2audio"
  
scan:
  auto_scan_interval: 0  # 分钟，0=关闭
  video_extensions:
    - ".mp4"
    - ".mkv"
    - ".avi"
    - ".mov"
  
extraction:
  default_format: "mp3"
  default_quality: "standard"
  default_sample_rate: 44100
  max_concurrent_jobs: 2
  ffmpeg_threads: 4
  
tts:
  enabled: true
  engine: "edge-tts"  # edge-tts / coqui / azure
  voice: "zh-CN-XiaoxiaoNeural"
  rate: "+0%"
  volume_normalize: true
  
logging:
  level: "INFO"
  file: "/app/data/vid2audio.log"
  max_size: "10MB"
  backup_count: 5
```

---

## 9. 特殊场景处理

### 9.1 多季合集处理

当目录结构包含多季时：

```
/videos/萌鸡小队/
├── 第一季/
│   ├── S01E01.mp4
│   └── ...
├── 第二季/
│   ├── S02E01.mp4
│   └── ...
```

**处理策略：** 每季识别为独立的合集，名称自动包含季信息：
- 合集1: "萌鸡小队第一季"
- 合集2: "萌鸡小队第二季"

### 9.2 单文件多故事处理

某些视频可能是"合集"形式 (一部电影包含多个小故事)：

**处理方式：** 提供手动拆分功能，或按章节提取 (如果视频有章节标记)。

### 9.3 无明确集数的情况

对于没有明确集数标记的文件：

```
/ videos/宝宝儿歌/
├── 两只老虎.mp4
├── 小星星.mp4
├── 拔萝卜.mp4
```

**处理策略：** 按文件名字母顺序分配序号，或让用户手动调整顺序。

### 9.4 音轨语言识别失败

某些视频音轨没有language标签：

**处理策略：**
1. 显示 "未知语言"
2. 允许用户试听并手动标记
3. 记住用户的选择，下次自动应用

### 9.5 文件名编码问题

某些视频文件名可能包含非UTF-8编码的字符 (常见于从Windows复制的文件)：

**处理策略：** 使用 `ftfy` 库修复编码，或尝试多种编码解码。

---

## 10. 性能与优化

### 10.1 扫描性能

- **增量扫描**: 记录文件mtime，只扫描修改过的目录
- **并发扫描**: 使用线程池并发调用ffprobe
- **缓存**: 缓存ffprobe结果，避免重复解析

### 10.2 提取性能

- **并发控制**: 限制同时运行的ffmpeg进程数 (默认2个)
- **硬件加速**: 检测是否支持Intel QSV/NVIDIA NVENC加速
- **直接复制模式**: 如果目标格式与源音轨格式相同，直接复制不重新编码

### 10.3 存储优化

- **临时文件清理**: 任务完成后清理临时文件
- **日志轮转**: 限制日志文件大小
- **数据库压缩**: SQLite定期VACUUM

---

## 11. 测试策略

### 11.1 单元测试

- 文件名解析测试 (各种命名格式)
- 排序验证测试
- 标题清理测试
- 前导零计算测试

### 11.2 集成测试

- 完整扫描流程测试
- FFmpeg调用测试
- TTS生成测试
- API端点测试

### 11.3 测试数据

准备包含以下特征的测试视频集：
- 不同格式: MP4, MKV, AVI
- 多音轨: 中文+英文
- 不同命名: S01E01, 第01集, 01, 无编号
- 特殊字符: 包含空格、括号、中文符号

---

## 12. 部署与维护

### 12.1 NAS平台适配

| NAS系统 | Docker支持 | 注意事项 |
|---------|-----------|---------|
| Synology DSM | ✅ 内置 | 使用Container Manager |
| QNAP QTS | ✅ 内置 | 使用Container Station |
| Unraid | ✅ 内置 | 使用Apps市场 |
| TrueNAS SCALE | ✅ 内置 | 使用Apps |
| 自组NAS | ✅ Docker Engine | 手动安装Docker |

### 12.2 更新策略

- **容器更新**: 拉取新镜像，保持配置和数据卷
- **数据库迁移**: 使用Alembic管理schema变更
- **配置兼容**: 新版本兼容旧版配置文件

### 12.3 备份建议

用户需要备份的内容：
- `/app/config/` - 配置文件
- `/app/data/` - 数据库和日志
- `/app/output/` - 生成的音频文件 (可选)

---

## 13. 未来扩展方向

### 13.1 可能的增强功能

1. **字幕提取**: 从视频中提取字幕，生成LRC歌词文件
2. **封面生成**: 提取视频帧作为音频封面
3. **章节拆分**: 根据视频章节标记拆分音频
4. **批量重命名**: 支持自定义文件名模板
5. **云端同步**: 支持同步到网盘
6. **移动端适配**: 优化手机浏览器体验
7. **多用户支持**: 家庭多用户隔离
8. **播放列表**: 生成M3U播放列表

### 13.2 集成可能性

- **Plex/Jellyfin**: 读取媒体库元数据
- **Sonarr/Radarr**: 监听下载完成事件自动处理
- **Home Assistant**: 添加为插件

---

## 14. 附录

### 14.1 术语表

| 术语 | 说明 |
|------|------|
| 合集 (Collection) | 一组相关的视频文件，如"某动画第一季" |
| 音轨 (Audio Track) | 视频中的音频流，一个视频可包含多条 |
| TTS | Text-to-Speech，文本转语音 |
| 前导零 (Zero Padding) | 在数字前补零以保持排序一致 |
| NTFS排序 | Windows文件系统的文件名排序规则 |
| FFmpeg | 开源多媒体处理工具 |
| ffprobe | FFmpeg配套的工具，用于查看媒体元数据 |

### 14.2 参考资源

- FFmpeg文档: https://ffmpeg.org/documentation.html
- Edge-TTS: https://github.com/rany2/edge-tts
- FastAPI: https://fastapi.tiangolo.com/
- NTFS排序规则: https://docs.microsoft.com/en-us/windows/win32/fileio/naming-a-file

### 14.3 示例数据

**测试用的合集结构：**

```
/input/
├── 萌鸡小队第一季/
│   ├── 萌鸡小队.S01E01.植树节.1080p.mp4
│   ├── 萌鸡小队.S01E02.找妈妈.1080p.mp4
│   ├── 萌鸡小队.S01E03.去海边.1080p.mp4
│   └── ... (共52集)
│
├── 小猪佩奇第三季 (国语)/
│   ├── 第01集.泥坑.mp4
│   ├── 第02集.恐龙先生丢失了.mp4
│   ├── 第03集.最好的朋友.mp4
│   └── ... (共26集)
│
└── 经典儿歌合集/
    ├── 两只老虎.mp4
    ├── 小星星.mp4
    ├── 拔萝卜.mp4
    └── 小兔子乖乖.mp4
```

**期望的输出结构：**

```
/output/
├── 萌鸡小队第一季/
│   ├── 000_萌鸡小队第一季.mp3      # TTS生成
│   ├── 001_植树节.mp3
│   ├── 002_找妈妈.mp3
│   ├── 003_去海边.mp3
│   └── ... (052_...)
│
├── 小猪佩奇第三季/
│   ├── 000_小猪佩奇第三季.mp3
│   ├── 001_泥坑.mp3
│   ├── 002_恐龙先生丢失了.mp3
│   └── ... (026_...)
│
└── 经典儿歌合集/
    ├── 000_经典儿歌合集.mp3
    ├── 001_两只老虎.mp3
    ├── 002_小星星.mp3
    ├── 003_拔萝卜.mp3
    └── 004_小兔子乖乖.mp3
```

---

## 15. 实现优先级

### Phase 1 - MVP (最小可用产品)

- [ ] Docker容器化基础框架
- [ ] 目录扫描和视频识别
- [ ] ffprobe音轨解析
- [ ] 基础Web界面 (合集列表、详情)
- [ ] FFmpeg音频提取 (MP3格式)
- [ ] 前导零排序适配
- [ ] Edge-TTS合集提示音生成
- [ ] 基础设置页面

### Phase 2 - 增强体验

- [ ] 音轨试听功能
- [ ] 多种输出格式支持
- [ ] 质量等级选择
- [ ] 任务队列和进度显示
- [ ] 批量提取优化
- [ ] 增量扫描
- [ ] 多语言音轨自动识别

### Phase 3 - 高级功能

- [ ] 硬件加速支持
- [ ] 自动扫描调度
- [ ] 多用户支持
- [ ] 播放列表生成
- [ ] 字幕提取
- [ ] 移动端适配
- [ ] 与媒体服务器集成

---

> **文档结束**  
> 本文档为Vid2Audio产品的完整设计规范，后续开发应严格遵循此文档的数据结构、工作流程和接口定义。如有设计变更，需同步更新本文档。
