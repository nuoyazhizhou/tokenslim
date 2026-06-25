/**
 * TokenSlim Chrome 扩展 - 核心输入拦截器
 * 协调平台适配器和 Server API，实现无感压缩流程：
 * 1. 检测输入框 → 2. 监听提交 → 3. 提取文本 → 4. 判断阈值 →
 * 5. 调用 API 压缩 → 6. 替换输入内容 → 7. 显示压缩指示器
 */

import type { PlatformAdapter } from './platform-adapter';
import type { TokenSlimAPIProxy, CompressResult } from './api';
import type { TokenSlimConfig } from './config';

/** 压缩指令前缀，AI 看到后应理解需要解压 */
const COMPRESS_PREFIX = '[TokenSlim compressed payload below, please decompress using the dictionary]';
const COMPRESS_SUFFIX = '[End TokenSlim]';

/** 文本中是否包含代码/日志特征的启发式检测 */
const CODE_PATTERNS = [
  /^import\s+/m,           // 导入语句
  /^ERROR\s*:/m,           // 错误日志
  /^\s+at\s+\w+/m,         // 堆栈跟踪
  /\{[\s\S]*\}/m,          // JSON / 代码块
  /```/m,                   // Markdown 代码块
  /^\[.*\]\s+\w+/m,        // 日志格式 [timestamp] level
  /^\d{4}-\d{2}-\d{2}/m,   // 日期开头日志
  /^FAIL(ED)?\s*:/im,       // 测试失败
  /warning\s*:/im,          // 警告
  /^---\s/m,                // diff 分隔符
];

export class Interceptor {
  private adapter: PlatformAdapter;
  private api: TokenSlimAPIProxy;
  private config: TokenSlimConfig;
  private inputObserver: MutationObserver | null = null;
  private isProcessing = false;

  constructor(adapter: PlatformAdapter, api: TokenSlimAPIProxy, config: TokenSlimConfig) {
    this.adapter = adapter;
    this.api = api;
    this.config = config;
  }

  /**
   * 启动拦截监听。
   * 使用 MutationObserver 等待输入框出现，然后绑定事件。
   */
  start(): void {
    // 先尝试立即绑定
    const el = this.adapter.getInputEl();
    if (el) {
      this.bindInput(el);
      return;
    }

    // 输入框还没出现，用 MutationObserver 等待
    this.inputObserver = new MutationObserver(() => {
      const inputEl = this.adapter.getInputEl();
      if (inputEl) {
        this.bindInput(inputEl);
        this.inputObserver?.disconnect();
        this.inputObserver = null;
      }
    });

    this.inputObserver.observe(document.body, { childList: true, subtree: true });
  }

  /**
   * 停止拦截监听。
   */
  stop(): void {
    this.inputObserver?.disconnect();
    this.inputObserver = null;
  }

  /**
   * 绑定到具体输入框元素，注册提交拦截。
   */
  private bindInput(el: HTMLElement): void {
    this.adapter.interceptSubmit(async (originalText: string) => {
      return this.handleSubmission(el, originalText);
    });
  }

  /**
   * 处理提交文本：判断是否需要压缩，调用 API，返回替换文本。
   */
  private async handleSubmission(el: HTMLElement, text: string): Promise<string> {
    if (this.isProcessing) return text;

    // 检查是否应该跳过（文本太短或不含代码/日志特征）
    if (!this.shouldCompress(text)) {
      return text;
    }

    this.isProcessing = true;
    try {
      const result = await this.api.compress(text);
      const compressedText = this.formatCompressedPayload(result);
      const originalSize = new Blob([text]).size;
      const compressedSize = new Blob([compressedText]).size;

      // 显示压缩指示器
      this.adapter.showCompressionIndicator(el, originalSize, compressedSize);

      return compressedText;
    } catch (err) {
      console.warn('[TokenSlim] 压缩失败，使用原始文本:', err);
      return text;
    } finally {
      this.isProcessing = false;
    }
  }

  /**
   * 启发式判断：文本是否值得压缩。
   * 条件：超过阈值 且 包含代码/日志特征。
   */
  private shouldCompress(text: string): boolean {
    if (text.length < this.config.compressionThreshold) return false;
    return CODE_PATTERNS.some(p => p.test(text));
  }

  /**
   * 将压缩结果格式化为可注入 AI 聊天框的文本。
   */
  private formatCompressedPayload(result: CompressResult): string {
    const payload = JSON.stringify({
      tokens: result.tokens,
      dictionary: result.dictionary,
    });
    return `${COMPRESS_PREFIX}\n${payload}\n${COMPRESS_SUFFIX}`;
  }
}
