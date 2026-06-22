#!/usr/bin/env node
// scripts/build-npm-binary-package.mjs
//
// Stuffs a freshly-built Rust binary + the runtime plugin config into a
// per-platform npm package directory, then `npm pack`s it.  Called by
// `.github/workflows/build-release.yml` once per (os, arch) combination.
//
// Usage (called by CI; not for end users):
//   node scripts/build-npm-binary-package.mjs \
//     --platform linux-x64-gnu \
//     --bin-dir target/release \
//     --config-dir config/plugins
//
// Output: prints the path to the generated .tgz tarball on stdout, last
// line.  Other output goes to stderr so the caller can capture it.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
// Script lives at `<repo>/scripts/build-npm-binary-package.mjs`, so one
// `..` from `__dirname` lands at the repository root.  An earlier draft
// had `"..", ".."` which climbed one level too high and made
// `PACKAGES_DIR` resolve to `<parent of repo>/packages` — that path
// didn't exist in CI, so the script's `npm pack` step failed with
// ENOENT for every platform's `package.json`.  The copy steps
// `mkdir -p`'d `<wrong>/packages/cli-binary-*/bin` and wrote binaries
// to a location that was thrown away, while the real
// `<repo>/packages/cli-binary-*/package.json` was never read.
const REPO_ROOT = path.resolve(__dirname, "..");
const PACKAGES_DIR = path.join(REPO_ROOT, "packages");

// Map the platform identifier the workflow uses → npm package directory.
const PLATFORM_DIR = {
  "linux-x64-gnu": "cli-binary-linux-x64-gnu",
  "linux-arm64-gnu": "cli-binary-linux-arm64-gnu",
  "darwin-x64": "cli-binary-darwin-x64",
  "darwin-arm64": "cli-binary-darwin-arm64",
  "windows-x64": "cli-binary-windows-x64",
  "windows-arm64": "cli-binary-windows-arm64",
};

// Human-readable display name used in the per-platform README.
// Substituted into the template at <repo>/packages/cli-binary-linux-x64-gnu/README.md
// when staging README.md into each platform package.
const PLATFORM_DISPLAY = {
  "linux-x64-gnu": "Linux x64 (glibc)",
  "linux-arm64-gnu": "Linux ARM64 (glibc)",
  "darwin-x64": "macOS x64 (Intel)",
  "darwin-arm64": "macOS ARM64 (Apple Silicon)",
  "windows-x64": "Windows x64",
  "windows-arm64": "Windows ARM64",
};

// Filenames for the two binaries we ship in every platform package.
const EXE = process.platform === "win32" ? ".exe" : "";
const BINARIES = [
  { src: `tokenslim${EXE}`, dst: `tokenslim${EXE}` },
  { src: `tokenslim-server${EXE}`, dst: `tokenslim-server${EXE}` },
];

function parseArgs(argv) {
  const out = { platform: null, binDir: null, configDir: null };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--platform") out.platform = argv[++i];
    else if (a === "--bin-dir") out.binDir = argv[++i];
    else if (a === "--config-dir") out.configDir = argv[++i];
  }
  for (const [k, v] of Object.entries(out)) {
    if (!v) throw new Error(`missing --${k.replace(/([A-Z])/g, "-$1").toLowerCase()}`);
  }
  return out;
}

function whichExe(name) {
  return process.platform === "win32" ? `${name}.cmd` : name;
}

async function rimraf(p) {
  await fs.rm(p, { recursive: true, force: true });
}

