#!/usr/bin/env node
// packages/sdk-nodejs/scripts/postinstall.js
//
// npm `postinstall` hook. Runs after `npm install tokenslim`.
//
// What we do, in order:
//   1. If the matching @tokenslim/cli-binary-* optional package is already
//      on disk (npm/yarn/pnpm usually pick the right one automatically),
//      we just `chmod +x` the binaries and exit. This is the common case.
//   2. If no optional package is present (e.g. user passed
//      `--ignore-optional`, or we run on an unsupported platform), we try
//      to download a matching tarball from the GitHub Release tagged
//      "v<package-version>". This requires network access.
//   3. If the download also fails, we log a friendly hint and exit 0.
//      We intentionally don't fail the install — the SDK JS still works
//      as a REST client even without the native binary; users who only
//      need the SDK and have a server elsewhere can ignore the warning.

const fs = require("node:fs");
const path = require("node:path");
const https = require("node:https");

const PKG_VERSION = require("../package.json").version;
const PACKAGE_NAME = "tokenslim";

// Map (platform, arch) → optional-dependency package name.
const PLATFORM_PKG = {
  "linux:x64": "@tokenslim/cli-binary-linux-x64-gnu",
  "linux:arm64": "@tokenslim/cli-binary-linux-arm64-gnu",
  "darwin:x64": "@tokenslim/cli-binary-darwin-x64",
  "darwin:arm64": "@tokenslim/cli-binary-darwin-arm64",
  "win32:x64": "@tokenslim/cli-binary-windows-x64",
  "win32:arm64": "@tokenslim/cli-binary-windows-arm64",
};

function info(msg) {
  process.stdout.write(`[${PACKAGE_NAME}/postinstall] ${msg}\n`);
}

function warn(msg) {
  process.stderr.write(`[${PACKAGE_NAME}/postinstall] WARN: ${msg}\n`);
}

function vendorDir() {
  return path.join(__dirname, "..", "vendor");
}

function chmodX(file) {
  if (process.platform === "win32") return; // no-op on Windows
  try {
    fs.chmodSync(file, 0o755);
  } catch (e) {
    warn(`chmod +x failed for ${file}: ${e.message}`);
  }
}

function findOptionalBinary() {
  const key = `${process.platform}:${process.arch}`;
  const pkgName = PLATFORM_PKG[key];
  if (!pkgName) return null;
  try {
    const pkgJson = require.resolve(`${pkgName}/package.json`, {
      paths: [
        path.join(__dirname, "..", "..", ".."),
        path.join(__dirname, "..", ".."),
      ],
    });
    const binDir = path.join(path.dirname(pkgJson), "bin");
    if (!fs.existsSync(binDir)) return null;
    return { pkgName, binDir };
  } catch {
    return null;
  }
}

/**
 * Stream a URL to a local file via the built-in `https` module — no
 * external deps.  Resolves on 2xx, rejects on non-2xx / network error.
 */
function downloadToFile(url, dest, redirectsLeft = 5) {
  return new Promise((resolve, reject) => {
    const req = https.get(url, (res) => {
      // Handle GitHub Release CDN redirects (302 → S3).
      if (
        res.statusCode &&
        res.statusCode >= 300 &&
        res.statusCode < 400 &&
        res.headers.location
      ) {
        if (redirectsLeft <= 0) {
          reject(new Error("too many redirects"));
          return;
        }
        res.resume();
        downloadToFile(res.headers.location, dest, redirectsLeft - 1)
          .then(resolve)
          .catch(reject);
        return;
      }
      if (!res.statusCode || res.statusCode >= 400) {
        reject(new Error(`HTTP ${res.statusCode} for ${url}`));
        return;
      }
      const file = fs.createWriteStream(dest);
      res.pipe(file);
      file.on("finish", () => file.close(() => resolve(dest)));
      file.on("error", reject);
    });
    req.on("error", reject);
    req.setTimeout(60_000, () => req.destroy(new Error("download timeout")));
  });
}

async function downloadFromGitHubRelease(binName) {
  const tag = `v${PKG_VERSION}`;
  const archive = `${binName}.tar.gz`;
  const url = `https://github.com/nuoyazhizhou/tokenslim/releases/download/${tag}/${archive}`;
  const out = path.join(vendorDir(), binName);

  info(`downloading ${url}`);
  await downloadToFile(url, out);
  chmodX(out);
  info(`saved → ${out}`);
}

function alreadyCached(binName) {
  const p = path.join(vendorDir(), binName);
  return fs.existsSync(p);
}

async function tryGithubReleaseFallback() {
  const key = `${process.platform}:${process.arch}`;
  if (!PLATFORM_PKG[key]) return false; // unsupported platform

  const isWin = process.platform === "win32";
  const tokenslim = isWin ? "tokenslim.exe" : "tokenslim";
  const server = isWin ? "tokenslim-server.exe" : "tokenslim-server";

  if (alreadyCached(tokenslim) && alreadyCached(server)) return true;

  try {
    if (!alreadyCached(tokenslim)) {
      await downloadFromGitHubRelease(tokenslim);
    }
    if (!alreadyCached(server)) {
      await downloadFromGitHubRelease(server);
    }
    return true;
  } catch (e) {
    warn(`GitHub Release fallback failed: ${e.message}`);
    warn(
      `the SDK JS still works as a REST client; the \`tokenslim\` / \`tokenslim-server\` commands will be unavailable.`,
    );
    return false;
  }
}

async function main() {
  // (1) The matching optional package brought the binary along.
  const found = findOptionalBinary();
  if (found) {
    info(`platform package found: ${found.pkgName}`);
    const dir = found.binDir;
    for (const name of fs.readdirSync(dir)) {
      const p = path.join(dir, name);
      const st = fs.statSync(p);
      if (st.isFile()) chmodX(p);
    }
    info(`binaries ready in ${dir}`);
    return;
  }

  // (2) GitHub Release fallback. We only attempt this for matching
  // (platform, arch) so users on unusual platforms (FreeBSD, etc.) just
  // see a clean warning instead of a download error.
  info(
    `no platform-specific package for ${process.platform}/${process.arch}; trying GitHub Release v${PKG_VERSION} ...`,
  );
  fs.mkdirSync(vendorDir(), { recursive: true });
  const ok = await tryGithubReleaseFallback();
  if (!ok) {
    warn(
      `skipping binary install; \`tokenslim\` CLI commands won't be available. SDK HTTP client works normally.`,
    );
  }
}

main().catch((e) => {
  warn(`unexpected error: ${e && e.stack ? e.stack : e}`);
  // Never fail npm install on postinstall errors. The user gets the JS
  // SDK and a clear warning; they can rerun with --foreground-scripts
  // to see logs and re-attempt.
  process.exit(0);
});
