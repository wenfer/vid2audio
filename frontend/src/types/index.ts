export interface AudioTrack {
  id?: string
  index: number
  codec: string
  language: string
  language_full: string
  channels?: number
  sample_rate?: number
  bitrate?: number
  title: string
  default: boolean
}

export interface VideoFile {
  id: string
  filename: string
  filepath: string
  file_size: number
  duration?: number
  video_codec: string
  resolution: string
  audio_tracks: AudioTrack[]
  episode_number: number
  episode_title: string
  status: string
}

export interface Collection {
  id: string
  name: string
  source_path: string
  output_path?: string
  episode_count: number
  status: string
  video_files: VideoFile[]
  created_at?: string
  updated_at?: string
}

export interface AppSettings {
  scan_directories: string[]
  video_extensions: string[]
  min_file_size_mb: number
  ignored_extensions: string[]
  default_output_format: string
  default_quality: string
  default_sample_rate: number
  tts_enabled: boolean
  tts_provider: string
  tts_voice: string
  tts_rate: string
  tts_failure_mode: string
  intro_text_template: string
  output_directory: string
  padding_digits: string
  filesystem_sorting: string
  ffmpeg_threads: number
}

export interface BrowserEntry {
  name: string
  path: string
  type: 'directory' | 'file'
  is_video: boolean
  selectable: boolean
  size?: number
  reason?: string
}

export interface BrowserState {
  path: string
  parent?: string
  entries: BrowserEntry[]
  sorting?: string
  warning?: string
}

export interface ExtractJob {
  id: string
  collection_id?: string
  name: string
  source_path: string
  status: string
  progress: number
  current_file?: string
  total_count: number
  success_count: number
  failure_count: number
  error_message?: string
  output_path?: string
  summary: Record<string, unknown>
  created_at?: string
}

export interface ExtractJobItem {
  id: string
  job_id: string
  source_path: string
  output_path?: string
  title: string
  status: string
  error_message?: string
}

export interface ExtractJobDetail extends ExtractJob {
  items: ExtractJobItem[]
}
