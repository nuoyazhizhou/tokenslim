/**
 * 共享类型定义
 * 描述 TokenSlim CLI 的 JSON 输出结构以及 MCP Server 内部统计类型。
 */

/** CLI 压缩成功时的 data 字段：TokenSlim CompressionOutput 的 JSON 表示 */
export interface CompressionOutputData {
  tokens: unknown[];
  dictionary: Record<string, unknown>;
  metadata: CompressionMetadata;
}

/** CLI 压缩元数据 */
export interface CompressionMetadata {
  original_size: number;
  compressed_size: number;
  original_tokens: number;
  compressed_tokens: number;
  token_savings: number;
  compression_ratio: number;
  token_ratio: number;
  slice_count: number;
  processing_time_ms: number;
}

/** CLI 在 --json 模式下输出的包装结构 */
export interface CliJsonResponse<T = unknown> {
  status: "success" | "error";
  data?: T;
  stats?: { original_size: number; compressed_size: number };
  error?: string;
  code?: string;
}

/** CLI 解压成功时的 data 字段 */
export interface DecompressionData {
  text: string;
}

/** 单次压缩操作的统计摘要 */
export interface CompressionStats {
  originalSize: number;
  compressedSize: number;
  ratio: number;
}

/** 会话级累计统计 */
export interface SessionStats {
  totalCompressions: number;
  totalBytesSaved: number;
  averageRatio: number;
}

/** 单个插件的描述信息 */
export interface PluginInfo {
  name: string;
  category?: string;
  description: string;
}
