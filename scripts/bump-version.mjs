#!/usr/bin/env node
// scripts/bump-version.mjs
//
// One-shot version bumper for a TokenSlim release.  Updates every place
// where a release version is pinned so the `tokenslim` package and
// the 6 platform binary packages stay in lockstep, and keeps the
// `optionalDependencies` block of `tokenslim` pointing at the same
// version of each binary package.
//
// Files updated (all paths are relative to the repo root):
//   - packages/sdk-nodejs/package.json
//   - packages/cli-binary-linux-x64-gnu/package.json
//   - packages/cli-binary-linux-arm64-gnu/package.json
//   - packages/cli-binary-darwin-x64/package.json
//   - packages/cli-binary-darwin-arm64/package.json
//   - packages/cli-binary-windows-x64/package.json
//   - packages/cli-binary-windows-arm64/package.json
//   - Cargo.toml                           (root crate version)
//   - packages/sdk-nodejs/package-lock.json  (regenerated via `npm install` to
//                                            keep the @tokenslim/cli-binary-*
//                                            pins in sync — otherwise the next
//                                            `npm ci` EUSAGEs on the drift)
//
// Files NOT touched:
//   - crates/tokenslim-py/Cargo.toml        (Python binding, separate track)
//   - crates/plugin-interface/Cargo.toml   (interface crate, separate track)
//
// Usage:
//   node scripts/bump-version.mjs 0.2.0
//   node scripts/bump-version.mjs 0.2.0 --dry-run    # show diff, don't write
//   node scripts/bump-version.mjs 0.2.0 --commit     # also git commit -am
//   node scripts/bump-version.mjs --check 0.2.0      # CI gate: exit 1 on drift
//
// Exit code: 0 on success, 1 on validation / IO error.  Under `--check`,
// exit code 1 specifically means at least one file's version does not
// match the requested value.

import { promises as fs } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const REPO_ROOT = path.resolve(__dirname, "..");

// On Windows, `npm` resolves to `npm.cmd` (a batch file) via the
// shell's PATHEXT lookup, but `execFileSync` does NOT do PATHEXT
// resolution — it needs the exact filename.  Hard-coding the `.cmd`
// suffix on win32 lets the same script work on Linux/macOS runners
// (where `npm` is a plain ELF/Mach-O binary) and on Windows dev boxes.
const NPM_CMD = process.platform === "win32" ? "npm.cmd" : "npm";

const SEMVER_RE = /^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/;

const NPM_PACKAGE_FILES = [
  "packages/sdk-nodejs/package.json",
  "packages/cli-binary-linux-x64-gnu/package.json",
  "packages/cli-binary-linux-arm64-gnu/package.json",
  "packages/cli-binary-darwin-x64/package.json",
  "packages/cli-binary-darwin-arm64/package.json",
  "packages/cli-binary-windows-x64/package.json",
  "packages/cli-binary-windows-arm64/package.json",
];

const CARGO_TOML = "Cargo.toml";

function parseArgs(argv) {
  const out = { version: null, dryRun: false, commit: false, check: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--dry-run") out.dryRun = true;
    else if (a === "--commit") out.commit = true;
    else if (a === "--check") out.check = true;
    else if (a === "-h" || a === "--help") {
      printHelp();
      process.exit(0);
    } else if (!a.startsWith("-")) {
      if (out.version !== null) {
        throw new Error(`multiple version arguments given: ${out.version} and ${a}`);
      }
      out.version = a;
    } else {
      throw new Error(`unknown flag: ${a}`);
    }
  }
  if (!out.version) {
    printHelp();
    throw new Error("missing version argument");
  }
  if (!SEMVER_RE.test(out.version)) {
    throw new Error(
      `version "${out.version}" is not a valid semver string (e.g. 0.2.0, 1.0.0-rc.1)`,
    );
  }
  // `--check` is a strict superset of `--dry-run` for our purposes
  // (it never writes), so make the two explicit.
  if (out.check && out.dryRun) {
    throw new Error("`--check` and `--dry-run` are mutually exclusive");
  }
  if (out.check && out.commit) {
    throw new Error("`--check` and `--commit` are mutually exclusive");
  }
  return out;
}

