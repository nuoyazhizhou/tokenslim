# tokenslim

[![npm version](https://img.shields.io/npm/v/tokenslim.svg)](https://www.npmjs.com/package/tokenslim)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](./LICENSE)

Node.js / TypeScript SDK for [TokenSlim](https://github.com/nuoyazhizhou/tokenslim) — a high-performance Rust compression engine for LLM inputs.

Save **50%–95% tokens** on VCS logs, build output, runtime traces, and structured text before sending them to an LLM.

## Install

```bash
npm install tokenslim
# or
pnpm add tokenslim
# or
yarn add tokenslim
```

Requires Node.js **>= 18**.

You also need the TokenSlim server running. Two easy options:

```bash
# Option A — install the Rust CLI globally
cargo install tokenslim
tokenslim serve --port 10086

# Option B — Docker
docker run -d -p 10086:10086 ghcr.io/nuoyazhizhou/tokenslim:latest
```

## Quickstart (10 lines)

```ts
import { TokenSlimClient } from 'tokenslim';

const client = new TokenSlimClient();              // default http://127.0.0.1:10086
if (await client.isHealthy()) {
    const r = await client.compress(longBuildLog, { preset: 'ai' });
    console.log(`${r.original_tokens} → ${r.compressed_tokens} tokens (saved ${(100 - r.ratio * 100).toFixed(1)}%)`);
    // Later, re-hydrate:
    const original = await client.decompress(r.compressed, r.dictionary ?? {});
}
```

That's it. No configuration files, no plugin selection — TokenSlim auto-detects the input type and routes to the right plugin (git log, pytest, gradle, JSON, …).

## API Reference

### `new TokenSlimClient(opts?)`

| Option | Default | Description |
|---|---|---|
| `host` | `127.0.0.1` | TokenSlim server hostname |
| `port` | `10086` | TokenSlim server port |
| `timeoutMs` | `30000` | Per-request timeout |
| `headers` | `{}` | Extra HTTP headers (auth, etc.) |

### `client.isHealthy(): Promise<boolean>`

Pings `GET /health`. Returns `true` only when the server reports `status: UP`.

### `client.compress(text, opts?): Promise<CompressResponse>`

Compresses a string. `opts` is optional:

```ts
{
    preset?: 'ai' | 'balanced' | 'lossless' | string;   // default 'balanced'
    plugin_hint?: string;                                // e.g. 'vcs_git_plugin'
}
```

Returns:

```ts
{
    compressed: string;             // compressed output with $P/$T token placeholders
    original_tokens: number;        // input token count
    compressed_tokens: number;      // output token count
    ratio: number;                  // compressed_tokens / original_tokens (0.05 = 95% saving)
    plugin_used: string;            // 'vcs_git_plugin', 'pytest_plugin', ...
    dictionary?: Record<string, string>;  // token → original
}
```

### `client.decompress(compressed, dictionary): Promise<string>`

Re-hydrates a compressed string back to its original text. Requires the dictionary returned by `compress()`.

### `client.describe(): Promise<{ version, plugin_count, families }>`

Returns server metadata.

### `TokenSlimError`

Thrown on network or non-2xx responses. Has `.statusCode` and `.cause` fields.

```ts
import { TokenSlimClient, TokenSlimError } from 'tokenslim';

try {
    await client.compress(text);
} catch (e) {
    if (e instanceof TokenSlimError && e.statusCode === 503) {
        console.error('TokenSlim server overloaded');
    }
}
```

## Common Recipes

### Wrap a long `git log` for an LLM agent

```ts
const r = await client.compress(gitLogOutput, { plugin_hint: 'vcs_git_plugin' });
return `${r.compressed}\n\n[Dictionary]\n${JSON.stringify(r.dictionary)}`;
```

### Compress test output and assert savings

```ts
const r = await client.compress(pytestOutput);
if (r.ratio > 0.5) {
    throw new Error(`Compression too lossy: ratio=${r.ratio}`);
}
```

### Health-aware retry loop

```ts
async function compressWithRetry(text: string, maxTries = 3): Promise<CompressResponse> {
    for (let i = 0; i < maxTries; i++) {
        if (await client.isHealthy()) return client.compress(text);
        await new Promise((r) => setTimeout(r, 500 * (i + 1)));
    }
    throw new Error('TokenSlim server unavailable');
}
```

## License

MIT — see [LICENSE](./LICENSE).
