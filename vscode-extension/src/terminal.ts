/**
 * TokenSlim VS Code 扩展 - 终端输出拦截与压缩
 * 监听集成终端的输出，当输出超长时自动压缩并在 Output Channel 中显示压缩版。
 */

import * as vscode from 'vscode';

const AUTO_COMPRESS_THRESHOLD = 5000; // 字符数

export class TerminalCompressor {
  private outputChannel: vscode.OutputChannel;
  private writeEmitter: vscode.EventEmitter<string>;
  private pty: vscode.Pseudoterminal | undefined;

  constructor(
    private makeRequest: (method: string, path: string, body?: any) => Promise<any>
  ) {
    this.outputChannel = vscode.window.createOutputChannel('TokenSlim Compressed');
    this.writeEmitter = new vscode.EventEmitter<string>();
  }

  /**
   * 注册终端数据写入监听器。
   * 当终端输出超过阈值时，自动压缩并在 Output Channel 中显示结果。
   */
  register(context: vscode.ExtensionContext): void {
    const terminalBuffer = new Map<string, string>();

    // 监听终端输出
    const writeDisposable = vscode.window.onDidWriteTerminalData(async (e) => {
      const terminalName = e.terminal.name;
      const current = terminalBuffer.get(terminalName) || '';
      const updated = current + e.data;
      terminalBuffer.set(terminalName, updated);

      // 防抖：积累 500ms 后处理
      setTimeout(async () => {
        const buffered = terminalBuffer.get(terminalName) || '';
        if (buffered.length >= AUTO_COMPRESS_THRESHOLD) {
          terminalBuffer.set(terminalName, '');
          await this.compressAndShow(terminalName, buffered);
        }
      }, 500);
    });

    context.subscriptions.push(writeDisposable);
  }

  /**
   * 压缩终端输出并在 Output Channel 中显示。
   */
  private async compressAndShow(terminalName: string, text: string): Promise<void> {
    try {
      const result = await this.makeRequest('POST', '/compress', { text });
      const meta = result.metadata;
      const ratio = meta ? ((meta.compressed_size / meta.original_size) * 100).toFixed(1) : '?';

      this.outputChannel.appendLine(`--- Terminal: ${terminalName} ---`);
      this.outputChannel.appendLine(`Original: ${text.length} chars | Compressed: ${ratio}%`);
      this.outputChannel.appendLine(JSON.stringify(result, null, 2));
      this.outputChannel.appendLine('');

      // 显示信息提示
      vscode.window.showInformationMessage(
        `TokenSlim: Terminal "${terminalName}" output compressed (${ratio}% ratio)`
      );
    } catch (err: any) {
      // 压缩失败时静默处理，不影响终端使用
      console.warn('[TokenSlim] Terminal compression failed:', err.message);
    }
  }

  /**
   * 手动触发：压缩指定终端的当前全部输出。
   */
  async compressTerminalOutput(terminal: vscode.Terminal): Promise<void> {
    // VS Code API 不直接提供终端历史输出，使用 shell integration 或提示用户
    vscode.window.showInformationMessage(
      'TokenSlim: Terminal auto-compression is active. Long outputs will be compressed automatically.'
    );
  }

  dispose(): void {
    this.outputChannel.dispose();
  }
}
