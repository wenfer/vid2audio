export function formatBytes(value?: number): string {
  if (!value) return ''
  if (value < 1024 * 1024) return `${Math.round(value / 1024)} KB`
  if (value < 1024 * 1024 * 1024) return `${(value / 1024 / 1024).toFixed(1)} MB`
  return `${(value / 1024 / 1024 / 1024).toFixed(1)} GB`
}

export function formatDuration(value?: number): string {
  if (!value) return '-'
  const m = Math.floor(value / 60)
  const s = Math.floor(value % 60)
  return `${m}:${String(s).padStart(2, '0')}`
}

export function statusBadgeClass(status: string): string {
  const map: Record<string, string> = {
    completed: 'badge-success',
    scanned: 'badge-success',
    processing: 'badge-info',
    queued: 'badge-info',
    paused: 'badge-warning',
    failed: 'badge-danger',
    error: 'badge-danger',
    cancelled: 'badge-muted',
  }
  return map[status] || 'badge-muted'
}

const STATUS_LABELS: Record<string, string> = {
  pending: '待处理',
  queued: '排队中',
  processing: '进行中',
  paused: '已暂停',
  completed: '已完成',
  scanned: '已分析',
  failed: '失败',
  error: '出错',
  cancelled: '已取消',
}

export function statusLabel(status: string): string {
  return STATUS_LABELS[status] || status
}

export function formatTimestamp(value?: string): string {
  if (!value) return '—'
  // 后端存的是 SQLite 的 UTC 字符串（"2026-08-10 12:34:56"），补上时区再本地化。
  const parsed = new Date(value.includes('T') ? value : `${value.replace(' ', 'T')}Z`)
  if (Number.isNaN(parsed.getTime())) return value
  return parsed.toLocaleString('zh-CN', { hour12: false })
}
