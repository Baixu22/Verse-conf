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

/** 语言服务器是否已经就绪。三个命令都依赖它，未就绪时必须如实告知而不是静默失败。 */
function isServerRunning(): boolean {
    return client !== undefined && client.isRunning();
}

function serverUnavailable(action: string): string {
    return (
        `VerseConf: 语言服务器未运行，无法${action}。` +
        '请检查 verseconf.lsp.enabled 是否为 true，以及 verseconf.lsp.serverPath 是否指向可用的二进制。'
    );
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
        // 真正的格式化由语言服务器的 documentFormattingProvider 提供
        // （复用核心库的保注释格式化器），这里只负责触发并把结果如实反馈给用户。
        vscode.commands.registerCommand('verseconf.format', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                return;
            }
            if (!isServerRunning()) {
                vscode.window.showWarningMessage(serverUnavailable('格式化'));
                return;
            }

            const before = editor.document.getText();
            try {
                await vscode.commands.executeCommand('editor.action.formatDocument');
            } catch (err: any) {
                vscode.window.showErrorMessage(`VerseConf: 格式化失败：${err?.message ?? err}`);
                return;
            }

            const after = editor.document.getText();
            if (after === before) {
                // 两种可能：本来就规范，或者文档无法解析（服务端此时不做任何修改）。
                // 不谎报"已格式化"，把判断权交给用户。
                vscode.window.showInformationMessage(
                    'VerseConf: 没有需要调整的格式（文档已规范，或因存在语法错误而未改动）。'
                );
            }
        }),

        // 校验结果直接来自语言服务器发布的诊断——就是编辑器里实时看到的那一份，
        // 不另起一条命令行通路，避免两套结果不一致。
        vscode.commands.registerCommand('verseconf.validate', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                return;
            }
            if (!isServerRunning()) {
                vscode.window.showWarningMessage(serverUnavailable('校验'));
                return;
            }

            const diagnostics = vscode.languages
                .getDiagnostics(editor.document.uri)
                .filter((d) => d.source === 'verseconf');

            if (diagnostics.length === 0) {
                vscode.window.showInformationMessage('VerseConf: 未发现问题。');
                return;
            }

            const first = diagnostics[0];
            const line = first.range.start.line + 1;
            const column = first.range.start.character + 1;
            const choice = await vscode.window.showErrorMessage(
                `VerseConf: 发现 ${diagnostics.length} 个问题。首个在 ${line}:${column} —— ${first.message}`,
                '显示问题面板'
            );
            if (choice === '显示问题面板') {
                await vscode.commands.executeCommand('workbench.actions.view.problems');
            }
        }),

        // 由当前文档推断 #@schema 块。推断在语言服务器里做（同一个二进制、同一条通道），
        // 扩展不需要额外依赖任何命令行工具。
        vscode.commands.registerCommand('verseconf.schema.generate', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                return;
            }
            if (!isServerRunning()) {
                vscode.window.showWarningMessage(serverUnavailable('生成 schema'));
                return;
            }

            const text = editor.document.getText();

            // 一个文件里出现两份 #@schema 会直接解析失败，所以先挡住。
            if (text.includes('#@schema')) {
                vscode.window.showWarningMessage(
                    'VerseConf: 当前文档已经包含 #@schema 块。请先移除它再生成，避免一个文件里出现两份 schema。'
                );
                return;
            }

            let schema: string;
            try {
                const result = await client!.sendRequest<{ schema: string }>('verseconf/generateSchema', {
                    text
                });
                schema = result.schema;
            } catch (err: any) {
                vscode.window.showErrorMessage(
                    `VerseConf: 无法生成 schema —— ${err?.message ?? err}（文档必须能通过解析）`
                );
                return;
            }

            const inserted = await editor.edit((builder) => {
                builder.insert(new vscode.Position(0, 0), `${schema}\n`);
            });

            if (!inserted) {
                vscode.window.showErrorMessage('VerseConf: 插入 schema 失败（编辑器拒绝了这次编辑）。');
            }
        })
    );
}

export function deactivate(): Thenable<void> | undefined {
    if (client) {
        return client.stop();
    }
    return undefined;
}
