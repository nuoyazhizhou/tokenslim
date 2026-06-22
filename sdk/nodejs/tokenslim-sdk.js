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