function printHelp() {
  process.stderr.write(
    [
      "Usage: node scripts/bump-version.mjs <version> [--dry-run] [--commit] [--check]",
      "",
      "Bumps every version pin (7 npm packages + root Cargo.toml) and",
      "regenerates packages/sdk-nodejs/package-lock.json so the next",
      "`npm ci` doesn't EUSAGE on a drift.  `--dry-run` skips both the",
      "package.json writes and the lockfile regen (strictly read-only).",
      "",
      "Examples:",
      "  node scripts/bump-version.mjs 0.2.0",
      "  node scripts/bump-version.mjs 0.2.0 --dry-run",
      "  node scripts/bump-version.mjs 0.2.0 --commit",
      "  node scripts/bump-version.mjs --check 0.2.0    # exit 1 if drift",
      "",
    ].join("\n"),
  );
}

/**
 * Find a TOML key and replace its value, preserving the rest of the file
 * byte-for-byte.  Only handles the simple `key = "value"` top-of-line
 * form we use in this repo.
 */
function updateTomlVersion(text, newVersion) {
  const lines = text.split(/\r?\n/);
  let replaced = false;
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(/^(\s*version\s*=\s*")([^"]*)("\s*)$/);
    if (m) {
      lines[i] = `${m[1]}${newVersion}${m[3]}`;
      replaced = true;
      // Stop after the first hit (root package version).  Workspace member
      // crates keep their own versions and we don't want to clobber them.
      break;
    }
  }
  if (!replaced) {
    throw new Error(`no \`version = "..."\` line found in Cargo.toml`);
  }
  return lines.join("\n");
}

async function bumpNpmPackage(relPath, newVersion) {
  const abs = path.join(REPO_ROOT, relPath);
  const text = await fs.readFile(abs, "utf8");

  // Targeted text-rewrite instead of JSON.parse + JSON.stringify so the
  // original formatting (inline arrays, key order) survives byte-for-byte.
  // The package.json files in this repo all use the form
  //   "version": "X.Y.Z"
  // on a line by itself, with optional whitespace.
  const versionRe = /^(\s*)"version"(\s*:\s*)"[^"]*"(\s*,?\s*)$/m;
  const versionMatch = text.match(versionRe);
  if (!versionMatch) {
    throw new Error(`no top-level "version" field in ${relPath}`);
  }
  const oldVersion = (text.match(/"version"\s*:\s*"([^"]+)"/) || [])[1];
  if (oldVersion === newVersion) {
    return { relPath, oldVersion, newVersion, changed: false };
  }

  let newText = text.replace(
    /"version"\s*:\s*"[^"]+"/,
    `"version": "${newVersion}"`,
  );

  // In the SDK's package.json, also keep every `@tokenslim/cli-binary-*`
  // pin in `optionalDependencies` pointing at the new version.  We do
  // this as a string rewrite so we don't have to re-emit the whole file.
  if (relPath === "packages/sdk-nodejs/package.json") {
    newText = newText.replace(
      /("@tokenslim\/cli-binary-[a-z0-9-]+"\s*:\s*)"[^"]+"/g,
      `$1"${newVersion}"`,
    );
  }

  if (!args.dryRun) {
    await fs.writeFile(abs, newText, "utf8");
  }
  return { relPath, oldVersion, newVersion, changed: true };
}

async function bumpCargoToml(newVersion) {
  const abs = path.join(REPO_ROOT, CARGO_TOML);
  const text = await fs.readFile(abs, "utf8");
  const oldMatch = text.match(/^\s*version\s*=\s*"([^"]+)"/m);
  const oldVersion = oldMatch ? oldMatch[1] : null;
  if (oldVersion === newVersion) {
    return { relPath: CARGO_TOML, oldVersion, newVersion, changed: false };
  }
  if (!args.dryRun) {
    const newText = updateTomlVersion(text, newVersion);
    await fs.writeFile(abs, newText, "utf8");
  }
  return { relPath: CARGO_TOML, oldVersion, newVersion, changed: true };
}

