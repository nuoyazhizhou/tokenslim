import * as vscode from 'vscode';
import * as cp from 'child_process';
import * as path from 'path';
import * as fs from 'fs';
import * as http from 'http';

const SERVER_PORT = 10086;
const SERVER_URL = `http://127.0.0.1:${SERVER_PORT}`;
let serverProcess: cp.ChildProcess | undefined;

export function activate(context: vscode.ExtensionContext) {
    console.log('TokenSlim (REST API mode) is now active!');

    const compressCurrentFile = vscode.commands.registerCommand('tokenslim.compressCurrentFile', async () => {
        const editor = vscode.window.activeTextEditor;
        if (!editor) {
            vscode.window.showErrorMessage('No active file to compress.');
            return;
        }

        const text = editor.document.getText();
        await compressAndShow(text);
    });

    const compressSelection = vscode.commands.registerCommand('tokenslim.compressSelection', async () => {
        const editor = vscode.window.activeTextEditor;
        if (!editor) return;

        const selection = editor.selection;
        const text = editor.document.getText(selection);
        
        if (!text || text.trim().length === 0) {
            vscode.window.showInformationMessage('No text selected to compress.');
            return;
        }

        await compressAndShow(text);
    });

    const restartServer = vscode.commands.registerCommand('tokenslim.restartServer', async () => {
        await startServer(context);
    });

    context.subscriptions.push(compressCurrentFile, compressSelection, restartServer);
    
    // Initial server start check
    ensureServerRunning(context);
}

async function ensureServerRunning(context: vscode.ExtensionContext) {
    try {
        await makeRequest('GET', '/health');
        console.log('TokenSlim server is already running.');
    } catch (e) {
        // Server not running
        await startServer(context);
    }
}

async function startServer(context: vscode.ExtensionContext) {
    if (serverProcess) {
        serverProcess.kill();
    }

    const binPath = path.join(context.extensionPath, '..', 'target', 'release', 'tokenslim-server.exe');
    if (!fs.existsSync(binPath)) {
        vscode.window.showErrorMessage(`TokenSlim Server binary not found at ${binPath}. Please run 'cargo build --release --bin tokenslim-server'.`);
        return;
    }

    serverProcess = cp.spawn(binPath, {
        detached: true,
        stdio: 'ignore'
    });
    serverProcess.unref();

    // Wait a bit for server to start
    setTimeout(async () => {
        try {
            await makeRequest('GET', '/health');
            vscode.window.showInformationMessage('TokenSlim Sidecar Server started successfully.');
        } catch (e) {
            console.error('Server start verification failed', e);
        }
    }, 1000);
}

function makeRequest(method: string, path: string, body?: any): Promise<any> {
    return new Promise((resolve, reject) => {
        const data = body ? JSON.stringify(body) : '';
        const options = {
            hostname: '127.0.0.1',
            port: SERVER_PORT,
            path: path,
            method: method,
            headers: {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(data)
            }
        };

        const req = http.request(options, (res) => {
            let resData = '';
            res.on('data', (chunk) => resData += chunk);
            res.on('end', () => {
                if (res.statusCode && res.statusCode >= 200 && res.statusCode < 300) {
                    try {
                        resolve(resData ? JSON.parse(resData) : {});
                    } catch (e) {
                        reject(new Error(`Failed to parse response: ${e}`));
                    }
                } else {
                    reject(new Error(`Server returned status ${res.statusCode}: ${resData}`));
                }
            });
        });

        req.on('error', (e) => {
            reject(e);
        });

        if (data) {
            req.write(data);
        }
        req.end();
    });
}

async function compressAndShow(text: string) {
    vscode.window.withProgress({
        location: vscode.ProgressLocation.Notification,
        title: "TokenSlim",
        cancellable: false
    }, async (progress) => {
        progress.report({ message: "Sending to TokenSlim API..." });

        try {
            const json = await makeRequest('POST', '/compress', { text });
            
            const doc = await vscode.workspace.openTextDocument({
                content: JSON.stringify(json, null, 2),
                language: 'json'
            });
            
            await vscode.window.showTextDocument(doc, { viewColumn: vscode.ViewColumn.Beside });

            const meta = json.metadata;
            if (meta) {
                const ratio = ((meta.compressed_size / meta.original_size) * 100).toFixed(2);
                vscode.window.showInformationMessage(`TokenSlim Success: ${ratio}% payload ratio.`);
            }
        } catch (e: any) {
            vscode.window.showErrorMessage(`TokenSlim API Error: ${e.message}. Try "TokenSlim: Restart Server" if it's offline.`);
        }
    });
}

export function deactivate() {
    if (serverProcess) {
        serverProcess.kill();
    }
}
