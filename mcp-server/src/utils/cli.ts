/**
 * TokenSlim CLI 调用封装
 * 负责定位 tokenslim 可执行文件、校验可用性，并通过 child_process 与其交互。
 */

import { execFile, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { promisify } from "node:util";
import type {
  CliJsonResponse,
  CompressionOutputData,
  DecompressionData,
} from "./types.js";

const execFileAsync = promisify(execFile);

/** 缓存的 CLI 可执行文件路径 */
let cachedCliPath: string | undefined;

/** 缓存的项目根目录 */
let cachedProjectRoot: string | undefined;

/**
 * 查找 TokenSlim 项目根目录（包含 Cargo.toml 的目录）。
 * 优先从当前工作目录向上回溯，若找不到且已知 CLI 路径，则尝试从 CLI 路径回溯。
 * 默认回退到当前工作目录。
 */
export function findProjectRoot(cliPath?: string): string {
  if (cachedProjectRoot) return cachedProjectRoot;

  const searchFrom = (startDir: string): string | undefined => {
    let dir = startDir;
    for (let i = 0; i < 10; i++) {
      if (existsSync(resolve(dir, "Cargo.toml"))) {
        return dir;
      }
      const parent = dirname(dir);
      if (parent === dir) break;
      dir = parent;
    }
    return undefined;
  };

  // 1) 从当前工作目录查找
  const fromCwd = searchFrom(process.cwd());
  if (fromCwd) {
    cachedProjectRoot = fromCwd;
    return fromCwd;
  }

  // 2) 从 CLI 所在路径回溯（适用于全局安装或 mcp-server 子目录运行场景）
  if (cliPath) {
    const fromCli = searchFrom(dirname(cliPath));
    if (fromCli) {
      cachedProjectRoot = fromCli;
      return fromCli;
    }
  }

  cachedProjectRoot = process.cwd();
  return cachedProjectRoot;
}

/**
 * 在 PATH、常见位置以及项目 target/release 中查找 tokenslim 可执行文件。
 * Windows 下优先尝试 .exe 扩展名。
 */
export async function findTokenslimCli(): Promise<string> {
  if (cachedCliPath) return cachedCliPath;

  const candidates: string[] = [];
  const isWin = process.platform === "win32";

  // 1) PATH 中的命令
  candidates.push("tokenslim");
  if (isWin) candidates.push("tokenslim.exe");

  // 2) 当前项目 target/release 目录（便于本地开发调试）
  const projectRoot = resolve(process.cwd(), "..");
  candidates.push(
    resolve(projectRoot, "target", "release", isWin ? "tokenslim.exe" : "tokenslim"),
    resolve(projectRoot, "target", "debug", isWin ? "tokenslim.exe" : "tokenslim"),
    resolve(process.cwd(), "target", "release", isWin ? "tokenslim.exe" : "tokenslim"),
    resolve(process.cwd(), "target", "debug", isWin ? "tokenslim.exe" : "tokenslim")
  );

  // 3) 尝试直接执行并获取版本，第一个成功返回的即为可用路径
  for (const candidate of candidates) {
    try {
      await execFileAsync(candidate, ["--version"], { timeout: 5000 });
      cachedCliPath = candidate;
      return candidate;
    } catch {
      continue;
    }
  }

  throw new Error(
    "未找到可用的 tokenslim CLI。请确保 tokenslim 已安装并在 PATH 中，或已编译到 target/release/tokenslim。"
  );
}

/**
 * 启动时检查 CLI 是否可用，返回版本字符串。
 */
export async function checkCliVersion(cliPath: string): Promise<string> {
  const { stdout } = await execFileAsync(cliPath, ["--version"], { timeout: 5000 });
  return stdout.trim();
}

/**
 * 执行不需要向 stdin 写入数据的 CLI 命令，返回原始字符串输出。
 */
export async function execCli(
  cliPath: string,
  args: string[],
  options: { cwd?: string; timeout?: number } = {}
): Promise<string> {
  const { stdout } = await execFileAsync(cliPath, args, {
    // 默认使用项目根目录作为 cwd，确保 CLI 能正确读取 config/ 等内置资源
    cwd: options.cwd ?? findProjectRoot(cachedCliPath),
    timeout: options.timeout ?? 60000,
    maxBuffer: 50 * 1024 * 1024, // 50MB，允许处理较大日志
  });
  return stdout;
}

/**
 * 执行需要向 stdin 写入数据的 CLI 命令，返回原始字符串输出。
 * 使用 spawn 避免 shell 注入，并正确传递二进制安全文本。
 */
export async function execCliWithStdin(
  cliPath: string,
  args: string[],
  input: string,
  options: { cwd?: string; timeout?: number } = {}
): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = spawn(cliPath, args, {
      // 默认使用项目根目录作为 cwd，确保 CLI 能正确读取 config/ 等内置资源
      cwd: options.cwd ?? findProjectRoot(cachedCliPath),
      stdio: ["pipe", "pipe", "pipe"],
    });

    let stdout = "";
    let stderr = "";
    const timeout = options.timeout ?? 60000;
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      reject(new Error(`tokenslim CLI 执行超时（${timeout}ms）`));
    }, timeout);

    child.stdout.on("data", (chunk: Buffer) => {
      stdout += chunk.toString("utf-8");
    });
    child.stderr.on("data", (chunk: Buffer) => {
      stderr += chunk.toString("utf-8");
    });

    child.on("error", (err) => {
      clearTimeout(timer);
      reject(err);
    });

    child.on("close", (code) => {
      clearTimeout(timer);
      if (code !== 0) {
        reject(new Error(`tokenslim CLI 退出码 ${code}：${stderr || stdout}`));
        return;
      }
      resolve(stdout);
    });

    child.stdin.write(input, "utf-8", (err) => {
      if (err) {
        reject(err);
        return;
      }
      child.stdin.end();
    });
  });
}

