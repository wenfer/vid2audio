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
    failed: 'badge-danger',
    error: 'badge-danger',
    cancelled: 'badge-muted',
  }
  return map[status] || 'badge-muted'
}