async function regenerateSdkLockfile() {
  // The SDK's `optionalDependencies` pin specific @tokenslim/cli-binary-*
  // versions, and after a bump those pins have moved.  The lockfile
  // needs to be regenerated so a subsequent `npm ci` (e.g. in
  // ci-sdk.yml, or in a fresh CI runner that re-uses this checkout)
  // does not EUSAGE on a lockfile / package.json drift.
  //
  // We use `npm install` (not `npm ci`) here, and pass
  // `--ignore-scripts` because:
  //   1. `npm ci` would refuse to run — the lockfile is precisely the
  //      thing we are about to regenerate, and at this point it
  //      describes the *old* version, so `npm ci` would fail.
  //   2. The SDK's `postinstall` script tries to download a platform
  //      binary from a GitHub Release URL (e.g.
  //      `https://github.com/.../releases/download/vX.Y.Z/...`).  At
  //      the moment we run the bump, that release does not exist yet
  //      (we are *preparing* the release), so the download 404s and
  //      wastes ~30 seconds.  The install-time download belongs in
  //      the consumer's `npm install`, not in the publisher's
  //      bump-time regen.
  //   3. We only care about resolving the new optionalDeps into a
  //      fresh lockfile, not about actually running the binary.
  const sdkDir = path.join(REPO_ROOT, "packages/sdk-nodejs");
  process.stdout.write(
    "\nregenerating packages/sdk-nodejs/package-lock.json ...\n",
  );
  // Node ≥18 (CVE-2024-27980) refuses to spawn .cmd / .bat files
  // without `shell: true`.  All our args are simple ASCII flags with
  // no spaces, so routing through cmd.exe on Windows is safe.
  execFileSync(
    NPM_CMD,
    ["install", "--no-audit", "--no-fund", "--ignore-scripts"],
    {
      stdio: "inherit",
      cwd: sdkDir,
      shell: process.platform === "win32",
    },
  );
  // Did the lockfile actually change?  `npm install` is a no-op when
  // the lockfile already satisfies package.json, so we have to ask git.
  // The caller uses this to decide whether to `git add` the file.
  const status = execFileSync(
    "git",
    ["status", "--porcelain", "packages/sdk-nodejs/package-lock.json"],
    { cwd: REPO_ROOT, encoding: "utf8" },
  ).trim();
  return status.length > 0;
}

function printTable(rows) {
  if (rows.length === 0) return;
  const width = Math.max(...rows.map((r) => r.relPath.length));
  for (const r of rows) {
    const arrow = r.changed ? `${r.oldVersion} → ${r.newVersion}` : "unchanged";
    process.stdout.write(`  ${r.relPath.padEnd(width)}  ${arrow}\n`);
  }
}

async function readNpmPackageVersion(relPath) {
  const abs = path.join(REPO_ROOT, relPath);
  const text = await fs.readFile(abs, "utf8");
  // Mirror the regex in `bumpNpmPackage` so the two functions agree on
  // which field they consider "the version".
  const m = text.match(/"version"\s*:\s*"([^"]+)"/);
  if (!m) {
    throw new Error(`no top-level "version" field in ${relPath}`);
  }
  return m[1];
}

async function readCargoTomlVersion() {
  const abs = path.join(REPO_ROOT, CARGO_TOML);
  const text = await fs.readFile(abs, "utf8");
  const m = text.match(/^\s*version\s*=\s*"([^"]+)"/m);
  if (!m) {
    throw new Error(`no \`version = "..."\` line found in ${CARGO_TOML}`);
  }
  return m[1];
}

