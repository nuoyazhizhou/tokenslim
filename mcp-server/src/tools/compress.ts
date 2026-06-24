/**
 * compress tool 实现
 * 接收文本并返回 TokenSlim 压缩后的结果与统计信息。
 */

import { z } from "zod";
import { callCompress } from "../utils/cli.js";
import { recordCompression } from "../utils/session.js";


/** compress tool 参数校验 */
export const compressInputSchema = z.object({
  text: z.string().min(1, "text 不能为空"),
  mode: z.enum(["fast", "balanced", "max"]).optional().describe("压缩模式：fast|balanced|max"),
  plugins: z.array(z.string()).optional().describe("期望启用的插件列表（当前由 CLI 自动路由）"),
});

export type CompressInput = z.infer<typeof compressInputSchema>;

/** 将用户传入的 mode 映射为 CLI 识别的 preset */
function mapMode(mode?: string): string | undefined {
  if (!mode) return undefined;
  // MCP 接口使用 max 表示最大压缩，CLI 使用 ai preset
  return mode === "max" ? "ai" : mode;
}

/**
 * 处理 compress 请求。
 */
export async function handleCompress(
  cliPath: string,
  input: CompressInput
): Promise<{ content: Array<{ type: "text"; text: string }> }> {
  const mode = mapMode(input.mode);
  const response = await callCompress(cliPath, input.text, mode);

  if (response.status === "error" || !response.data) {
    throw new Error(response.error || "tokenslim compress 执行失败");
  }

  const originalSize = response.stats?.original_size ?? response.data.metadata.original_size;
  const compressedSize = response.stats?.compressed_size ?? response.data.metadata.compressed_size;
  const ratio =
    originalSize > 0 ? Number(((originalSize - compressedSize) / originalSize).toFixed(4)) : 0;

  recordCompression({ originalSize, compressedSize, ratio });

  const result = {
    compressed: JSON.stringify(response.data),
    stats: {
      original_size: originalSize,
      compressed_size: compressedSize,
      compression_ratio: ratio,
      used_plugins: input.plugins ?? [],
      note: "当前由 TokenSlim CLI 自动选择插件；如需精确控制，请在配置文件中定义路由。",
    },
  };

  return {
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
  };
}

export const compressTool = {
  name: "compress",
  description:
    "使用 TokenSlim 压缩任意文本。返回可被 decompress 还原的 CompressionOutput JSON 与统计信息。",
  inputSchema: {
    type: "object" as const,
    properties: {
      text: { type: "string", description: "待压缩的原始文本" },
      mode: {
        type: "string",
        enum: ["fast", "balanced", "max"],
        description: "压缩模式，max 对应 CLI 的 ai preset",
      },
      plugins: {
        type: "array",
        items: { type: "string" },
        description: "期望启用的插件名称列表（当前版本由 CLI 自动路由）",
      },
    },
    required: ["text"],
  },
};
