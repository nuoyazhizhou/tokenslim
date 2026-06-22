# @tokenslim/cli-binary-linux-x64-gnu

Prebuilt TokenSlim CLI binaries for **Linux x64 (glibc)**.

This package is an internal `optionalDependency` of [`tokenslim`](https://www.npmjs.com/package/tokenslim).
It is selected automatically by npm/pnpm/yarn when you install on a Linux x64
host. **Do not depend on it directly** — it has no public API surface; the
binaries it contains are loaded by `bin/tokenslim.js` / `bin/tokenslim-server.js`
in the parent package.

## What's inside

- `bin/tokenslim` — main CLI (the one with `tokenslim run` / `serve` / `decompress` / `plugins` / …)
- `bin/tokenslim-server` — standalone HTTP server (`/health`, `/compress`, `/decompress`)
- `config/` — plugin configs (`plugins/`), framework configs (`frameworks/`), language configs (`languages/`), and root `.toml`/`.json` files

## Replacement policy

The maintainer republishes a new version of this package on every
`tokenslim` release. `optionalDependencies` is pinned to the same
version as the parent SDK so a `npm update` always pulls matching
binaries.

## License

[MIT](../../LICENSE)
