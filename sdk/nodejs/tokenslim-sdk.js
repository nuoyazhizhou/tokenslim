const http = require('http');
const { Readable } = require('stream');

/**
 * TokenSlim REST API Client for Node.js.
 * 支持 compress / decompress / compressFile / compressStream / batchCompress。
 * @param {string} host 
 * @param {number} port 
 */
class TokenSlimClient {
    constructor(host = "127.0.0.1", port = 10086) {
        this.host = host;
        this.port = port;
        this.maxRetries = 3;
        this.retryDelay = 1000; // ms
    }

    /**
     * 检测 server 是否在线。
     * @returns {Promise<boolean>}
     */
    async isHealthy() {
        try {
            const res = await this._makeRequest('GET', '/health');
            return res.status === 'UP';
        } catch (e) {
            return false;
        }
    }

    /**
     * 压缩文本。
     * @param {string} text 
     * @returns {Promise<CompressResult>}
     */
    async compress(text) {
        return this._makeRequest('POST', '/compress', { text });
    }

    /**
     * 解压 TokenSlim payload。
     * @param {any[]} tokens 
     * @param {object} dictionary 
     * @returns {Promise<DecompressResult>}
     */
    async decompress(tokens, dictionary) {
        return this._makeRequest('POST', '/decompress', { tokens, dictionary });
    }

    /**
     * 压缩文件内容（读取整个文件后发送）。
     * @param {string} filePath 
     * @returns {Promise<CompressResult>}
     */
    async compressFile(filePath) {
        const fs = require('fs');
        const text = fs.readFileSync(filePath, 'utf-8');
        return this.compress(text);
    }

    /**
     * 流式压缩：收集 Readable stream 的全部数据后压缩。
     * @param {Readable} readable 
     * @returns {Promise<CompressResult>}
     */
    async compressStream(readable) {
        const chunks = [];
        for await (const chunk of readable) {
            chunks.push(typeof chunk === 'string' ? chunk : chunk.toString('utf-8'));
        }
        return this.compress(chunks.join(''));
    }

    /**
     * 批量压缩多段文本。
     * @param {string[]} texts 
     * @param {number} [concurrency=5] 并发数
     * @returns {Promise<CompressResult[]>}
     */
    async batchCompress(texts, concurrency = 5) {
        const results = [];
        for (let i = 0; i < texts.length; i += concurrency) {
            const batch = texts.slice(i, i + concurrency);
            const batchResults = await Promise.all(batch.map(t => this.compress(t)));
            results.push(...batchResults);
        }
        return results;
    }

    /**
     * 内部 HTTP 请求，带自动重连重试。
     * @param {string} method 
     * @param {string} path 
     * @param {object|null} body 
     * @param {number} [attempt=0]
     * @returns {Promise<any>}
     */
    _makeRequest(method, path, body = null, attempt = 0) {
        return new Promise((resolve, reject) => {
            const data = body ? JSON.stringify(body) : '';
            const options = {
                hostname: this.host,
                port: this.port,
                path: path,
                method: method,
                headers: {
                    'Content-Type': 'application/json',
                    'Content-Length': Buffer.byteLength(data)
                }
            };

            const req = http.request(options, (res) => {
                let resData = '';
                res.on('data', (chunk) => resData += chunk);
                res.on('end', () => {
                    if (res.statusCode >= 200 && res.statusCode < 300) {
                        try {
                            resolve(resData ? JSON.parse(resData) : {});
                        } catch (e) {
                            reject(new Error(`Failed to parse response: ${e.message}`));
                        }
                    } else {
                        reject(new Error(`Server returned status ${res.statusCode}: ${resData}`));
                    }
                });
            });

            req.on('error', (err) => {
                // 自动重连：网络错误时重试
                if (attempt < this.maxRetries && (err.code === 'ECONNREFUSED' || err.code === 'ECONNRESET')) {
                    setTimeout(() => {
                        this._makeRequest(method, path, body, attempt + 1)
                            .then(resolve)
                            .catch(reject);
                    }, this.retryDelay * (attempt + 1));
                } else {
                    reject(err);
                }
            });

            if (data) req.write(data);
            req.end();
        });
    }
}

module.exports = TokenSlimClient;

// 示例用法
if (require.main === module) {
    const client = new TokenSlimClient();
    client.isHealthy().then(healthy => {
        if (healthy) {
            console.log("Connected to TokenSlim Server.");
            client.compress("sample log line").then(res => {
                console.log(`Compressed! Ratio: ${res.metadata.compression_ratio}`);
            });
        } else {
            console.log("Server offline.");
        }
    });
}
const http = require('http');

class TokenSlimClient {
    /**
     * TokenSlim REST API Client for Node.js.
     * @param {string} host 
     * @param {number} port 
     */
    constructor(host = "127.0.0.1", port = 10086) {
        this.host = host;
        this.port = port;
    }

    async isHealthy() {
        try {
            const res = await this._makeRequest('GET', '/health');
            return res.status === 'UP';
        } catch (e) {
            return false;
        }
    }

    async compress(text) {
        return this._makeRequest('POST', '/compress', { text });
    }

    async decompress(tokens, dictionary) {
        return this._makeRequest('POST', '/decompress', { tokens, dictionary });
    }

    _makeRequest(method, path, body = null) {
        return new Promise((resolve, reject) => {
            const data = body ? JSON.stringify(body) : '';
            const options = {
                hostname: this.host,
                port: this.port,
                path: path,
                method: method,
                headers: {
                    'Content-Type': 'application/json',
                    'Content-Length': Buffer.byteLength(data)
                }
            };

            const req = http.request(options, (res) => {
                let resData = '';
                res.on('data', (chunk) => resData += chunk);
                res.on('end', () => {
                    if (res.statusCode >= 200 && res.statusCode < 300) {
                        try {
                            resolve(resData ? JSON.parse(resData) : {});
                        } catch (e) {
                            reject(new Error(`Failed to parse response: ${e.message}`));
                        }
                    } else {
                        reject(new Error(`Server returned status ${res.statusCode}: ${resData}`));
                    }
                });
            });

            req.on('error', reject);
            if (data) req.write(data);
            req.end();
        });
    }
}

module.exports = TokenSlimClient;

// Example Usage
if (require.main === module) {
    const client = new TokenSlimClient();
    client.isHealthy().then(healthy => {
        if (healthy) {
            console.log("Connected to TokenSlim Server.");
            client.compress("sample log line").then(res => {
                console.log(`Compressed! Ratio: ${res.metadata.compression_ratio}`);
            });
        } else {
            console.log("Server offline.");
        }
    });
}
