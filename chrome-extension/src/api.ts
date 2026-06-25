/**
 * TokenSlim Chrome 扩展 - Server 通信层
 * 封装与 tokenslim-server REST API 的所有 HTTP 交互。
 * content_script 通过 background service worker 中转请求。
 */

import type { TokenSlimPayload } from './rehydrator';

// ---------- 类型定义 ----------

export interface CompressResult {
  tokens: any[];
  dictionary: TokenSlimPayload['dictionary'];
  metadata: {
    original_size: number;
    compressed_size: number;
    compression_ratio: number;
    plugin_used: string;
    exec_time_ms: number;
  };
}

export interface DecompressResult {
  text: string;
}

export interface StatsResult {
  total_commands: number;
  total_input_tokens: number;
  total_output_tokens: number;
  tokens_saved: number;
  savings_pct: number;
}

// ---------- Background ↔ Content Script 消息协议 ----------

export type ApiRequest =
  | { type: 'COMPRESS'; text: string }
  | { type: 'DECOMPRESS'; payload: TokenSlimPayload }
  | { type: 'HEALTH' }
  | { type: 'STATS' };

export type ApiResponse<T = any> =
  | { success: true; data: T }
  | { success: false; error: string };

// ---------- TokenSlimAPI 类 ----------

export class TokenSlimAPI {
  private baseUrl: string;

  constructor(baseUrl: string = 'http://127.0.0.1:10086') {
    // 去掉末尾斜杠，保持一致性
    this.baseUrl = baseUrl.replace(/\/+$/, '');
  }

  /**
   * 压缩文本，返回压缩结果（tokens + dictionary + metadata）。
   */
  async compress(text: string): Promise<CompressResult> {
    const res = await this.fetch('/compress', { text });
    return res as CompressResult;
  }

  /**
   * 解压 TokenSlim 压缩后的 payload，恢复原始文本。
   */
  async decompress(payload: TokenSlimPayload): Promise<DecompressResult> {
    const res = await this.fetch('/decompress', {
      tokens: payload.tokens,
      dictionary: payload.dictionary,
    });
    return res as DecompressResult;
  }

  /**
   * 检测 server 是否在线。
   */
  async health(): Promise<boolean> {
    try {
      const res = await this.fetch('/health');
      return (res as any)?.status === 'UP';
    } catch {
      return false;
    }
  }

  /**
   * 获取 server 累计统计数据。
   */
  async stats(): Promise<StatsResult> {
    return (await this.fetch('/stats/aggregate')) as StatsResult;
  }

  // ---------- 内部方法 ----------

  private async fetch(path: string, body?: unknown): Promise<any> {
    const url = `${this.baseUrl}${path}`;
    const headers: Record<string, string> = { 'Content-Type': 'application/json' };
    const init: RequestInit = { method: body ? 'POST' : 'GET', headers };
    if (body) {
      init.body = JSON.stringify(body);
    }

    const response = await window.fetch(url, init);
    if (!response.ok) {
      const errText = await response.text().catch(() => 'unknown');
      throw new Error(`TokenSlim API error ${response.status}: ${errText}`);
    }
    const text = await response.text();
    return text ? JSON.parse(text) : {};
  }
}

/**
 * 创建一个通过 background service worker 中转请求的 API 代理。
 * 用于 content_script 中（CORS 限制）。
 */
export class TokenSlimAPIProxy {
  async compress(text: string): Promise<CompressResult> {
    return this.sendRequest<CompressResult>({ type: 'COMPRESS', text });
  }

  async decompress(payload: TokenSlimPayload): Promise<DecompressResult> {
    return this.sendRequest<DecompressResult>({ type: 'DECOMPRESS', payload });
  }

  async health(): Promise<boolean> {
    try {
      return await this.sendRequest<boolean>({ type: 'HEALTH' });
    } catch {
      return false;
    }
  }

  async stats(): Promise<StatsResult> {
    return this.sendRequest<StatsResult>({ type: 'STATS' });
  }

  private sendRequest<T>(msg: ApiRequest): Promise<T> {
    return new Promise((resolve, reject) => {
      chrome.runtime.sendMessage(msg, (resp: ApiResponse<T>) => {
        if (chrome.runtime.lastError) {
          reject(new Error(chrome.runtime.lastError.message));
          return;
        }
        if (!resp) {
          reject(new Error('未收到 background 响应'));
          return;
        }
        if (resp.success) {
          resolve(resp.data);
        } else {
          reject(new Error(resp.error));
        }
      });
    });
  }
}
