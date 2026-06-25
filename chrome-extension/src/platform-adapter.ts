/**
 * TokenSlim Chrome 扩展 - 平台适配器接口
 * 每个 AI 聊天平台（ChatGPT/Claude/Gemini/通义千问/文心一言）实现此接口，
 * 提供统一的输入获取、提交拦截和内容替换能力。
 */

export interface PlatformAdapter {
  /** 平台唯一标识 */
  readonly name: string;

  /** 检测当前页面是否属于本平台 */
  match(): boolean;

  /**
   * 获取当前页面的输入框元素。
   * 返回 null 表示尚未找到输入框（可能需要等待 DOM 加载）。
   */
  getInputEl(): HTMLElement | null;

  /**
   * 从输入框中提取纯文本内容。
   */
  extractText(el: HTMLElement): string;

  /**
   * 拦截提交事件。
   * 当用户触发发送时，调用 callback 处理文本，然后用处理后的文本替换输入并重新提交。
   * @param cb 异步回调，接收原始文本，返回替换后的文本
   */
  interceptSubmit(cb: (text: string) => Promise<string>): void;

  /**
   * 将输入框内容替换为新文本。
   * 需要触发框架能感知到的事件（React 用 nativeInputValueSetter，Vue 用 dispatchEvent）。
   */
  replaceInput(el: HTMLElement, newText: string): void;

  /**
   * 在输入框附近显示压缩指示器（如 "TokenSlim: 12KB -> 3KB"）。
   */
  showCompressionIndicator(el: HTMLElement, originalSize: number, compressedSize: number): void;
}

/**
 * 设置 textarea / contenteditable 的文本内容，同时触发 React 兼容的变更事件。
 * React 劫持了 input/textarea 的 value setter，需要用原生 setter 绕过。
 */
export function setNativeValue(el: HTMLElement, value: string): void {
  if (el instanceof HTMLTextAreaElement || el instanceof HTMLInputElement) {
    // React 兼容：使用原生 setter
    const nativeInputValueSetter = Object.getOwnPropertyDescriptor(
      window.HTMLTextAreaElement.prototype, 'value'
    )?.set || Object.getOwnPropertyDescriptor(
      window.HTMLInputElement.prototype, 'value'
    )?.set;

    if (nativeInputValueSetter) {
      nativeInputValueSetter.call(el, value);
    } else {
      el.value = value;
    }

    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
  } else if (el.isContentEditable) {
    // contenteditable 元素
    el.textContent = value;
    el.dispatchEvent(new Event('input', { bubbles: true }));
  }
}

/**
 * 创建压缩指示器 DOM 元素。
 */
export function createIndicator(originalSize: number, compressedSize: number): HTMLElement {
  const indicator = document.createElement('div');
  indicator.className = 'tokenslim-compression-indicator';
  const ratio = ((compressedSize / originalSize) * 100).toFixed(1);
  indicator.textContent = `TokenSlim: ${formatBytes(originalSize)} → ${formatBytes(compressedSize)} (${ratio}%)`;
  return indicator;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes}B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)}KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)}MB`;
}
