/**
 * TokenSlim Chrome 扩展 - 文心一言平台适配器
 * 适配 yiyan.baidu.com（文心一言/ERNIE Bot）的输入框和提交按钮。
 */

import { PlatformAdapter, setNativeValue, createIndicator } from '../platform-adapter';

export class WenxinAdapter implements PlatformAdapter {
  readonly name = 'wenxin';

  match(): boolean {
    return (
      location.hostname === 'yiyan.baidu.com' ||
      location.hostname === 'chat.baidu.com' ||
      location.hostname.includes('yiyan')
    );
  }

  getInputEl(): HTMLElement | null {
    // 文心一言的输入框
    return (
      document.querySelector('textarea[class*="chat-input"]') ||
      document.querySelector('[contenteditable="true"][class*="input"]') ||
      document.querySelector('div[contenteditable="true"]') ||
      document.querySelector('textarea')
    ) as HTMLElement | null;
  }

  extractText(el: HTMLElement): string {
    if (el.isContentEditable) {
      return el.innerText || el.textContent || '';
    }
    return (el as HTMLTextAreaElement).value || '';
  }

  interceptSubmit(cb: (text: string) => Promise<string>): void {
    // 文心一言发送按钮
    const sendButtonSelector = 'button[class*="send"], button[aria-label*="发送"], div[class*="send-btn"]';

    // Enter 键拦截
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

    // 发送按钮拦截
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
        (sendBtn as HTMLElement).click();
      });
    }, true);
  }

  replaceInput(el: HTMLElement, newText: string): void {
    setNativeValue(el, newText);
  }

  showCompressionIndicator(el: HTMLElement, originalSize: number, compressedSize: number): void {
    const indicator = createIndicator(originalSize, compressedSize);
    const parent = (el.closest('[class*="input-box"]') as HTMLElement | null) || el.parentElement;
    if (parent) {
      parent.style.position = 'relative';
      parent.appendChild(indicator);
      setTimeout(() => indicator.remove(), 3000);
    }
  }
}
