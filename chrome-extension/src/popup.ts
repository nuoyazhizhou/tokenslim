/**
 * TokenSlim Chrome 扩展 - Popup 控制面板逻辑
 * 读取/写入配置，显示 server 状态和统计数据。
 */

import { loadConfig, saveConfig, type TokenSlimConfig } from './config';
import { TokenSlimAPI } from './api';

// ---------- DOM 元素 ----------

const $ = (id: string) => document.getElementById(id) as HTMLElement | null;

const statusDot = $('statusDot') as HTMLDivElement;
const statusText = $('statusText') as HTMLSpanElement;
const enableToggle = $('enableToggle') as HTMLInputElement;
const thresholdSlider = $('thresholdSlider') as HTMLInputElement;
const thresholdValue = $('thresholdValue') as HTMLSpanElement;
const serverUrlInput = $('serverUrl') as HTMLInputElement;
const statCommands = $('statCommands') as HTMLDivElement;
const statSaved = $('statSaved') as HTMLDivElement;

const platformCheckboxes: Record<string, HTMLInputElement> = {
  chatgpt: $('p_chatgpt') as HTMLInputElement,
  claude: $('p_claude') as HTMLInputElement,
  gemini: $('p_gemini') as HTMLInputElement,
  qianwen: $('p_qianwen') as HTMLInputElement,
  wenxin: $('p_wenxin') as HTMLInputElement,
};

// ---------- 初始化 ----------

async function init() {
  const config = await loadConfig();
  renderConfig(config);
  await checkServerStatus(config.serverUrl);
  await loadStats(config.serverUrl);
  bindEvents();
}

/**
 * 将配置渲染到 UI。
 */
function renderConfig(config: TokenSlimConfig) {
  enableToggle.checked = config.seamlessCompressionEnabled;
  thresholdSlider.value = String(config.compressionThreshold);
  thresholdValue.textContent = `${config.compressionThreshold} chars`;
  serverUrlInput.value = config.serverUrl;

  for (const [key, checkbox] of Object.entries(platformCheckboxes)) {
    const platformKey = key as keyof TokenSlimConfig['platforms'];
    checkbox.checked = config.platforms[platformKey] !== false;
  }
}

/**
 * 检测 server 在线状态。
 */
async function checkServerStatus(url: string) {
  const api = new TokenSlimAPI(url);
  const online = await api.health();

  statusDot.className = `status-dot ${online ? 'online' : 'offline'}`;
  statusText.textContent = online ? 'Server Online' : 'Server Offline';
}

/**
 * 加载 server 统计数据。
 */
async function loadStats(url: string) {
  try {
    const api = new TokenSlimAPI(url);
    const stats = await api.stats();
    statCommands.textContent = String(stats.total_commands || 0);
    statSaved.textContent = formatTokens(stats.tokens_saved || 0);
  } catch {
    statCommands.textContent = '--';
    statSaved.textContent = '--';
  }
}

/**
 * 绑定 UI 事件。
 */
function bindEvents() {
  // 无感压缩开关
  enableToggle.addEventListener('change', () => {
    saveConfig({ seamlessCompressionEnabled: enableToggle.checked });
  });

  // 压缩阈值滑块
  thresholdSlider.addEventListener('input', () => {
    const val = Number(thresholdSlider.value);
    thresholdValue.textContent = `${val} chars`;
  });
  thresholdSlider.addEventListener('change', () => {
    saveConfig({ compressionThreshold: Number(thresholdSlider.value) });
  });

  // Server URL
  serverUrlInput.addEventListener('change', () => {
    const url = serverUrlInput.value.trim();
    if (url) {
      saveConfig({ serverUrl: url });
      checkServerStatus(url);
      loadStats(url);
    }
  });

  // 平台勾选
  for (const [key, checkbox] of Object.entries(platformCheckboxes)) {
    checkbox.addEventListener('change', async () => {
      const config = await loadConfig();
      const platforms = { ...config.platforms };
      (platforms as any)[key] = checkbox.checked;
      saveConfig({ platforms });
    });
  }
}

function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1000000) return `${(n / 1000).toFixed(1)}K`;
  return `${(n / 1000000).toFixed(1)}M`;
}

// 启动
init();
