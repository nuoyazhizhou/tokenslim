/**
 * TokenSlim Chrome 扩展 - Gemini 平台适配器
 * 适配 gemini.google.com 的输入框和提交按钮。
 */

import { PlatformAdapter, setNativeValue, createIndicator } from '../platform-adapter';

export class GeminiAdapter implements PlatformAdapter {
  readonly name = 'gemini';

  match(): boolean {
    return location.hostname === 'gemini.google.com';
  }

  getInputEl(): HTMLElement | null {
    // Gemini 使用 rich-text-textarea（基于 contenteditable）
    return (
      document.querySelector('[data-test-id="input-text-area"]') ||
      document.querySelector('rich-textarea [contenteditable="true"]') ||
      document.querySelector('.ql-editor[contenteditable="true"]') ||
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
    // Gemini 的发送按钮
    const sendButtonSelector = 'button[aria-label*="Send"], button[data-test-id="send-button"]';

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
    const parent = (el.closest('.input-area') as HTMLElement | null) || el.parentElement;
    if (parent) {
      parent.style.position = 'relative';
      parent.appendChild(indicator);
      setTimeout(() => indicator.remove(), 3000);
    }
  }
}
