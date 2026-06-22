/**
 * TokenSlim Node.js / TypeScript SDK
 *
 * Lightweight REST client for the TokenSlim server.
 * Default endpoint: http://127.0.0.1:10086
 *
 * @see https://github.com/nuoyazhizhou/tokenslim
 */

import * as http from 'http';
import { URL } from 'url';

/** Health-check response payload from `GET /health`. */
export interface HealthResponse {
    status: 'UP' | 'DOWN';
    version?: string;
    plugin_count?: number;
}

/** Compression request body for `POST /compress`. */
export interface CompressRequest {
    text: string;
    preset?: 'ai' | 'balanced' | 'lossless' | string;
    plugin_hint?: string;
}

/** Compression result. */
export interface CompressResponse {
    compressed: string;
    original_tokens: number;
    compressed_tokens: number;
    ratio: number;
    plugin_used: string;
    /** Dictionary mapping token placeholders (e.g. `$P1`) to original strings. */
    dictionary?: Record<string, string>;
}

/** Decompression request body for `POST /decompress`. */
export interface DecompressRequest {
    compressed: string;
    dictionary: Record<string, string>;
}

/** Decompression result. */
export interface DecompressResponse {
    text: string;
}

/** Error thrown when the SDK cannot reach the server or the server returns a non-2xx status. */
export class TokenSlimError extends Error {
    public readonly statusCode?: number;
    public readonly cause?: unknown;

    constructor(message: string, opts: { statusCode?: number; cause?: unknown } = {}) {
        super(message);
        this.name = 'TokenSlimError';
        this.statusCode = opts.statusCode;
        this.cause = opts.cause;
    }
}

/** SDK client options. */
export interface TokenSlimClientOptions {
    host?: string;
    port?: number;
    /** Request timeout in milliseconds. Default 30000. */
    timeoutMs?: number;
    /** Extra HTTP headers (e.g. authorization). */
    headers?: Record<string, string>;
}

/**
 * TokenSlim REST API client.
 *
 * @example
 * ```ts
 * import { TokenSlimClient } from 'tokenslim-sdk';
 *
 * const client = new TokenSlimClient();
 * if (await client.isHealthy()) {
 *     const r = await client.compress('git status output...');
 *     console.log(`${r.original_tokens} → ${r.compressed_tokens} (${(r.ratio * 100).toFixed(1)}%)`);
 * }
 * ```
 */
export class TokenSlimClient {
    private readonly host: string;
    private readonly port: number;
    private readonly timeoutMs: number;
    private readonly headers: Record<string, string>;

    constructor(opts: TokenSlimClientOptions = {}) {
        this.host = opts.host ?? '127.0.0.1';
        this.port = opts.port ?? 10086;
        this.timeoutMs = opts.timeoutMs ?? 30_000;
        this.headers = opts.headers ?? {};
    }

    /** Ping the server. Returns true if `GET /health` returns `status: UP`. */
    async isHealthy(): Promise<boolean> {
        try {
            const res = await this.request<HealthResponse>('GET', '/health');
            return res.status === 'UP';
        } catch {
            return false;
        }
    }

    /** Compress a text blob via the server. */
    async compress(text: string, opts: Partial<CompressRequest> = {}): Promise<CompressResponse> {
        if (typeof text !== 'string') {
            throw new TokenSlimError('compress() expects a string input');
        }
        const body: CompressRequest = { text, ...opts };
        return this.request<CompressResponse>('POST', '/compress', body);
    }

    /** Re-hydrate a compressed string back to the original text using its dictionary. */
    async decompress(compressed: string, dictionary: Record<string, string>): Promise<string> {
        const body: DecompressRequest = { compressed, dictionary };
        const res = await this.request<DecompressResponse>('POST', '/decompress', body);
        return res.text;
    }

    /** Get plugin registry metadata (count, families, available presets). */
    async describe(): Promise<{
        version: string;
        plugin_count: number;
        families: string[];
    }> {
        return this.request('GET', '/describe');
    }

    /** Underlying HTTP helper. */
    private async request<T>(method: 'GET' | 'POST', path: string, body?: unknown): Promise<T> {
        const payload = body !== undefined ? JSON.stringify(body) : undefined;
        const url = new URL(`http://${this.host}:${this.port}${path}`);

        const headers: Record<string, string> = {
            Accept: 'application/json',
            ...this.headers,
        };
        if (payload) {
            headers['Content-Type'] = 'application/json';
            headers['Content-Length'] = Buffer.byteLength(payload).toString();
        }

        return new Promise<T>((resolve, reject) => {
            const req = http.request(
                {
                    hostname: url.hostname,
                    port: url.port,
                    path: url.pathname,
                    method,
                    headers,
                },
                (res) => {
                    const chunks: Buffer[] = [];
                    res.on('data', (c: Buffer) => chunks.push(c));
                    res.on('end', () => {
                        const raw = Buffer.concat(chunks).toString('utf8');
                        const status = res.statusCode ?? 0;
                        if (status < 200 || status >= 300) {
                            reject(
                                new TokenSlimError(
                                    `TokenSlim ${method} ${path} failed: HTTP ${status} ${raw.slice(0, 256)}`,
                                    { statusCode: status },
                                ),
                            );
                            return;
                        }
                        try {
                            resolve(JSON.parse(raw) as T);
                        } catch (e) {
                            reject(
                                new TokenSlimError(`Invalid JSON from ${method} ${path}: ${raw.slice(0, 256)}`, {
                                    cause: e,
                                }),
                            );
                        }
                    });
                },
            );
            req.setTimeout(this.timeoutMs, () => {
                req.destroy(new TokenSlimError(`Timeout after ${this.timeoutMs}ms calling ${method} ${path}`));
            });
            req.on('error', (e) => reject(new TokenSlimError(`Network error: ${e.message}`, { cause: e })));
            if (payload) req.write(payload);
            req.end();
        });
    }
}

export default TokenSlimClient;
