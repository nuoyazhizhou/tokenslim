#!/usr/bin/env node
// packages/sdk-nodejs/bin/tokenslim.js
//
// Node.js wrapper for the `tokenslim` Rust CLI binary.
//
// Why a wrapper?  We use npm `optionalDependencies` (per-platform packages
// like @tokenslim/cli-binary-linux-x64-gnu) so the platform-matching binary
// is installed into node_modules. This wrapper locates that binary at
// runtime, ensures it is executable, and re-execs it with the original
// argv.  The end user just sees a normal CLI: `tokenslim --version`,
// `tokenslim run -- git status`, etc.
//
// Resolution order:
//   1. The platform-specific @tokenslim/cli-binary-* package (preferred;
//      ships the binary + 60+ plugin configs).
//   2. A binary cached by scripts/postinstall.js (GitHub Releases fallback).
//   3. A system `tokenslim` on PATH (developer escape hatch).

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const BINARY_NAME = process.platform === "win32" ? "tokenslim.exe" : "tokenslim";

// Map Node's (platform, arch) → optionalDependency package name we ship.
const PLATFORM_PKG = {
  "linux:x64": "@tokenslim/cli-binary-linux-x64-gnu",
  "linux:arm64": "@tokenslim/cli-binary-linux-arm64-gnu",
  "darwin:x64": "@tokenslim/cli-binary-darwin-x64",
  "darwin:arm64": "@tokenslim/cli-binary-darwin-arm64",
  "win32:x64": "@tokenslim/cli-binary-windows-x64",
  "win32:arm64": "@tokenslim/cli-binary-windows-arm64",
};

/**
 * Locate the tokenslim binary, trying (1) the platform-specific package,
 * (2) the postinstall cache, (3) PATH. Returns absolute path or null.
 */
function locateBinary() {
  const key = `${process.platform}:${process.arch}`;
  const pkgName = PLATFORM_PKG[key];

  // (1) Per-platform optional dep, resolved from this file's location so it
  // works whether the package is installed globally or locally.
  if (pkgName) {
    try {
      // require.resolve walks node_modules upward; throws if not installed.
      const pkgPath = require.resolve(`${pkgName}/package.json`, {
        paths: [path.dirname(__filename), path.join(path.dirname(__filename), "..", "..")],
      });
      const binDir = path.join(path.dirname(pkgPath), "bin");
      const candidate = path.join(binDir, BINARY_NAME);
      if (fs.existsSync(candidate)) return candidate;
    } catch {
      // optional dep not installed (e.g. yarn with --ignore-optional); fall through.
    }
  }

  // (2) Cache populated by scripts/postinstall.js (GitHub Releases download).
  const cached = path.join(__dirname, "..", "vendor", BINARY_NAME);
  if (fs.existsSync(cached)) return cached;

  // (3) System PATH (developer / test mode).
  return null;
}

function main() {
  const bin = locateBinary();
  if (!bin) {
    const key = `${process.platform}:${process.arch}`;
    const pkg = PLATFORM_PKG[key];
    process.stderr.write(
      [
        `tokenslim: native binary not found for ${key}.`,
        pkg
          ? `Tried optional package "${pkg}" and the postinstall cache.`
          : `No prebuilt package for ${key}.`,
        ``,
        `Fix:`,
        pkg
          ? `  1. Re-run: npm install tokenslim    (avoid --ignore-optional)`
          : `  1. Use a supported platform, OR`,
        `  2. Install Rust and run: cargo install tokenslim --locked`,
        `  3. Or download from https://github.com/nuoyazhizhou/tokenslim/releases`,
        ``,
      ].join("\n"),
    );
    process.exit(127);
  }

  // Forward argv[2..] (skip `node`, the wrapper path).
  const args = process.argv.slice(2);

  const child = spawn(bin, args, {
    stdio: "inherit",
    // Detach so signals (Ctrl-C) propagate to the child correctly on all
    // platforms. Without this, the wrapper often swallows SIGINT.
    windowsHide: true,
  });

  child.on("error", (err) => {
    process.stderr.write(`tokenslim: failed to exec ${bin}: ${err.message}\n`);
    process.exit(1);
  });

  // Forward exit code. Use `close` rather than `exit` so stdio streams are
  // fully flushed before we hand control back to the shell.
  child.on("close", (code, signal) => {
    if (signal) {
      // Re-raise the signal so the parent shell sees the right status.
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });
}

main();
