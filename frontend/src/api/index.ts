const BASE = '/api/v1'

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message)
  }
}

export async function request<T = unknown>(path: string, options: RequestInit = {}): Promise<T> {
  const response = await fetch(`${BASE}${path}`, {
    headers: { 'Content-Type': 'application/json' },
    ...options,
  })
  if (!response.ok) {
    const data = await response.json().catch(() => ({}))
    throw new ApiError(response.status, data.detail || response.statusText)
  }
  if (response.status === 204) return null as T
  const contentType = response.headers.get('content-type') || ''
  return contentType.includes('application/json') ? response.json() : (response.text() as unknown as T)
}

export const api = {
  // Settings
  getSettings: () => request<import('../types').AppSettings>('/settings'),
  updateSettings: (data: Partial<import('../types').AppSettings>) =>
    request<import('../types').AppSettings>('/settings', { method: 'PUT', body: JSON.stringify(data) }),

  // Files
  getFiles: (path?: string) => {
    const query = path ? `?path=${encodeURIComponent(path)}` : ''
    return request<import('../types').BrowserState>(`/files${query}`)
  },
  copyFiles: (sources: string[], destination: string) =>
    request('/files/copy', { method: 'POST', body: JSON.stringify({ sources, destination }) }),
  moveFiles: (sources: string[], destination: string) =>
    request('/files/move', { method: 'POST', body: JSON.stringify({ sources, destination }) }),
  renameFile: (path: string, newName: string) =>
    request('/files/rename', { method: 'POST', body: JSON.stringify({ path, new_name: newName }) }),
  deleteFiles: (paths: string[]) =>
    request('/files/delete', { method: 'POST', body: JSON.stringify({ paths }) }),
  fatSort: (path: string) =>
    request<{ success: boolean; count: number; recovered: boolean }>('/files/fat-sort', {
      method: 'POST',
      body: JSON.stringify({ path }),
    }),
  archiveUrl: (path: string) => `${BASE}/files/archive?path=${encodeURIComponent(path)}`,
  archiveTo: (path: string, destination: string) =>
    request<{ success: boolean; path: string; size: number }>('/files/archive-to', {
      method: 'POST',
      body: JSON.stringify({ path, destination }),
    }),

  // Scan
  startScan: (sourcePaths: string[]) =>
    request<{ collections: import('../types').Collection[]; files_found: number; warnings: string[] }>(
      '/scan/start',
      { method: 'POST', body: JSON.stringify({ source_paths: sourcePaths }) }
    ),

  // Collections
  getCollections: () => request<import('../types').Collection[]>('/collections'),
  getCollection: (id: string) => request<import('../types').Collection>(`/collections/${id}`),
  deleteCollection: (id: string) => request(`/collections/${id}`, { method: 'DELETE' }),
  rescanCollection: (id: string) =>
    request<{ collections: import('../types').Collection[]; files_found: number; warnings: string[] }>(
      `/collections/${id}/scan`,
      { method: 'POST' }
    ),

  // Extract
  createJob: (data: Record<string, unknown>) =>
    request<import('../types').ExtractJob>('/extract', { method: 'POST', body: JSON.stringify(data) }),
  getJobs: () => request<import('../types').ExtractJob[]>('/extract/jobs'),
  getJob: (id: string) => request<import('../types').ExtractJobDetail>(`/extract/jobs/${id}`),
  deleteJob: (id: string) => request(`/extract/jobs/${id}`, { method: 'DELETE' }),
  cancelJob: (id: string) => request(`/extract/jobs/${id}/cancel`, { method: 'POST' }),
  pauseJob: (id: string) =>
    request<import('../types').ExtractJob>(`/extract/jobs/${id}/pause`, { method: 'POST' }),
  resumeJob: (id: string) =>
    request<import('../types').ExtractJob>(`/extract/jobs/${id}/resume`, { method: 'POST' }),

  // Preview
  previewUrl: (videoId: string, track: number, start: number) =>
    `${BASE}/preview/${videoId}?track=${track}&duration=10&start=${start}&_=${Date.now()}`,
  jobItemAudioUrl: (jobId: string, itemId: string) =>
    `${BASE}/extract/jobs/${jobId}/items/${itemId}/audio?_=${Date.now()}`,
}
