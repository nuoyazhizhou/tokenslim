/**
 * decompress tool 实现
 * 接收 TokenSlim 压缩后的 JSON，还原为原始文本。
 */

import { z } from "zod";
import { callDecompress } from "../utils/cli.js";

/** decompress tool 参数校验 */
export const decompressInputSchema = z.object({
  text: z.string().min(1, "text 不能为空"),
});

export type DecompressInput = z.infer<typeof decompressInputSchema>;

/**
 * 从用户输入中提取原始 CompressionOutput JSON。
 * 兼容直接传入的 CompressionOutput 以及被 status/data 包装过的结果。
 */
function extractCompressedJson(text: string): string {
  const trimmed = text.trim();
  if (!trimmed.startsWith("{")) {
    return trimmed;
  }
  try {
    const parsed = JSON.parse(trimmed);
    if (parsed && typeof parsed === "object" && "data" in parsed && parsed.data) {
      return JSON.stringify(parsed.data);
    }
  } catch {
    // 不是 JSON，按原样返回
  }
  return trimmed;
}

/**
 * 处理 decompress 请求。
 */
export async function handleDecompress(
  cliPath: string,
  input: DecompressInput
): Promise<{ content: Array<{ type: "text"; text: string }> }> {
  const compressedJson = extractCompressedJson(input.text);
  const response = await callDecompress(cliPath, compressedJson);

  if (response.status === "error" || !response.data) {
    throw new Error(response.error || "tokenslim decompress 执行失败");
  }

  return {
    content: [{ type: "text", text: response.data.text }],
  };
}

export const decompressTool = {
  name: "decompress",
  description: "将 TokenSlim 压缩结果还原为原始文本。",
  inputSchema: {
    type: "object" as const,
    properties: {
      text: { type: "string", description: "TokenSlim 压缩后的 JSON 字符串" },
    },
    required: ["text"],
  },
};
