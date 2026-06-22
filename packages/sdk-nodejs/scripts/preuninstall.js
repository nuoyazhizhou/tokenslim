#!/usr/bin/env node
// packages/sdk-nodejs/scripts/preuninstall.js
//
// npm `preuninstall` hook. Runs before `npm uninstall -g tokenslim`.
// Removes the shell hook block from PowerShell/Bash profile so that
// the `function npm { tokenslim run npm @args }` wrapper doesn't
// break after the binary is gone.
//
// Best-effort: never fails npm uninstall. If hook removal fails, we
// print a hint for the user to clean up manually.

const { execFileSync } = require("node:child_process");
const path = require("node:path");

function info(msg) {
  process.stdout.write(`[tokenslim/preuninstall] ${msg}\n`);
}

function warn(msg) {
  process.stderr.write(`[tokenslim/preuninstall] WARN: ${msg}\n`);
}

try {
  // 调用 bin/tokenslim.js 包装器，它会找到原生二进制并转发
  const wrapper = path.join(__dirname, "..", "bin", "tokenslim.js");
  info("removing shell hooks from profile...");
  execFileSync(process.execPath, [wrapper, "hooks", "uninstall"], {
    stdio: "inherit",
    timeout: 15000,
  });
  info("shell hooks removed.");
} catch (e) {
  warn(`failed to remove shell hooks: ${e.message}`);
  warn(
    `please manually remove the "# >>> tokenslim hook >>>" block from your shell profile.`,
  );
  // 永不因 hook 清理失败而中断 npm uninstall
  process.exit(0);
}
