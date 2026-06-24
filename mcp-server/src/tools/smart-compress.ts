/**
 * smart_compress tool 实现
 * 自动判断是否值得压缩：仅当压缩率达到阈值时才返回压缩结果，否则返回原文。
 */

import { z } from "zod";
import { callCompress } from "../utils/cli.js";
import { recordCompression } from "../utils/session.js";

/** smart_compress tool 参数校验 */
export const smartCompressInputSchema = z.object({
  text: z.string().min(1, "text 不能为空"),
  threshold: z.number().min(0).max(1).optional().default(0.3),
});

export type SmartCompressInput = z.infer<typeof smartCompressInputSchema>;

/**
 * 处理 smart_compress 请求。
 */
export async function handleSmartCompress(
  cliPath: string,
  input: SmartCompressInput
): Promise<{ content: Array<{ type: "text"; text: string }> }> {
  const response = await callCompress(cliPath, input.text);

  if (response.status === "error" || !response.data) {
    throw new Error(response.error || "tokenslim compress 执行失败");
  }

  const originalSize = response.stats?.original_size ?? response.data.metadata.original_size;
  const compressedSize = response.stats?.compressed_size ?? response.data.metadata.compressed_size;
  const ratio =
    originalSize > 0 ? Number(((originalSize - compressedSize) / originalSize).toFixed(4)) : 0;

  const result: Record<string, unknown> = {
    threshold: input.threshold,
    achieved_ratio: ratio,
    original_size: originalSize,
    compressed_size: compressedSize,
  };

  if (ratio >= input.threshold) {
    recordCompression({ originalSize, compressedSize, ratio });
    result.compressed = JSON.stringify(response.data);
    result.decision = "compressed";
    result.reason = `压缩率达到 ${ratio * 100}%，满足阈值 ${input.threshold * 100}%。`;
  } else {
    result.compressed = input.text;
    result.decision = "skipped";
    result.reason = `压缩率仅 ${ratio * 100}%，未达到阈值 ${input.threshold * 100}%，返回原文以避免额外开销。`;
  }

  return {
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
  };
}

export const smartCompressTool = {
  name: "smart_compress",
  description:
    "智能压缩：当压缩率超过 threshold（默认 0.3）时返回压缩结果，否则返回原文并说明原因。",
  inputSchema: {
    type: "object" as const,
    properties: {
      text: { type: "string", description: "待压缩的原始文本" },
      threshold: {
        type: "number",
        default: 0.3,
        description: "触发压缩的最小压缩率（0-1），默认 0.3 表示至少节省 30%",
      },
    },
    required: ["text"],
  },
};
