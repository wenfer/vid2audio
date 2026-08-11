import { shallowRef } from 'vue'

export interface PromptOptions {
  title: string
  /** 输入框上方的说明文字。 */
  label?: string
  value?: string
  placeholder?: string
  confirmLabel?: string
}

interface PromptRequest extends PromptOptions {
  resolve: (value: string | null) => void
}

/**
 * 应用内的文本输入对话框。
 *
 * 为什么不用 `window.prompt`：WebView2 **没有实现** `window.prompt`，桌面版里
 * 它不弹窗也不报错，直接返回 null——重命名会变成静默失效。Tauri 的原生对话框
 * 也只有消息/确认/选文件，没有文本输入，所以自己做一个，两端共用同一条路径。
 *
 * 用 `shallowRef` 而不是 `ref`：请求对象里存着 promise 的 resolve 函数，
 * 不需要（也不该）被深度代理。
 */
const current = shallowRef<PromptRequest | null>(null)

export function usePrompt() {
  function ask(options: PromptOptions): Promise<string | null> {
    // 同一时刻只保留一个请求；旧的按取消处理，否则它的 promise 会一直悬着。
    current.value?.resolve(null)
    return new Promise<string | null>((resolve) => {
      current.value = { ...options, resolve }
    })
  }

  function settle(value: string | null) {
    const request = current.value
    if (!request) return
    current.value = null
    request.resolve(value)
  }

  return {
    current,
    ask,
    submit: (value: string) => settle(value),
    cancel: () => settle(null),
  }
}