async function runCheck(expected) {
  // Read every file's current version, compare with the expected value,
  // and print a compact report.  Exit 0 when everything matches, 1
  // otherwise.  Never writes to disk, regardless of `args.dryRun`.
  const rows = [];
  for (const f of NPM_PACKAGE_FILES) {
    rows.push({ relPath: f, current: await readNpmPackageVersion(f) });
  }
  rows.push({ relPath: CARGO_TOML, current: await readCargoTomlVersion() });

  const drifting = rows.filter((r) => r.current !== expected);
  const width = Math.max(...rows.map((r) => r.relPath.length));
  for (const r of rows) {
    const ok = r.current === expected;
    const tag = ok ? "ok  " : "DRIFT";
    process.stdout.write(
      `  [${tag}] ${r.relPath.padEnd(width)}  ${r.current}\n`,
    );
  }
  if (drifting.length === 0) {
    process.stdout.write(
      `\nok: all ${rows.length} files at version ${expected}\n`,
    );
    return 0;
  }
  process.stderr.write(
    `\nFAIL: ${drifting.length}/${rows.length} files do not match ${expected}:\n` +
      drifting.map((r) => `  - ${r.relPath}: ${r.current}`).join("\n") +
      "\n\n" +
      `re-run without --check to rewrite them:\n` +
      `  node scripts/bump-version.mjs ${expected}\n`,
  );
  return 1;
}

const args = parseArgs(process.argv.slice(2));

// `--check` is a read-only fast path used by CI before the heavy
// cross-platform build kicks off.  Branch out before any write-capable
// helper is invoked.
if (args.check) {
  process.exit(await runCheck(args.version));
}

const npmResults = [];
for (const f of NPM_PACKAGE_FILES) {
  npmResults.push(await bumpNpmPackage(f, args.version));
}
const cargoResult = await bumpCargoToml(args.version);

const all = [...npmResults, cargoResult];
const anyChanged = all.some((r) => r.changed);

process.stdout.write(
  `\n${args.dryRun ? "DRY RUN — no files written\n" : ""}version = ${args.version}\n\n`,
);
printTable(all);

if (!anyChanged) {
  process.stdout.write("\n(no changes; already at this version)\n");
  process.exit(0);
}

// After the version bumps, regenerate the SDK's package-lock.json so
// the @tokenslim/cli-binary-* pins in `optionalDependencies` resolve
// to the new versions.  Without this step the lockfile stays at the
// previous version and the next `npm ci` (e.g. in ci-sdk.yml) EUSAGEs
// on the drift.  We also gate the regen on `--dry-run` so dry-runs
// stay strictly read-only.
let lockfileChanged = false;
if (!args.dryRun) {
  lockfileChanged = await regenerateSdkLockfile();
  if (lockfileChanged) {
    process.stdout.write(
      "  packages/sdk-nodejs/package-lock.json  regenerated\n",
    );
  } else {
    process.stdout.write(
      "  packages/sdk-nodejs/package-lock.json  unchanged\n",
    );
  }
}

if (args.commit) {
  if (args.dryRun) {
    process.stdout.write("\n--commit ignored under --dry-run\n");
  } else {
    const files = all.filter((r) => r.changed).map((r) => r.relPath);
    // Pull the regenerated lockfile in if the regen actually changed
    // it.  We only add it explicitly (rather than `git add -A`) so we
    // never accidentally commit a stray file the user hadn't reviewed.
    if (lockfileChanged && !files.includes("packages/sdk-nodejs/package-lock.json")) {
      files.push("packages/sdk-nodejs/package-lock.json");
    }
    process.stdout.write(`\ngit adding ${files.length} files...\n`);
    execFileSync("git", ["add", ...files], { stdio: "inherit", cwd: REPO_ROOT });
    const msg = `chore(release): bump version to ${args.version}`;
    execFileSync("git", ["commit", "-m", msg], {
      stdio: "inherit",
      cwd: REPO_ROOT,
      env: { ...process.env, GIT_AUTHOR_NAME: "tokenslim-bot", GIT_AUTHOR_EMAIL: "bot@tokenslim.local", GIT_COMMITTER_NAME: "tokenslim-bot", GIT_COMMITTER_EMAIL: "bot@tokenslim.local" },
    });
    process.stdout.write(`\ncommitted as: ${msg}\n`);
  }
}

process.stdout.write(
  `\nnext: review the diff, then\n` +
    `  git push origin main\n` +
    `  git tag v${args.version}\n` +
    `  git push origin v${args.version}\n\n`,
);
