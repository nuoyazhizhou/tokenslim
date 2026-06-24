/**
 * tokenslim://plugins resource 实现
 * 返回 TokenSlim 可用插件列表及描述。
 */

import { callPlugins } from "../utils/cli.js";
import type { PluginInfo } from "../utils/types.js";

export const PLUGINS_RESOURCE_URI = "tokenslim://plugins";
export const PLUGINS_RESOURCE_NAME = "TokenSlim Plugins";

/**
 * 解析 `tokenslim plugins` 的文本输出。
 * 识别类别标题与 `- name : description` 格式的插件行。
 */
export function parsePluginsOutput(text: string): PluginInfo[] {
  const plugins: PluginInfo[] = [];
  const lines = text.split(/\r?\n/);
  let currentCategory = "";

  for (const rawLine of lines) {
    const line = rawLine.trim();
    if (!line) continue;

    // 类别标题形如：[命令路由 / 环境探测]
    const categoryMatch = line.match(/^\[(.+?)\]\s*$/);
    if (categoryMatch) {
      currentCategory = categoryMatch[1].trim();
      continue;
    }

    // 插件行形如：- android_gradle   : Android Gradle 构建日志脱水
    const pluginMatch = line.match(/^-\s+(\S+)\s*:\s*(.+)$/);
    if (pluginMatch) {
      const name = pluginMatch[1].trim();
      const description = pluginMatch[2].trim();
      // 忽略纯占位行（例如 vcs : -）
      if (description === "-") continue;
      plugins.push({ name, category: currentCategory, description });
      continue;
    }

    // 处理被换行截断的描述（上一行插件描述的续行）
    if (plugins.length > 0 && !line.startsWith("-") && !line.startsWith("[")) {
      const last = plugins[plugins.length - 1];
      last.description += " " + line;
    }
  }

  return plugins;
}

/**
 * 读取 plugins 资源内容。
 */
export async function readPluginsResource(cliPath: string): Promise<{
  uri: string;
  mimeType: string;
  text: string;
}> {
  const rawText = await callPlugins(cliPath);
  const plugins = parsePluginsOutput(rawText);

  const result = {
    plugins,
    raw_text: rawText,
    note: "已尽量解析文本输出；若解析不完整，可查看 raw_text 字段。",
  };

  return {
    uri: PLUGINS_RESOURCE_URI,
    mimeType: "application/json",
    text: JSON.stringify(result, null, 2),
  };
}
