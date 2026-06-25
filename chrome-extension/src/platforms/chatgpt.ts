/**
 * TokenSlim Chrome 扩展 - ChatGPT 平台适配器
 * 适配 chatgpt.com 的输入框（contenteditable div）和提交按钮。
 */

import { PlatformAdapter, setNativeValue, createIndicator } from '../platform-adapter';

export class ChatGPTAdapter implements PlatformAdapter {
  readonly name = 'chatgpt';

  match(): boolean {
    return location.hostname === 'chatgpt.com' || location.hostname === 'chat.openai.com';
  }

  getInputEl(): HTMLElement | null {
    // ChatGPT 使用 contenteditable div 作为输入框
    // 选择器: #prompt-textarea 是新版 UI 的输入框
    return (
      document.querySelector('#prompt-textarea') ||
      document.querySelector('[data-testid="text-editor-0"]') ||
      document.querySelector('div[contenteditable="true"][class*="ProseMirror"]') ||
      document.querySelector('div[contenteditable="true"]')
    ) as HTMLElement | null;
  }

  extractText(el: HTMLElement): string {
    if (el.isContentEditable) {
      return el.innerText || el.textContent || '';
    }
    return (el as HTMLTextAreaElement).value || '';
  }

  interceptSubmit(cb: (text: string) => Promise<string>): void {
    // ChatGPT 的发送按钮
    const sendButtonSelector = '[data-testid="send-button"], button[aria-label="Send message"]';

    // 监听 Enter 键（非 Shift+Enter）
    document.addEventListener('keydown', async (e: KeyboardEvent) => {
      if (e.key !== 'Enter' || e.shiftKey || e.isComposing) return;

      const el = this.getInputEl();
      if (!el || !el.contains(document.activeElement)) return;

      const text = this.extractText(el);
      if (!text.trim()) return;

      e.preventDefault();
      e.stopPropagation();

      const processed = await cb(text);
      this.replaceInput(el, processed);

      // 等一帧让 React 状态同步后再点发送
      requestAnimationFrame(() => {
        const sendBtn = document.querySelector(sendButtonSelector) as HTMLButtonElement | null;
        sendBtn?.click();
      });
    }, true);

    // 监听发送按钮点击
    document.addEventListener('click', async (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      const sendBtn = target.closest(sendButtonSelector);
      if (!sendBtn) return;

      const el = this.getInputEl();
      if (!el) return;

      const text = this.extractText(el);
      if (!text.trim()) return;

      e.preventDefault();
      e.stopPropagation();

      const processed = await cb(text);
      this.replaceInput(el, processed);

      requestAnimationFrame(() => {
        (sendBtn as HTMLButtonElement).click();
      });
    }, true);
  }

  replaceInput(el: HTMLElement, newText: string): void {
    setNativeValue(el, newText);
  }

  showCompressionIndicator(el: HTMLElement, originalSize: number, compressedSize: number): void {
    const indicator = createIndicator(originalSize, compressedSize);
    const parent = (el.closest('[class*="composer"]') as HTMLElement | null) || el.parentElement;
    if (parent) {
      parent.style.position = 'relative';
      parent.appendChild(indicator);
      setTimeout(() => indicator.remove(), 3000);
    }
  }
}
