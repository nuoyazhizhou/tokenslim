#!/usr/bin/env node
// packages/sdk-nodejs/bin/tokenslim-server.js
//
// Node.js wrapper for the standalone `tokenslim-server` Rust binary.
// Mirrors the resolution logic in bin/tokenslim.js but for the server
// binary. See that file for the full design notes; this one is the
// server-only counterpart.

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const BINARY_NAME =
  process.platform === "win32" ? "tokenslim-server.exe" : "tokenslim-server";

const PLATFORM_PKG = {
  "linux:x64": "@tokenslim/cli-binary-linux-x64-gnu",
  "linux:arm64": "@tokenslim/cli-binary-linux-arm64-gnu",
  "darwin:x64": "@tokenslim/cli-binary-darwin-x64",
  "darwin:arm64": "@tokenslim/cli-binary-darwin-arm64",
  "win32:x64": "@tokenslim/cli-binary-windows-x64",
  "win32:arm64": "@tokenslim/cli-binary-windows-arm64",
};

function locateBinary() {
  const key = `${process.platform}:${process.arch}`;
  const pkgName = PLATFORM_PKG[key];

  if (pkgName) {
    try {
      const pkgPath = require.resolve(`${pkgName}/package.json`, {
        paths: [path.dirname(__filename), path.join(path.dirname(__filename), "..", "..")],
      });
      const binDir = path.join(path.dirname(pkgPath), "bin");
      const candidate = path.join(binDir, BINARY_NAME);
      if (fs.existsSync(candidate)) return candidate;
    } catch {
      // optional dep not installed; fall through.
    }
  }

  const cached = path.join(__dirname, "..", "vendor", BINARY_NAME);
  if (fs.existsSync(cached)) return cached;

  return null;
}

function main() {
  const bin = locateBinary();
  if (!bin) {
    const key = `${process.platform}:${process.arch}`;
    process.stderr.write(
      [
        `tokenslim-server: native binary not found for ${key}.`,
        ``,
        `Fix:`,
        `  1. npm install tokenslim-sdk    (avoid --ignore-optional)`,
        `  2. cargo install tokenslim-server --locked`,
        `  3. Download from https://github.com/nuoyazhizhou/tokenslim/releases`,
        ``,
        `Tip: the main \`tokenslim\` binary also has a \`serve\` subcommand`,
        `if you only need the server side.`,
        ``,
      ].join("\n"),
    );
    process.exit(127);
  }

  const args = process.argv.slice(2);

  const child = spawn(bin, args, {
    stdio: "inherit",
    windowsHide: true,
  });

  child.on("error", (err) => {
    process.stderr.write(`tokenslim-server: failed to exec ${bin}: ${err.message}\n`);
    process.exit(1);
  });

  child.on("close", (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });
}

main();
