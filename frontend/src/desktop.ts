/**
 * 桌面版（Tauri）与浏览器版共用同一份前端，两者的差异全部收在这个模块里。
 *
 * 判定方式是看 `__TAURI_INTERNALS__` 在不在：Tauri 会在每个 webview 启动前注入它，
 * 浏览器里永远不存在。不要用 UA 判断——WebView2 的 UA 就是 Edge 的 UA。
 *
 * Tauri 的 npm 包用动态 `import()` 引入，浏览器版的构建产物里它们是独立 chunk，
 * 永远不会被请求，因此 Docker 部署的体积和行为完全不受影响。
 */

const IS_DESKTOP = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/** 当前是否跑在桌面版外壳里。模板里用它决定要不要显示「浏览…」这类按钮。 */
export function isDesktop(): boolean {
  return IS_DESKTOP
}

/**
 * 弹系统文件夹选择框，返回绝对路径；用户取消返回 `null`。
 *
 * 浏览器里直接返回 `null`（网页拿不到真实路径），调用方应该先用 `isDesktop()`
 * 把入口藏掉。
 */
export async function pickDirectory(defaultPath?: string): Promise<string | null> {
  if (!IS_DESKTOP) return null
  const { open } = await import('@tauri-apps/plugin-dialog')
  const picked = await open({ directory: true, defaultPath: defaultPath || undefined })
  // 单选时返回 string；这里不开 multiple，其他类型一律当成取消。
  return typeof picked === 'string' ? picked : null
}

/** 弹系统「另存为」框，返回目标文件的绝对路径；用户取消返回 `null`。 */
export async function pickSavePath(
  defaultName: string,
  filter?: { name: string; extensions: string[] }
): Promise<string | null> {
  if (!IS_DESKTOP) return null
  const { save } = await import('@tauri-apps/plugin-dialog')
  const picked = await save({
    defaultPath: defaultName,
    filters: filter ? [filter] : undefined,
  })
  return typeof picked === 'string' ? picked : null
}

/**
 * 确认对话框。
 *
 * 桌面版用系统原生对话框，而不是 `window.confirm`：后者在 WebView2 里带
 * “v2a.localhost 显示”这种页面来源前缀，在应用里看着像网页弹窗；
 * WKWebView 还需要宿主实现 UI delegate，不同平台表现不一致。
 */
export async function confirmAction(
  message: string,
  options: { title?: string; okLabel?: string; danger?: boolean } = {}
): Promise<boolean> {
  if (!IS_DESKTOP) return window.confirm(message)
  const { confirm } = await import('@tauri-apps/plugin-dialog')
  return confirm(message, {
    title: options.title ?? 'Vid2Audio',
    kind: options.danger ? 'warning' : 'info',
    okLabel: options.okLabel ?? '确定',
    cancelLabel: '取消',
  })
}

/**
 * 在系统文件管理器里定位到某个文件/目录。
 *
 * 失败不抛错：这只是保存之后的顺手操作，路径已经写好了，弹错反而扰人。
 */
export async function revealInFileManager(path: string): Promise<void> {
  if (!IS_DESKTOP) return
  try {
    const { revealItemInDir } = await import('@tauri-apps/plugin-opener')
    await revealItemInDir(path)
  } catch {
    /* 打不开就算了 */
  }
}

/** 自动更新检查结果。 */
export type UpdateCheckResult =
  | { state: 'current' }
  | { state: 'available'; version: string }
  | { state: 'error'; message: string }

/**
 * 检查更新（Tauri updater）。浏览器版不支持，直接返回 error。
 * 更新源是 GitHub Release 上的 latest.json（见 tauri.conf.json 的 updater.endpoints）。
 */
export async function checkForUpdates(): Promise<UpdateCheckResult> {
  if (!IS_DESKTOP) return { state: 'error', message: '浏览器版不支持自动更新' }
  try {
    const { check } = await import('@tauri-apps/plugin-updater')
    const update = await check()
    return update ? { state: 'available', version: update.version } : { state: 'current' }
  } catch (e) {
    return { state: 'error', message: (e as Error).message }
  }
}

/**
 * 下载并安装最新版。安装完成后需要重启应用生效。
 * 非桌面返回 false；无可用更新返回 false。
 */
export async function installUpdate(): Promise<boolean> {
  if (!IS_DESKTOP) return false
  const { check } = await import('@tauri-apps/plugin-updater')
  const update = await check()
  if (!update) return false
  await update.downloadAndInstall()
  return true
}