async function copyDirContents(src, dst) {
  await fs.mkdir(dst, { recursive: true });
  const entries = await fs.readdir(src, { withFileTypes: true });
  for (const e of entries) {
    const s = path.join(src, e.name);
    const d = path.join(dst, e.name);
    if (e.isDirectory()) await copyDirContents(s, d);
    else if (e.isFile()) await fs.copyFile(s, d);
  }
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const platformDir = PLATFORM_DIR[args.platform];
  if (!platformDir) {
    throw new Error(`unknown platform: ${args.platform}`);
  }
  const pkgRoot = path.join(PACKAGES_DIR, platformDir);

  // 1. Stage the Rust binaries.
  const binDir = path.join(pkgRoot, "bin");
  await rimraf(binDir);
  await fs.mkdir(binDir, { recursive: true });
  for (const { src, dst } of BINARIES) {
    const from = path.join(args.binDir, src);
    const to = path.join(binDir, dst);
    await fs.copyFile(from, to);
    if (process.platform !== "win32") {
      await fs.chmod(to, 0o755);
    }
    process.stderr.write(`copied ${from} → ${to}\n`);
  }

  // 2. Stage the plugin configs.
  //
  // The workflow passes `--config-dir config/plugins` (we only ship
  // plugin configs in the binary, not `frameworks/` / `languages/` /
  // root .toml files which are dev-tree-only).  We must preserve the
  // `plugins/` basename so the staged tree is `pkgRoot/config/plugins/`
  // — both the README (packages/cli-binary-linux-x64-gnu/README.md
  // §"What's inside") and the runtime plugin loader
  // (src/core/plugin_config_loader/mod.rs::find_config_dir, which only
  // searches `config/plugins/` not `config/`) assume that layout.  An
  // earlier revision used `copyDirContents(args.configDir, configDir)`
  // here, which flattened the 67 plugin .json files directly under
  // `pkgRoot/config/` — the resulting tgz had `package/config/*.json`
  // and the runtime's `find_config_dir` returned the default fallback
  // path that didn't exist on disk, so every installed binary loaded
  // zero plugins.  This commit adds the `plugins/` segment back.
  const configDir = path.join(pkgRoot, "config");
  await rimraf(configDir);
  if (args.configDir) {
    const stagedConfigDir = path.join(
      configDir,
      path.basename(args.configDir),
    );
    await copyDirContents(args.configDir, stagedConfigDir);
    process.stderr.write(
      `copied ${args.configDir} → ${stagedConfigDir}\n`,
    );
  }

  // 2.5. Stage README.md.  The repo only commits a template at
  //      packages/cli-binary-linux-x64-gnu/README.md; the other five
  //      platform directories are filled in at pack time.  Each
  //      platform's package.json declares "files": ["bin/", "config/",
  //      "README.md"], so without a real README present `npm pack`
  //      aborts with ENOENT.  Reading the template + substituting the
  //      platform name keeps the message in one place and avoids
  //      hand-editing six near-identical files.
  const readmeTemplatePath = path.join(
    PACKAGES_DIR,
    "cli-binary-linux-x64-gnu",
    "README.md",
  );
  const readmeTemplate = await fs.readFile(readmeTemplatePath, "utf8");
  const readmeStaged = readmeTemplate
    .replaceAll("cli-binary-linux-x64-gnu", `cli-binary-${args.platform}`)
    .replaceAll("Linux x64 (glibc)", PLATFORM_DISPLAY[args.platform]);
  const readmePath = path.join(pkgRoot, "README.md");
  await fs.writeFile(readmePath, readmeStaged, "utf8");
  process.stderr.write(`staged README.md → ${readmePath}\n`);

  // 3. Run `npm pack` and capture the resulting tarball path.
  const npm = whichExe("npm");
  // `npm pack --pack-destination <dir>` requires <dir> to exist; on a
  // fresh CI workspace the `dist/` folder is not created by the build
  // step above (it only makes `artifacts/bin/`), so we mkdir it here.
  // If the folder is already there this is a no-op.
  const distDir = path.join(REPO_ROOT, "dist");
  await fs.mkdir(distDir, { recursive: true });
  const packOut = execFileSync(
    npm,
    ["pack", "--pack-destination", distDir],
    {
      cwd: pkgRoot,
      encoding: "utf8",
      // On Windows, `npm` resolves to `npm.cmd` and Node 18+ refuses
      // to spawn `.cmd`/`.bat` files without `shell: true` (CVE-2024-
      // 27980).  Without this the windows-* jobs fail with
      // `spawnSync npm.cmd EINVAL`.  Linux and macOS don't need it
      // (and we keep it off there so we don't pay the shell-escape
      // cost or risk shell injection from `distDir`).
      shell: process.platform === "win32",
    },
  );
  // `npm pack` (without --json) prints the tarball filename as the last
  // line of stdout.  Trim to handle trailing whitespace.
  const tarballName = packOut.trim().split(/\r?\n/).pop();
  const tarballPath = path.join(distDir, tarballName);
  process.stdout.write(tarballPath + "\n");
}

main().catch((e) => {
  process.stderr.write(`build-npm-binary-package: ${e.stack || e.message}\n`);
  process.exit(1);
});
