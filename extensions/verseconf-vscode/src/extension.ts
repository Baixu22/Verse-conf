import * as vscode from 'vscode';
import { LanguageClient, ServerOptions, TransportKind } from 'vscode-languageclient/node';

import { ensureExecutable, findServerBinary, serverBinaryRelativePath } from './serverPath';

let client: LanguageClient | undefined;

export function activate(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('verseconf');
    const lspEnabled = config.get<boolean>('lsp.enabled', true);

    if (lspEnabled) {
        startLSP(context);
    }

    registerCommands(context);
}

/**
 * 解析语言服务器路径，按优先级：
 *   1. 用户在设置里显式指定的 `verseconf.lsp.serverPath`
 *   2. 扩展包内与当前平台/架构匹配的二进制
 *   3. 扩展包内不带平台前缀的二进制（手工放置的兜底）
 */
export function resolveServerPath(context: vscode.ExtensionContext): string | undefined {
    const configured = vscode.workspace
        .getConfiguration('verseconf')
        .get<string>('lsp.serverPath', '')
        .trim();
    if (configured !== '') {
        return configured;
    }

    return findServerBinary(context.extensionPath);
}

function startLSP(context: vscode.ExtensionContext) {
    const serverPath = resolveServerPath(context);

    if (serverPath === undefined) {
        vscode.window.showWarningMessage(
            `VerseConf: 找不到当前平台的语言服务器（期望 ${serverBinaryRelativePath()}）。` +
                '可设置 verseconf.lsp.serverPath 指向自建二进制，或安装带该平台二进制的扩展包。'
        );
        return;
    }

    ensureExecutable(serverPath);

    const serverOptions: ServerOptions = {
        run: {
            command: serverPath,
            transport: TransportKind.stdio
        },
        debug: {
            command: serverPath,
            transport: TransportKind.stdio
        }
    };

    const clientOptions = {
        documentSelector: [
            { scheme: 'file', language: 'verseconf' },
            { scheme: 'file', pattern: '**/*.vcf' }
        ],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.vcf')
        }
    };

    client = new LanguageClient('verseconf-lsp', 'VerseConf Language Server', serverOptions, clientOptions);

    client.start().catch((err: Error) => {
        vscode.window.showWarningMessage(`VerseConf LSP 启动失败（${serverPath}）：${err.message}`);
    });
}

function registerCommands(context: vscode.ExtensionContext) {
    context.subscriptions.push(
        vscode.commands.registerCommand('verseconf.format', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) { return; }

            try {
                await vscode.commands.executeCommand('editor.action.formatDocument');
            } catch (err: any) {
                vscode.window.showErrorMessage(`Format failed: ${err.message}`);
            }
        }),

        vscode.commands.registerCommand('verseconf.validate', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) { return; }

            vscode.window.showInformationMessage('Running VerseConf validation...');
        }),

        vscode.commands.registerCommand('verseconf.schema.generate', async () => {
            vscode.window.showInformationMessage('Schema generation coming soon!');
        })
    );
}

export function deactivate(): Thenable<void> | undefined {
    if (client) {
        return client.stop();
    }
    return undefined;
}