/**
 * 解析 CLI 在 --json 模式下输出的 JSON 包装结构。
 */
export function parseCliJson<T>(raw: string): CliJsonResponse<T> {
  const trimmed = raw.trim();
  if (!trimmed) {
    throw new Error("tokenslim CLI 返回空输出");
  }
  try {
    return JSON.parse(trimmed) as CliJsonResponse<T>;
  } catch (err) {
    throw new Error(`解析 CLI JSON 输出失败：${err instanceof Error ? err.message : String(err)}`);
  }
}

/**
 * 调用压缩命令，返回原始 CompressionOutput 数据。
 */
export async function callCompress(
  cliPath: string,
  text: string,
  mode?: string
): Promise<CliJsonResponse<CompressionOutputData>> {
  const args = ["compress", "--json"];
  if (mode) {
    args.push("--preset", mode);
  }
  const raw = await execCliWithStdin(cliPath, args, text);
  return parseCliJson<CompressionOutputData>(raw);
}

/**
 * 调用文件压缩命令。
 */
export async function callCompressFile(
  cliPath: string,
  filePath: string,
  mode?: string
): Promise<CliJsonResponse<CompressionOutputData>> {
  const args = ["compress", "--json", "-i", filePath];
  if (mode) {
    args.push("--preset", mode);
  }
  const raw = await execCli(cliPath, args);
  return parseCliJson<CompressionOutputData>(raw);
}

/**
 * 调用解压命令，要求传入原始 CompressionOutput JSON（不含 status/data 包装）。
 */
export async function callDecompress(
  cliPath: string,
  compressedJson: string
): Promise<CliJsonResponse<DecompressionData>> {
  const raw = await execCliWithStdin(cliPath, ["decompress", "--json"], compressedJson);
  return parseCliJson<DecompressionData>(raw);
}

/**
 * 获取插件列表文本输出。
 */
export async function callPlugins(cliPath: string): Promise<string> {
  return execCli(cliPath, ["plugins"]);
}

/**
 * 获取环境/配置 JSON 输出。
 */
export async function callEnvJson(cliPath: string): Promise<Record<string, unknown>> {
  const raw = await execCli(cliPath, ["env", "--format", "json"]);
  return JSON.parse(raw) as Record<string, unknown>;
}
