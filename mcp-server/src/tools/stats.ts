/**
 * stats tool 实现
 * 返回本次 MCP Server 会话的累计压缩统计。
 */

import { z } from "zod";
import { getSessionStats } from "../utils/session.js";

/** stats tool 参数校验（无参数） */
export const statsInputSchema = z.object({});

export type StatsInput = z.infer<typeof statsInputSchema>;

/**
 * 处理 stats 请求。
 */
export async function handleStats(): Promise<{
  content: Array<{ type: "text"; text: string }>;
}> {
  const stats = getSessionStats();
  return {
    content: [{ type: "text", text: JSON.stringify(stats, null, 2) }],
  };
}

export const statsTool = {
  name: "stats",
  description: "返回本次会话累计压缩统计：总压缩次数、总节省 bytes、平均压缩率。",
  inputSchema: {
    type: "object" as const,
    properties: {},
  },
};
