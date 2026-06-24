/**
 * tokenslim://config resource 实现
 * 返回当前 TokenSlim 环境信息与项目配置摘要。
 */

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { callEnvJson, checkCliVersion } from "../utils/cli.js";

export const CONFIG_RESOURCE_URI = "tokenslim://config";
export const CONFIG_RESOURCE_NAME = "TokenSlim Config";

/**
 * 读取当前工作目录下的 .tokenslim.toml（如果存在），返回字符串内容。
 */
function readProjectConfig(): string | undefined {
  const candidates = [
    resolve(process.cwd(), ".tokenslim.toml"),
    resolve(process.cwd(), "..", ".tokenslim.toml"),
  ];
  for (const path of candidates) {
    try {
      return readFileSync(path, "utf-8");
    } catch {
      continue;
    }
  }
  return undefined;
}

/**
 * 读取 config/plugins.toml（如果存在），返回字符串内容。
 */
function readPluginsConfig(): string | undefined {
  const candidates = [
    resolve(process.cwd(), "config", "plugins.toml"),
    resolve(process.cwd(), "..", "config", "plugins.toml"),
  ];
  for (const path of candidates) {
    try {
      return readFileSync(path, "utf-8");
    } catch {
      continue;
    }
  }
  return undefined;
}

/**
 * 读取 config 资源内容。
 */
export async function readConfigResource(cliPath: string): Promise<{
  uri: string;
  mimeType: string;
  text: string;
}> {
  const version = await checkCliVersion(cliPath).catch(() => "unknown");
  const envInfo = await callEnvJson(cliPath).catch(() => ({}));

  const config = {
    cli: {
      path: cliPath,
      version,
    },
    environment: envInfo,
    project_config: readProjectConfig(),
    plugins_config: readPluginsConfig(),
  };

  return {
    uri: CONFIG_RESOURCE_URI,
    mimeType: "application/json",
    text: JSON.stringify(config, null, 2),
  };
}
