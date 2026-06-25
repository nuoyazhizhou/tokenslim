/**
 * TokenSlim Node.js SDK - TypeScript 类型定义
 */

import { Readable } from 'stream';

export interface CompressResult {
  tokens: any[];
  dictionary: TokenSlimDictionary;
  metadata: {
    original_size: number;
    compressed_size: number;
    compression_ratio: number;
    plugin_used: string;
    exec_time_ms: number;
  };
}

export interface DecompressResult {
  text: string;
}

export interface TokenSlimDictionary {
  paths: Record<string, string>;
  packages: Record<string, string>;
  macros: Record<string, string>;
  files: Record<string, string>;
  directories: Record<string, string>;
  flags: Record<string, string>;
  custom: Record<string, Record<string, string>>;
}

export interface HealthResult {
  status: 'UP' | 'DOWN';
  version?: string;
}

export class TokenSlimClient {
  constructor(host?: string, port?: number);

  /** 检测 server 是否在线 */
  isHealthy(): Promise<boolean>;

  /** 压缩文本 */
  compress(text: string): Promise<CompressResult>;

  /** 解压 TokenSlim payload */
  decompress(tokens: any[], dictionary: TokenSlimDictionary): Promise<DecompressResult>;

  /** 压缩文件内容 */
  compressFile(filePath: string): Promise<CompressResult>;

  /** 流式压缩 Readable stream */
  compressStream(readable: Readable): Promise<CompressResult>;

  /** 批量压缩多段文本 */
  batchCompress(texts: string[], concurrency?: number): Promise<CompressResult[]>;
}

export default TokenSlimClient;
