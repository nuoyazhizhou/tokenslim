/**
 * TokenSlim Chrome 扩展 - Background Service Worker (MV3)
 * 职责：
 * 1. 中转 content_script 的 API 请求到 tokenslim-server（绕过 CORS）
 * 2. 管理扩展生命周期
 */

import { TokenSlimAPI } from './api';
import { loadConfig } from './config';
import type { ApiRequest, ApiResponse } from './api';

let api: TokenSlimAPI | null = null;

/**
 * 确保 API 实例已初始化（使用最新配置）。
 */
async function getApi(): Promise<TokenSlimAPI> {
  if (!api) {
    const config = await loadConfig();
    api = new TokenSlimAPI(config.serverUrl);
  }
  return api;
}

/**
 * 监听来自 content_script 的消息，转发到 tokenslim-server。
 */
chrome.runtime.onMessage.addListener(
  (msg: ApiRequest, _sender: chrome.runtime.MessageSender, sendResponse: (resp: ApiResponse) => void) => {
    handleRequest(msg)
      .then(data => sendResponse({ success: true, data }))
      .catch(err => sendResponse({ success: false, error: err instanceof Error ? err.message : String(err) }));

    // 返回 true 表示异步响应
    return true;
  }
);

async function handleRequest(msg: ApiRequest): Promise<any> {
  const apiInstance = await getApi();

  switch (msg.type) {
    case 'COMPRESS':
      return apiInstance.compress(msg.text);
    case 'DECOMPRESS':
      return apiInstance.decompress(msg.payload);
    case 'HEALTH':
      return apiInstance.health();
    case 'STATS':
      return apiInstance.stats();
    default:
      throw new Error(`未知请求类型: ${(msg as any).type}`);
  }
}

/**
 * 配置变更时重建 API 实例。
 */
chrome.storage.onChanged.addListener((changes: Record<string, chrome.storage.StorageChange>, area: string) => {
  if (area === 'local' && changes['tokenslim_config']) {
    const newConfig = changes['tokenslim_config'].newValue;
    if (newConfig?.serverUrl) {
      api = new TokenSlimAPI(newConfig.serverUrl);
    }
  }
});

// 扩展安装/更新时的初始化
chrome.runtime.onInstalled.addListener(() => {
  console.log('[TokenSlim Background] 扩展已安装/更新');
});
