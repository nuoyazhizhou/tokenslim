#!/usr/bin/env node
/**
 * tokenslim-mcp-server 入口
 * 注册 MCP tools/resources，启动 stdio 传输服务。
 */

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListResourcesRequestSchema,
  ListToolsRequestSchema,
  ReadResourceRequestSchema,
} from "@modelcontextprotocol/sdk/types.js";
import {
  CONFIG_RESOURCE_NAME,
  CONFIG_RESOURCE_URI,
  readConfigResource,
} from "./resources/config.js";
import {
  PLUGINS_RESOURCE_NAME,
  PLUGINS_RESOURCE_URI,
  readPluginsResource,
} from "./resources/plugins.js";
import {
  compressInputSchema,
  compressTool,
  handleCompress,
} from "./tools/compress.js";
import {
  compressFileInputSchema,
  compressFileTool,
  handleCompressFile,
} from "./tools/compress-file.js";
import {
  decompressInputSchema,
  decompressTool,
  handleDecompress,
} from "./tools/decompress.js";
import {
  handleSmartCompress,
  smartCompressInputSchema,
  smartCompressTool,
} from "./tools/smart-compress.js";
import { handleStats, statsInputSchema, statsTool } from "./tools/stats.js";
import { checkCliVersion, findTokenslimCli } from "./utils/cli.js";

/**
 * 主函数：初始化 CLI 连接并启动 MCP Server。
 */
async function main() {
  // 定位并校验 TokenSlim CLI
  let cliPath: string;
  let cliVersion: string;
  try {
    cliPath = await findTokenslimCli();
    cliVersion = await checkCliVersion(cliPath);
    // 使用 stderr 输出诊断信息，避免污染 stdout 上的 MCP 消息
    console.error(`[tokenslim-mcp-server] CLI: ${cliPath} (${cliVersion})`);
  } catch (err) {
    console.error(
      `[tokenslim-mcp-server] 启动失败：${err instanceof Error ? err.message : String(err)}`
    );
    process.exit(1);
  }

  // 创建 MCP Server，声明具备 tools 与 resources 能力
  const server = new Server(
    {
      name: "tokenslim-mcp-server",
      version: "0.1.0",
    },
    {
      capabilities: {
        tools: {},
        resources: {},
      },
    }
  );

  // 注册 tools 列表
  server.setRequestHandler(ListToolsRequestSchema, async () => {
    return {
      tools: [
        compressTool,
        decompressTool,
        compressFileTool,
        smartCompressTool,
        statsTool,
      ],
    };
  });

  // 注册 tool 调用分发
  server.setRequestHandler(CallToolRequestSchema, async (request) => {
    const { name, arguments: args } = request.params;
    try {
      switch (name) {
        case "compress": {
          const input = compressInputSchema.parse(args);
          return await handleCompress(cliPath, input);
        }
        case "decompress": {
          const input = decompressInputSchema.parse(args);
          return await handleDecompress(cliPath, input);
        }
        case "compress_file": {
          const input = compressFileInputSchema.parse(args);
          return await handleCompressFile(cliPath, input);
        }
        case "smart_compress": {
          const input = smartCompressInputSchema.parse(args);
          return await handleSmartCompress(cliPath, input);
        }
        case "stats": {
          statsInputSchema.parse(args ?? {});
          return await handleStats();
        }
        default:
          throw new Error(`未知 tool：${name}`);
      }
    } catch (err) {
      // 将错误以 MCP 文本内容形式返回，避免直接抛异常断开连接
      const message = err instanceof Error ? err.message : String(err);
      return {
        content: [{ type: "text", text: JSON.stringify({ status: "error", error: message }) }],
        isError: true,
      };
    }
  });

  // 注册 resources 列表
  server.setRequestHandler(ListResourcesRequestSchema, async () => {
    return {
      resources: [
        {
          uri: CONFIG_RESOURCE_URI,
          name: CONFIG_RESOURCE_NAME,
          mimeType: "application/json",
        },
        {
          uri: PLUGINS_RESOURCE_URI,
          name: PLUGINS_RESOURCE_NAME,
          mimeType: "application/json",
        },
      ],
    };
  });

  // 注册 resource 读取分发
  server.setRequestHandler(ReadResourceRequestSchema, async (request) => {
    const { uri } = request.params;
    try {
      switch (uri) {
        case CONFIG_RESOURCE_URI:
          return { contents: [await readConfigResource(cliPath)] };
        case PLUGINS_RESOURCE_URI:
          return { contents: [await readPluginsResource(cliPath)] };
        default:
          throw new Error(`未知 resource URI：${uri}`);
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      throw new Error(`读取 resource 失败：${message}`);
    }
  });

  // 通过 stdio 与宿主（Claude Code / Cursor / Windsurf 等）通信
  const transport = new StdioServerTransport();
  await server.connect(transport);
  console.error("[tokenslim-mcp-server] stdio 传输已启动，等待 MCP 请求…");
}

main().catch((err) => {
  console.error(
    `[tokenslim-mcp-server] 未捕获异常：${err instanceof Error ? err.message : String(err)}`
  );
  process.exit(1);
});
