/**
 * TokenSlim Chrome 扩展 - Content Script 入口
 * 功能：
 * 1. 检测 AI 回复中的 TokenSlim JSON 块 → 提供"还原日志"按钮（rehydrator）
 * 2. 拦截用户输入 → 自动压缩后发送（无感压缩 interceptor）
 */

import { TSRehydrator } from './rehydrator';
import { TokenSlimAPIProxy } from './api';
import { loadConfig, type TokenSlimConfig } from './config';
import { Interceptor } from './interceptor';
import type { PlatformAdapter } from './platform-adapter';
import { ChatGPTAdapter } from './platforms/chatgpt';
import { ClaudeAdapter } from './platforms/claude';
import { GeminiAdapter } from './platforms/gemini';
import { QianwenAdapter } from './platforms/qianwen';
import { WenxinAdapter } from './platforms/wenxin';

// ---------- 全局状态 ----------

const rehydrator = new TSRehydrator();
const api = new TokenSlimAPIProxy();
let interceptor: Interceptor | null = null;
let config: TokenSlimConfig | null = null;

// ---------- 平台适配器注册表 ----------

const adapters: PlatformAdapter[] = [
  new ChatGPTAdapter(),
  new ClaudeAdapter(),
  new GeminiAdapter(),
  new QianwenAdapter(),
  new WenxinAdapter(),
];

// ---------- 初始化 ----------

async function init() {
  // 加载配置
  config = await loadConfig();

  // 启动还原监听（检测 AI 回复中的 TokenSlim JSON）
  startRehydrationObserver();

  // 启动无感压缩拦截
  if (config.seamlessCompressionEnabled) {
    startSeamlessCompression();
  }

  // 监听配置变更（popup 修改配置时自动响应）
  chrome.storage.onChanged.addListener((changes, area) => {
    if (area === 'local' && changes['tokenslim_config']) {
      const newConfig = changes['tokenslim_config'].newValue as TokenSlimConfig;
      handleConfigChange(newConfig);
    }
  });
}

/**
 * 启动无感压缩：检测当前平台，选择合适的适配器并启动拦截器。
 */
function startSeamlessCompression() {
  if (!config) return;

  const adapter = adapters.find(a => {
    if (!a.match()) return false;
    const platformKey = a.name as keyof TokenSlimConfig['platforms'];
    return config!.platforms[platformKey] !== false;
  });

  if (!adapter) {
    console.log('[TokenSlim] 当前页面无匹配平台或未启用');
    return;
  }

  console.log(`[TokenSlim] 启动无感压缩，平台: ${adapter.name}`);
  interceptor = new Interceptor(adapter, api, config);
  interceptor.start();
}

/**
 * 处理配置变更：重启拦截器。
 */
function handleConfigChange(newConfig: TokenSlimConfig) {
  console.log('[TokenSlim] 配置已更新，重新初始化');
  interceptor?.stop();
  interceptor = null;
  config = newConfig;

  if (newConfig.seamlessCompressionEnabled) {
    startSeamlessCompression();
  }
}

// ---------- 还原功能（Rehydration） ----------

function startRehydrationObserver() {
  const observer = new MutationObserver((mutations) => {
    for (const mutation of mutations) {
      for (const node of mutation.addedNodes) {
        if (node instanceof HTMLElement) {
          processRehydrationNode(node);
        }
      }
    }
  });

  observer.observe(document.body, { childList: true, subtree: true });
  processRehydrationNode(document.body);
}

/**
 * 扫描 DOM 节点中的 TokenSlim JSON 块，注入"还原日志"按钮。
 */
function processRehydrationNode(root: HTMLElement) {
  const codeBlocks = root.querySelectorAll('pre, code');
  for (const block of codeBlocks) {
    if (block.hasAttribute('data-tokenslim-processed')) continue;

    const text = block.textContent || '';
    if (text.includes('"tokens":') && text.includes('"dictionary":')) {
      try {
        const json = JSON.parse(text);
        if (json.tokens && json.dictionary) {
          injectRestoreButton(block as HTMLElement, json);
        }
      } catch {
        // JSON 不完整或无效，跳过
      }
    }
  }
}

/**
 * 在代码块上注入"还原日志"按钮。
 */
function injectRestoreButton(target: HTMLElement, payload: any) {
  const container = target.parentElement;
  if (!container) return;

  target.setAttribute('data-tokenslim-processed', 'true');

  const button = document.createElement('button');
  button.innerText = 'TokenSlim: Restore Logs';
  button.className = 'tokenslim-restore-btn';

  button.onclick = () => {
    try {
      const restoredText = rehydrator.rehydrate(payload);
      target.textContent = restoredText;
      button.remove();

      const badge = document.createElement('span');
      badge.innerText = '\u2713 Restored by TokenSlim';
      badge.className = 'tokenslim-badge';
      target.prepend(badge);
    } catch (e) {
      console.error('TokenSlim Rehydration failed', e);
      button.innerText = 'Restoration Failed';
    }
  };

  if (container.style.position === '') {
    container.style.position = 'relative';
  }
  container.appendChild(button);
}

// 启动
init();
