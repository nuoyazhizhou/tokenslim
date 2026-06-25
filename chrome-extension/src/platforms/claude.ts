/**
 * TokenSlim Chrome 扩展 - Claude 平台适配器
 * 适配 claude.ai 的输入框（contenteditable ProseMirror）和提交按钮。
 */

import { PlatformAdapter, setNativeValue, createIndicator } from '../platform-adapter';

export class ClaudeAdapter implements PlatformAdapter {
  readonly name = 'claude';

  match(): boolean {
    return location.hostname === 'claude.ai';
  }

  getInputEl(): HTMLElement | null {
    // Claude 使用 ProseMirror contenteditable div
    return (
      document.querySelector('div[contenteditable="true"][role="textbox"]') ||
      document.querySelector('div.ProseMirror[contenteditable="true"]') ||
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
    // Claude 的发送按钮选择器
    const sendButtonSelector = 'button[aria-label="Send Message"], button[data-testid="send-message"]';

    // 监听 Enter 键
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

      requestAnimationFrame(() => {
        const sendBtn = document.querySelector(sendButtonSelector) as HTMLButtonElement | null;
        sendBtn?.click();
      });
    }, true);

    // 监听发送按钮
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
    const parent = (el.closest('[class*="input"]') as HTMLElement | null) || el.parentElement;
    if (parent) {
      parent.style.position = 'relative';
      parent.appendChild(indicator);
      setTimeout(() => indicator.remove(), 3000);
    }
  }
}
