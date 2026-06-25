/**
 * TokenSlim Chrome 扩展 - 配置管理
 * 管理扩展的全局配置项，支持用户自定义和持久化存储。
 */

export interface TokenSlimConfig {
  /** tokenslim-server 的地址 */
  serverUrl: string;
  /** 触发自动压缩的最小字符数阈值 */
  compressionThreshold: number;
  /** 是否启用无感压缩 */
  seamlessCompressionEnabled: boolean;
  /** 各平台启用状态 */
  platforms: {
    chatgpt: boolean;
    claude: boolean;
    gemini: boolean;
    qianwen: boolean;
    wenxin: boolean;
  };
}

const DEFAULT_CONFIG: TokenSlimConfig = {
  serverUrl: 'http://127.0.0.1:10086',
  compressionThreshold: 500,
  seamlessCompressionEnabled: true,
  platforms: {
    chatgpt: true,
    claude: true,
    gemini: true,
    qianwen: true,
    wenxin: true,
  },
};

const STORAGE_KEY = 'tokenslim_config';

/**
 * 从 chrome.storage.local 加载配置，缺失字段用默认值填充。
 */
export async function loadConfig(): Promise<TokenSlimConfig> {
  try {
    const result = await chrome.storage.local.get(STORAGE_KEY);
    const stored = result[STORAGE_KEY] as Partial<TokenSlimConfig> | undefined;
    if (!stored) return { ...DEFAULT_CONFIG };
    return { ...DEFAULT_CONFIG, ...stored, platforms: { ...DEFAULT_CONFIG.platforms, ...(stored.platforms || {}) } };
  } catch {
    return { ...DEFAULT_CONFIG };
  }
}

/**
 * 将配置持久化到 chrome.storage.local。
 */
export async function saveConfig(config: Partial<TokenSlimConfig>): Promise<void> {
  const current = await loadConfig();
  const merged = { ...current, ...config, platforms: { ...current.platforms, ...(config.platforms || {}) } };
  await chrome.storage.local.set({ [STORAGE_KEY]: merged });
}

/**
 * 重置为默认配置。
 */
export async function resetConfig(): Promise<TokenSlimConfig> {
  await chrome.storage.local.set({ [STORAGE_KEY]: DEFAULT_CONFIG });
  return { ...DEFAULT_CONFIG };
}
