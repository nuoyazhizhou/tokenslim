/**
 * 会话级统计管理
 * 维护本次 MCP Server 运行期间的压缩次数、节省字节数与平均压缩率。
 */

import type { CompressionStats, SessionStats } from "./types.js";

/** 内部累计数据 */
const session = {
  totalCompressions: 0,
  totalBytesSaved: 0,
  totalRatioSum: 0,
};

/**
 * 记录一次压缩结果，更新会话统计。
 */
export function recordCompression(stats: CompressionStats): void {
  const saved = Math.max(0, stats.originalSize - stats.compressedSize);
  session.totalCompressions += 1;
  session.totalBytesSaved += saved;
  session.totalRatioSum += stats.ratio;
}

/**
 * 获取当前会话累计统计。
 */
export function getSessionStats(): SessionStats {
  const averageRatio =
    session.totalCompressions > 0
      ? session.totalRatioSum / session.totalCompressions
      : 0;
  return {
    totalCompressions: session.totalCompressions,
    totalBytesSaved: session.totalBytesSaved,
    averageRatio: Number(averageRatio.toFixed(4)),
  };
}
