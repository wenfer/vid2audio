use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

fn default_language() -> String {
    "und".into()
}
fn default_language_full() -> String {
    "未知语言".into()
}
fn default_pending() -> String {
    "pending".into()
}
fn default_output_format() -> String {
    "mp3".into()
}
fn default_quality() -> String {
    "standard".into()
}
fn default_sample_rate() -> i64 {
    44_100
}
fn default_true() -> bool {
    true
}
fn default_intro_voice() -> String {
    "zh_CN-huayan-medium".into()
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AudioTrack {
    pub id: Option<String>,
    pub video_file_id: Option<String>,
    pub index: i64,
    #[serde(default)]
    pub codec: String,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_language_full")]
    pub language_full: String,
    pub channels: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bitrate: Option<i64>,
    #[serde(default)]
    pub title: String,
    #[serde(default, rename = "default")]
    pub is_default: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VideoFile {
    pub id: String,
    pub collection_id: Option<String>,
    pub filename: String,
    pub filepath: String,
    #[serde(default)]
    pub file_size: i64,
    pub duration: Option<f64>,
    #[serde(default)]
    pub video_codec: String,
    #[serde(default)]
    pub resolution: String,
    #[serde(default)]
    pub audio_tracks: Vec<AudioTrack>,
    pub episode_number: i64,
    pub episode_title: String,
    #[serde(default = "default_pending")]
    pub status: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub source_path: String,
    pub output_path: Option<String>,
    #[serde(default)]
    pub episode_count: i64,
    #[serde(default = "default_pending")]
    pub status: String,
    #[serde(default)]
    pub video_files: Vec<VideoFile>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    #[serde(default)]
    pub settings: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub scan_directories: Vec<String>,
    pub video_extensions: Vec<String>,
    pub min_file_size_mb: f64,
    pub ignored_extensions: Vec<String>,
    pub default_output_format: String,
    pub default_quality: String,
    pub default_sample_rate: i64,
    pub tts_enabled: bool,
    pub tts_provider: String,
    pub tts_voice: String,
    pub tts_rate: String,
    pub tts_failure_mode: String,
    pub intro_text_template: String,
    pub output_directory: String,
    pub padding_digits: String,
    pub filesystem_sorting: String,
    pub ffmpeg_threads: i64,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            scan_directories: vec!["/app/input".into()],
            video_extensions: [
                ".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".webm", ".m4v", ".mpg", ".mpeg",
                ".ts", ".m2ts", ".vob",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            min_file_size_mb: 1.0,
            ignored_extensions: [".part", ".tmp", ".download", ".ds_store"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            default_output_format: "mp3".into(),
            default_quality: "standard".into(),
            default_sample_rate: 44_100,
            tts_enabled: true,
            tts_provider: "piper".into(),
            tts_voice: "zh_CN-huayan-medium".into(),
            tts_rate: "+0%".into(),
            tts_failure_mode: "silent".into(),
            intro_text_template: "{collection_name}".into(),
            output_directory: "/app/output".into(),
            padding_digits: "auto".into(),
            filesystem_sorting: "ntfs".into(),
            ffmpeg_threads: 4,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ScanRequest {
    pub directories: Option<Vec<String>>,
    pub source_paths: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ScanResult {
    pub scan_id: String,
    pub collections: Vec<Collection>,
    pub files_found: i64,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtractRequest {
    pub collection_id: Option<String>,
    pub source_path: Option<String>,
    pub job_name: Option<String>,
    #[serde(default)]
    pub track_index: i64,
    #[serde(default = "default_output_format")]
    pub output_format: String,
    #[serde(default = "default_quality")]
    pub quality: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: i64,
    #[serde(default = "default_true")]
    pub generate_intro: bool,
    #[serde(default = "default_intro_voice")]
    pub intro_voice: String,
    pub tts_provider: Option<String>,
    pub tts_rate: Option<String>,
    pub tts_failure_mode: Option<String>,
    pub intro_text: Option<String>,
    pub selected_video_ids: Option<Vec<String>>,
    #[serde(default)]
    pub trim_start_seconds: f64,
    #[serde(default)]
    pub trim_end_seconds: f64,
    pub filesystem_sorting: Option<String>,
    #[serde(default)]
    pub padding_digits: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExtractJob {
    pub id: String,
    pub collection_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source_path: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub progress: i64,
    pub current_file: Option<String>,
    #[serde(default)]
    pub selected_track_index: i64,
    #[serde(default)]
    pub output_format: String,
    #[serde(default)]
    pub quality_setting: String,
    #[serde(default)]
    pub trim_start_seconds: f64,
    #[serde(default)]
    pub trim_end_seconds: f64,
    #[serde(default)]
    pub total_count: i64,
    #[serde(default)]
    pub success_count: i64,
    #[serde(default)]
    pub failure_count: i64,
    pub error_message: Option<String>,
    pub output_path: Option<String>,
    #[serde(default)]
    pub summary: Value,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ExtractJobItem {
    pub id: String,
    pub job_id: String,
    pub video_file_id: Option<String>,
    pub source_path: String,
    pub output_path: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: Option<String>,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ExtractJobDetail {
    #[serde(flatten)]
    pub job: ExtractJob,
    pub items: Vec<ExtractJobItem>,
}
