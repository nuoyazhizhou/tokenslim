/**
 * compress_file tool 实现
 * 压缩本地文件内容，返回压缩结果与统计信息。
 */

import { resolve } from "node:path";
import { z } from "zod";
import { callCompressFile, findProjectRoot } from "../utils/cli.js";
import { recordCompression } from "../utils/session.js";

/** compress_file tool 参数校验 */
export const compressFileInputSchema = z.object({
  path: z.string().min(1, "path 不能为空"),
  mode: z.enum(["fast", "balanced", "max"]).optional().describe("压缩模式：fast|balanced|max"),
});

export type CompressFileInput = z.infer<typeof compressFileInputSchema>;

/** 将用户传入的 mode 映射为 CLI 识别的 preset */
function mapMode(mode?: string): string | undefined {
  if (!mode) return undefined;
  return mode === "max" ? "ai" : mode;
}

/**
 * 处理 compress_file 请求。
 */
export async function handleCompressFile(
  cliPath: string,
  input: CompressFileInput
): Promise<{ content: Array<{ type: "text"; text: string }> }> {
  const mode = mapMode(input.mode);
  // 将相对路径解析为绝对路径，避免 CLI 的 cwd 切换导致路径歧义
  const absolutePath = resolve(findProjectRoot(cliPath), input.path);
  const response = await callCompressFile(cliPath, absolutePath, mode);

  if (response.status === "error" || !response.data) {
    throw new Error(response.error || "tokenslim compress-file 执行失败");
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
      source_file: absolutePath,
    },
  };

  return {
    content: [{ type: "text", text: JSON.stringify(result, null, 2) }],
  };
}

export const compressFileTool = {
  name: "compress_file",
  description: "压缩本地文件内容。path 为绝对路径或相对当前工作目录的路径。",
  inputSchema: {
    type: "object" as const,
    properties: {
      path: { type: "string", description: "待压缩的文件路径" },
      mode: {
        type: "string",
        enum: ["fast", "balanced", "max"],
        description: "压缩模式，max 对应 CLI 的 ai preset",
      },
    },
    required: ["path"],
  },
};
